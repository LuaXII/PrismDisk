use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::OnceLock;

use rayon::prelude::*;
use xxhash_rust::xxh3::Xxh3;

use crate::cache::{Reader, Record, Store};

const EDGE: u64 = 64 * 1024;

#[derive(Default)]
pub struct Job {
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Job {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct FileRef {
    pub id: u32,
    pub path: String,
    pub size: u64,
    pub modified: u64,
}

#[derive(Clone, Copy)]
struct Full(u128);

impl Record for Full {
    const LEN: usize = 16;
    const TAG: &'static [u8; 4] = b"PDH1";
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.0.to_le_bytes());
    }
    fn read(b: &[u8]) -> Self {
        Full(Reader::new(b).u128())
    }
}

fn store() -> &'static Store<Full> {
    static S: OnceLock<Store<Full>> = OnceLock::new();
    S.get_or_init(|| Store::open("hashes.bin", 600_000))
}

fn partial(path: &str, size: u64, job: &Job) -> Option<u128> {
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    h.update(&size.to_le_bytes());
    if size <= EDGE * 2 {
        let mut buf = Vec::with_capacity(size as usize);
        f.read_to_end(&mut buf).ok()?;
        h.update(&buf);
        job.done.fetch_add(buf.len() as u64, Ordering::Relaxed);
        return Some(h.digest128());
    }
    let mut buf = vec![0u8; EDGE as usize];
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    f.seek(SeekFrom::Start(size - EDGE)).ok()?;
    f.read_exact(&mut buf).ok()?;
    h.update(&buf);
    job.done.fetch_add(EDGE * 2, Ordering::Relaxed);
    Some(h.digest128())
}

pub fn full_hash(path: &str, size: u64, modified: u64, job: &Job) -> Option<u128> {
    if let Some(Full(v)) = store().get(path, size, modified) {
        job.done.fetch_add(size, Ordering::Relaxed);
        return Some(v);
    }
    let mut f = File::open(path).ok()?;
    let mut h = Xxh3::new();
    h.update(&size.to_le_bytes());
    let mut buf = vec![0u8; 1 << 20];
    let mut read = 0u64;
    loop {
        if job.cancelled() {
            return None;
        }
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        read += n as u64;
        job.done.fetch_add(n as u64, Ordering::Relaxed);
    }
    if read != size {
        return None;
    }
    let v = h.digest128();
    store().put(path, size, modified, Full(v));
    Some(v)
}

fn split<K: std::hash::Hash + Eq + Send>(items: Vec<(K, FileRef)>) -> Vec<Vec<FileRef>> {
    let mut m: HashMap<K, Vec<FileRef>> = HashMap::new();
    for (k, f) in items {
        m.entry(k).or_default().push(f);
    }
    m.into_values().filter(|v| v.len() > 1).collect()
}

pub fn exact_groups(files: Vec<FileRef>, job: &Job) -> Vec<Vec<FileRef>> {
    let by_size = split(files.into_iter().filter(|f| f.size > 0).map(|f| (f.size, f)).collect());
    let partial_bytes: u64 = by_size.iter().flatten().map(|f| f.size.min(EDGE * 2)).sum();
    job.total.store(partial_bytes, Ordering::Relaxed);
    let stage1: Vec<(u128, FileRef)> = by_size
        .into_par_iter()
        .flatten()
        .filter_map(|f| {
            if job.cancelled() {
                return None;
            }
            partial(&f.path, f.size, job).map(|h| (h, f))
        })
        .collect();
    if job.cancelled() {
        return Vec::new();
    }
    let candidates = split(stage1);
    let (small, large): (Vec<Vec<FileRef>>, Vec<Vec<FileRef>>) = candidates.into_iter().partition(|g| g[0].size <= EDGE * 2);
    let full_bytes: u64 = large.iter().flatten().map(|f| f.size).sum();
    job.total.fetch_add(full_bytes, Ordering::Relaxed);
    let stage2: Vec<((u64, u128), FileRef)> = large
        .into_par_iter()
        .flatten()
        .filter_map(|f| {
            if job.cancelled() {
                return None;
            }
            full_hash(&f.path, f.size, f.modified, job).map(|h| ((f.size, h), f))
        })
        .collect();
    store().save();
    if job.cancelled() {
        return Vec::new();
    }
    let mut out = small;
    out.extend(split(stage2));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_renamed_copies() {
        let dir = std::env::temp_dir().join(format!("pd-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big: Vec<u8> = (0..400_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let mut other = big.clone();
        other[200_000] ^= 1;
        let files = [("a.bin", &big), ("renamed copy.dat", &big), ("same size other.bin", &other), ("tiny1", &vec![1u8; 10]), ("tiny2", &vec![1u8; 10])];
        let refs: Vec<FileRef> = files
            .iter()
            .enumerate()
            .map(|(i, (n, d))| {
                let p = dir.join(n);
                std::fs::write(&p, d).unwrap();
                FileRef { id: i as u32, path: p.display().to_string(), size: d.len() as u64, modified: i as u64 + 1 }
            })
            .collect();
        let job = Job::default();
        let mut groups: Vec<Vec<u32>> = exact_groups(refs, &job).into_iter().map(|g| {
            let mut v: Vec<u32> = g.iter().map(|f| f.id).collect();
            v.sort();
            v
        }).collect();
        groups.sort();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(groups, vec![vec![0, 1], vec![3, 4]]);
    }
}

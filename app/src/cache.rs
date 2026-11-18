use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;

pub trait Record: Sized + Clone {
    const LEN: usize;
    const TAG: &'static [u8; 4];
    fn write(&self, out: &mut Vec<u8>);
    fn read(b: &[u8]) -> Self;
}

#[derive(Clone)]
struct Entry<V> {
    size: u64,
    modified: u64,
    seen: bool,
    value: V,
}

pub struct Store<V: Record> {
    path: PathBuf,
    map: Mutex<HashMap<u64, Entry<V>>>,
    dirty: Mutex<bool>,
    cap: usize,
}

pub fn path_key(path: &str) -> u64 {
    xxhash_rust::xxh3::xxh3_64(path.to_lowercase().as_bytes())
}

pub fn dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!("PrismDisk-test-{}", std::process::id()));
    }
    let base = std::env::var("LOCALAPPDATA").or_else(|_| std::env::var("APPDATA")).unwrap_or_default();
    PathBuf::from(base).join("PrismDisk")
}

impl<V: Record> Store<V> {
    pub fn open(name: &str, cap: usize) -> Self {
        let path = dir().join(name);
        let map = Self::load(&path).unwrap_or_default();
        Store { path, map: Mutex::new(map), dirty: Mutex::new(false), cap }
    }

    fn load(path: &PathBuf) -> Option<HashMap<u64, Entry<V>>> {
        let mut r = BufReader::new(std::fs::File::open(path).ok()?);
        let mut head = [0u8; 16];
        r.read_exact(&mut head).ok()?;
        if &head[..4] != V::TAG || u32::from_le_bytes(head[4..8].try_into().ok()?) as usize != V::LEN {
            return None;
        }
        let count = u64::from_le_bytes(head[8..16].try_into().ok()?) as usize;
        let mut map = HashMap::with_capacity(count);
        let mut buf = vec![0u8; 24 + V::LEN];
        for _ in 0..count {
            if r.read_exact(&mut buf).is_err() {
                break;
            }
            let key = u64::from_le_bytes(buf[0..8].try_into().unwrap());
            let size = u64::from_le_bytes(buf[8..16].try_into().unwrap());
            let modified = u64::from_le_bytes(buf[16..24].try_into().unwrap());
            map.insert(key, Entry { size, modified, seen: false, value: V::read(&buf[24..]) });
        }
        Some(map)
    }

    pub fn reload(&self) {
        if let Some(m) = Self::load(&self.path) {
            *self.map.lock().unwrap() = m;
            *self.dirty.lock().unwrap() = false;
        }
    }

    pub fn get(&self, path: &str, size: u64, modified: u64) -> Option<V> {
        let mut m = self.map.lock().unwrap();
        let e = m.get_mut(&path_key(path))?;
        if e.size != size || e.modified != modified {
            return None;
        }
        e.seen = true;
        Some(e.value.clone())
    }

    pub fn put(&self, path: &str, size: u64, modified: u64, value: V) {
        self.map.lock().unwrap().insert(path_key(path), Entry { size, modified, seen: true, value });
        *self.dirty.lock().unwrap() = true;
    }

    pub fn save(&self) {
        let mut dirty = self.dirty.lock().unwrap();
        if !*dirty {
            return;
        }
        let mut m = self.map.lock().unwrap();
        if m.len() > self.cap {
            m.retain(|_, e| e.seen);
        }
        let _ = std::fs::create_dir_all(dir());
        let tmp = self.path.with_extension("tmp");
        let ok = (|| -> std::io::Result<()> {
            let mut w = BufWriter::new(std::fs::File::create(&tmp)?);
            w.write_all(V::TAG)?;
            w.write_all(&(V::LEN as u32).to_le_bytes())?;
            w.write_all(&(m.len() as u64).to_le_bytes())?;
            let mut rec = Vec::with_capacity(V::LEN);
            for (k, e) in m.iter() {
                w.write_all(&k.to_le_bytes())?;
                w.write_all(&e.size.to_le_bytes())?;
                w.write_all(&e.modified.to_le_bytes())?;
                rec.clear();
                e.value.write(&mut rec);
                rec.resize(V::LEN, 0);
                w.write_all(&rec)?;
            }
            w.flush()
        })()
        .is_ok();
        if ok && std::fs::rename(&tmp, &self.path).is_ok() {
            *dirty = false;
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

pub struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Reader { b, at: 0 }
    }
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0u8; N];
        if self.at + N <= self.b.len() {
            out.copy_from_slice(&self.b[self.at..self.at + N]);
        }
        self.at += N;
        out
    }
    pub fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.take())
    }
    pub fn i64(&mut self) -> i64 {
        i64::from_le_bytes(self.take())
    }
    pub fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take())
    }
    pub fn u128(&mut self) -> u128 {
        u128::from_le_bytes(self.take())
    }
    pub fn str(&mut self, n: usize) -> String {
        let end = (self.at + n).min(self.b.len());
        let s = &self.b[self.at.min(end)..end];
        self.at += n;
        let len = s.iter().position(|&c| c == 0).unwrap_or(s.len());
        String::from_utf8_lossy(&s[..len]).into_owned()
    }
}

pub fn put_str(out: &mut Vec<u8>, s: &str, n: usize) {
    let mut b = s.as_bytes().to_vec();
    while b.len() > n {
        b.pop();
    }
    while std::str::from_utf8(&b).is_err() {
        b.pop();
    }
    let len = b.len();
    out.extend_from_slice(&b);
    out.extend(std::iter::repeat(0).take(n - len));
}

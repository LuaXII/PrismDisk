use std::fs::File;
use std::io::Read;

const EXTS: [&str; 3] = ["tc", "hc", "vc"];
const MIN_SIZE: u64 = 256 * 1024;
const HALF: usize = 64 * 1024;
const MIN_ENTROPY: f64 = 7.99;
const MAX_PROBES: usize = 48;
const MAX_CANDIDATES: usize = 4000;

pub fn has_container_ext(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && EXTS.iter().any(|x| ext.eq_ignore_ascii_case(x)),
        None => false,
    }
}

fn has_ext(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && !ext.is_empty() && ext.len() <= 16 && !ext.contains(' '),
        None => false,
    }
}

fn plausible_size(size: u64) -> bool {
    size >= MIN_SIZE && size % 512 == 0
}

fn known_header(b: &[u8]) -> bool {
    const MAGIC: [&[u8]; 26] = [
        b"PK\x03\x04", b"PK\x05\x06", b"7z\xBC\xAF\x27\x1C", b"Rar!", b"\x1F\x8B", b"BZh", b"\xFD7zXZ", b"\x28\xB5\x2F\xFD",
        b"\x04\x22\x4D\x18", b"MZ", b"%PDF", b"\x89PNG", b"\xFF\xD8\xFF", b"GIF8", b"RIFF", b"OggS", b"fLaC", b"ID3",
        b"\x1A\x45\xDF\xA3", b"MSCF", b"MSWIM", b"SQLite format", b"vhdxfile", b"conectix", b"ITSF", b"\x00\x00\x01\xBA",
    ];
    if MAGIC.iter().any(|m| b.starts_with(m)) {
        return true;
    }
    if b.len() >= 12 && &b[4..8] == b"ftyp" {
        return true;
    }
    b.len() >= 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0
}

fn entropy(b: &[u8]) -> f64 {
    let mut counts = [0u32; 256];
    for &x in b {
        counts[x as usize] += 1;
    }
    let n = b.len() as f64;
    counts.iter().filter(|&&c| c > 0).map(|&c| {
        let p = c as f64 / n;
        -p * p.log2()
    }).sum()
}

pub fn looks_encrypted(bytes: &[u8]) -> bool {
    bytes.len() >= HALF * 2
        && !known_header(bytes)
        && entropy(&bytes[..HALF]) >= MIN_ENTROPY
        && entropy(&bytes[HALF..HALF * 2]) >= MIN_ENTROPY
}

fn probe(path: &str, size: u64) -> bool {
    if !plausible_size(size) {
        return false;
    }
    let Ok(mut f) = File::open(path) else { return false };
    let mut buf = vec![0u8; HALF * 2];
    f.read_exact(&mut buf).is_ok() && looks_encrypted(&buf)
}

pub struct Target {
    pub path: String,
    pub name: String,
    pub dir: bool,
    pub size: u64,
}

pub fn find(targets: &[Target], under: impl Fn(&str, usize) -> Vec<(String, u64, u64)>) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut probes = 0;
    for t in targets {
        if !t.dir {
            if has_container_ext(&t.name) || probe(&t.path, t.size) {
                out.push((t.path.clone(), t.size));
            }
            continue;
        }
        for (path, size, _) in under(&t.path, MAX_CANDIDATES) {
            let name = path.rsplit('\\').next().unwrap_or_default();
            if has_container_ext(name) {
                out.push((path, size));
            } else if !has_ext(name) && plausible_size(size) && probes < MAX_PROBES {
                probes += 1;
                if probe(&path, size) {
                    out.push((path, size));
                }
            }
        }
    }
    out
}

pub fn is_candidate(name: &str, size: u64) -> bool {
    has_container_ext(name) || (!has_ext(name) && plausible_size(size))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(n: usize) -> Vec<u8> {
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..n).map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 24) as u8
        }).collect()
    }

    #[test]
    fn extensions() {
        assert!(has_container_ext("vault.hc"));
        assert!(has_container_ext("Old.TC"));
        assert!(!has_container_ext(".hc"));
        assert!(!has_container_ext("movie.mp4"));
        assert!(is_candidate("vault", 1 << 20));
        assert!(!is_candidate("vault", (1 << 20) + 1));
        assert!(!is_candidate("movie.mp4", 1 << 30));
    }

    #[test]
    fn detection() {
        let r = noise(HALF * 2);
        assert!(looks_encrypted(&r));
        let mut zip = r.clone();
        zip[..4].copy_from_slice(b"PK\x03\x04");
        assert!(!looks_encrypted(&zip));
        let mut mp4 = r.clone();
        mp4[4..8].copy_from_slice(b"ftyp");
        assert!(!looks_encrypted(&mp4));
        let mut text = r.clone();
        for (i, b) in text[HALF..].iter_mut().enumerate() {
            *b = b"lorem ipsum "[i % 12];
        }
        assert!(!looks_encrypted(&text));
        assert!(!looks_encrypted(&vec![0u8; HALF * 2]));
    }
}

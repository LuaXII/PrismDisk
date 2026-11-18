use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::Ordering;

use crate::scan::ScanCtl;
use crate::tree::RawEntry;

const REF_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
const ROOT_RECORD: usize = 5;

const ATTR_STANDARD_INFORMATION: u32 = 0x10;
const ATTR_FILE_NAME: u32 = 0x30;
const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

#[inline]
fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
#[inline]
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
#[inline]
fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

pub struct Geometry {
    pub cluster: u64,
    pub total_bytes: u64,
    pub mft_lcn: u64,
    pub record_size: usize,
}

pub fn parse_boot(b: &[u8]) -> Option<Geometry> {
    if b.len() < 0x50 || &b[3..11] != b"NTFS    " {
        return None;
    }
    let bps = u16_at(b, 0x0B) as u64;
    let spc_raw = b[0x0D];
    if bps == 0 || spc_raw == 0 {
        return None;
    }
    let spc = if spc_raw > 0x80 { 1u64 << (256 - spc_raw as u32) } else { spc_raw as u64 };
    let cluster = bps * spc;
    let total_bytes = u64_at(b, 0x28) * bps;
    let mft_lcn = u64_at(b, 0x30);
    let cpr = b[0x40] as i8;
    let record_size = if cpr < 0 { 1usize << (-(cpr as i32)) } else { cpr as usize * cluster as usize };
    if !(256..=65536).contains(&record_size) {
        return None;
    }
    Some(Geometry { cluster, total_bytes, mft_lcn, record_size })
}

fn apply_fixups(rec: &mut [u8]) -> bool {
    let usa_off = u16_at(rec, 4) as usize;
    let usa_cnt = u16_at(rec, 6) as usize;
    if usa_cnt < 2 || usa_off + usa_cnt * 2 > rec.len() {
        return false;
    }
    let usn = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_cnt {
        let end = i * 512;
        if end > rec.len() {
            break;
        }
        if rec[end - 2] != usn[0] || rec[end - 1] != usn[1] {
            return false;
        }
        rec[end - 2] = rec[usa_off + i * 2];
        rec[end - 1] = rec[usa_off + i * 2 + 1];
    }
    true
}

fn decode_runs(b: &[u8], mut p: usize) -> Vec<(Option<u64>, u64)> {
    let mut out = Vec::new();
    let mut lcn: i64 = 0;
    while p < b.len() && b[p] != 0 {
        let h = b[p];
        let len_sz = (h & 0x0F) as usize;
        let off_sz = (h >> 4) as usize;
        p += 1;
        if len_sz == 0 || len_sz > 8 || off_sz > 8 || p + len_sz + off_sz > b.len() {
            break;
        }
        let mut len: u64 = 0;
        for i in 0..len_sz {
            len |= (b[p + i] as u64) << (8 * i);
        }
        p += len_sz;
        if off_sz == 0 {
            out.push((None, len));
        } else {
            let mut d: i64 = 0;
            for i in 0..off_sz {
                d |= (b[p + i] as i64) << (8 * i);
            }
            if b[p + off_sz - 1] & 0x80 != 0 && off_sz < 8 {
                d |= -1i64 << (8 * off_sz);
            }
            lcn += d;
            out.push((Some(lcn as u64), len));
        }
        p += off_sz;
    }
    out
}

#[derive(Default)]
struct Parsed {
    name: Option<(u8, u64, String)>,
    data: Option<(u64, u64)>,
    std: Option<(u64, u32)>,
    runs_off: Option<usize>,
}

fn parse_attrs(rec: &[u8], out: &mut Parsed) {
    let mut p = u16_at(rec, 0x14) as usize;
    let used = (u32_at(rec, 0x18) as usize).min(rec.len());
    while p + 16 <= used {
        let ty = u32_at(rec, p);
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, p + 4) as usize;
        if len < 16 || p + len > used {
            break;
        }
        let a = &rec[p..p + len];
        let nonres = a[8] != 0;
        let name_len = a[9];
        match ty {
            ATTR_STANDARD_INFORMATION if !nonres && len >= 0x18 => {
                let vo = u16_at(a, 0x14) as usize;
                if vo + 0x24 <= len {
                    out.std = Some((u64_at(a, vo + 0x08), u32_at(a, vo + 0x20)));
                }
            }
            ATTR_FILE_NAME if !nonres && len >= 0x18 => {
                let vo = u16_at(a, 0x14) as usize;
                if vo + 0x42 <= len {
                    let parent = u64_at(a, vo) & REF_MASK;
                    let nlen = a[vo + 0x40] as usize;
                    let ns = a[vo + 0x41];
                    let prio = match ns {
                        1 | 3 => 3,
                        0 => 2,
                        _ => 1,
                    };
                    let s = vo + 0x42;
                    if s + nlen * 2 <= len && out.name.as_ref().map_or(true, |n| prio > n.0) {
                        let w: Vec<u16> = (0..nlen).map(|i| u16_at(a, s + i * 2)).collect();
                        out.name = Some((prio, parent, String::from_utf16_lossy(&w)));
                    }
                }
            }
            ATTR_DATA if name_len == 0 => {
                if nonres {
                    if len >= 0x40 && u64_at(a, 0x10) == 0 {
                        let flags = u16_at(a, 0x0C);
                        let mut alloc = u64_at(a, 0x28);
                        let real = u64_at(a, 0x30);
                        if flags & 0x8001 != 0 && len >= 0x48 {
                            alloc = u64_at(a, 0x40);
                        }
                        out.data = Some((real, alloc));
                        out.runs_off = Some(p + u16_at(a, 0x20) as usize);
                    }
                } else {
                    out.data = Some((u32_at(a, 0x10) as u64, 0));
                }
            }
            _ => {}
        }
        p += len;
    }
}

fn read_at(f: &mut File, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
    f.seek(SeekFrom::Start(off))?;
    f.read_exact(buf)
}

#[cfg(windows)]
fn open_volume(letter: char) -> std::io::Result<File> {
    use std::os::windows::io::FromRawHandle;
    use windows::core::HSTRING;
    use windows::Win32::Foundation::GENERIC_READ;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    let path = format!("\\\\.\\{}:", letter);
    let h = unsafe {
        CreateFileW(
            &HSTRING::from(path.as_str()),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_SEQUENTIAL_SCAN,
            None,
        )
    }
    .map_err(|e| std::io::Error::from_raw_os_error(e.code().0 & 0xFFFF))?;
    Ok(unsafe { File::from_raw_handle(h.0 as _) })
}

#[cfg(not(windows))]
fn open_volume(_letter: char) -> std::io::Result<File> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "MFT scanning needs Windows"))
}

pub enum MftError {
    Access(std::io::Error),
    NotNtfs,
    Cancelled,
    Corrupt(String),
}

pub fn scan(letter: char, ctl: &ScanCtl) -> Result<(Vec<RawEntry>, usize, u64), MftError> {
    let mut f = open_volume(letter).map_err(MftError::Access)?;

    let mut boot = [0u8; 512];
    read_at(&mut f, 0, &mut boot).map_err(MftError::Access)?;
    let geo = parse_boot(&boot).ok_or(MftError::NotNtfs)?;
    let rs = geo.record_size;

    let mut rec0 = vec![0u8; rs.max(512)];
    read_at(&mut f, geo.mft_lcn * geo.cluster, &mut rec0).map_err(|e| MftError::Corrupt(e.to_string()))?;
    if &rec0[0..4] != b"FILE" || !apply_fixups(&mut rec0[..rs]) {
        return Err(MftError::Corrupt("$MFT record 0 is unreadable".into()));
    }
    let mut p0 = Parsed::default();
    parse_attrs(&rec0[..rs], &mut p0);
    let (mft_bytes, _) = p0.data.ok_or_else(|| MftError::Corrupt("$MFT has no $DATA".into()))?;
    let runs = decode_runs(&rec0[..rs], p0.runs_off.unwrap_or(rs));
    if runs.is_empty() {
        return Err(MftError::Corrupt("$MFT run list is empty".into()));
    }
    let total = (mft_bytes / rs as u64) as usize;
    ctl.total.store(total as u64, Ordering::Relaxed);
    ctl.set_phase("Reading Master File Table");

    let mut entries: Vec<RawEntry> = Vec::with_capacity(total);
    entries.resize_with(total, RawEntry::default);
    let mut prio = vec![0u8; total];

    const CHUNK: usize = 8 << 20;
    let chunk = (CHUNK / rs).max(1) * rs;
    let mut buf = vec![0u8; chunk];
    let mut rec_no: usize = 0;
    let mut parsed = Parsed::default();

    'runs: for (lcn, clusters) in runs {
        let run_bytes = clusters * geo.cluster;
        let Some(lcn) = lcn else {
            rec_no += (run_bytes / rs as u64) as usize;
            continue;
        };
        let mut done: u64 = 0;
        while done < run_bytes && rec_no < total {
            if ctl.cancel.load(Ordering::Relaxed) {
                return Err(MftError::Cancelled);
            }
            let want = ((run_bytes - done) as usize).min(chunk).min((total - rec_no) * rs);
            let want = want.div_ceil(512) * 512;
            let b = &mut buf[..want];
            read_at(&mut f, lcn * geo.cluster + done, b).map_err(|e| MftError::Corrupt(e.to_string()))?;
            done += want as u64;

            for r in b.chunks_exact_mut(rs) {
                let idx = rec_no;
                rec_no += 1;
                if idx >= total {
                    break 'runs;
                }
                if &r[0..4] != b"FILE" || u16_at(r, 0x16) & 1 == 0 || !apply_fixups(r) {
                    continue;
                }
                let flags = u16_at(r, 0x16);
                let base = (u64_at(r, 0x20) & REF_MASK) as usize;
                let target = if base != 0 { base } else { idx };
                if target >= total {
                    continue;
                }
                parsed.name = None;
                parsed.data = None;
                parsed.std = None;
                parse_attrs(r, &mut parsed);

                let e = &mut entries[target];
                if base == 0 {
                    e.valid = true;
                    e.is_dir = flags & 2 != 0;
                }
                if let Some((pr, parent, name)) = parsed.name.take() {
                    if pr > prio[target] {
                        prio[target] = pr;
                        e.parent = parent.min(u32::MAX as u64) as u32;
                        e.name = name;
                    }
                }
                if let Some((real, alloc)) = parsed.data {
                    e.size = real;
                    e.alloc = alloc;
                }
                if let Some((mt, at)) = parsed.std {
                    e.modified = mt;
                    e.attrs = at;
                }
            }
            ctl.done.store(rec_no.min(total) as u64, Ordering::Relaxed);
        }
    }

    for e in entries.iter_mut() {
        if e.is_dir {
            e.size = 0;
            e.alloc = 0;
        }
        if e.valid && e.name.is_empty() {
            e.valid = false;
        }
    }
    if total <= ROOT_RECORD || !entries[ROOT_RECORD].is_dir {
        return Err(MftError::Corrupt("root directory record missing".into()));
    }
    entries[ROOT_RECORD].valid = true;
    Ok((entries, ROOT_RECORD, geo.total_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_decode_signed_deltas() {
        let b = [0x21, 0x10, 0x00, 0x10, 0x11, 0x08, 0xF0, 0x00];
        let r = decode_runs(&b, 0);
        assert_eq!(r, vec![(Some(0x1000), 0x10), (Some(0xFF0), 0x08)]);
    }

    #[test]
    #[ignore]
    fn scan_c_drive() {
        let ctl = ScanCtl::default();
        let t0 = std::time::Instant::now();
        let (raw, root, _) = match scan('C', &ctl) {
            Ok(v) => v,
            Err(MftError::Access(e)) => panic!("open failed (run elevated): {e}"),
            Err(MftError::NotNtfs) => panic!("not NTFS"),
            Err(MftError::Cancelled) => panic!("cancelled"),
            Err(MftError::Corrupt(s)) => panic!("corrupt: {s}"),
        };
        let read = t0.elapsed();
        let tree = crate::tree::build(raw, root, "C:".into(), "C:\\".into()).unwrap();
        let r = &tree.nodes[0];
        println!(
            "records read in {:?}, tree in {:?}: {} nodes, {} files, {} folders, alloc {:.2} GiB, size {:.2} GiB",
            read,
            t0.elapsed(),
            tree.nodes.len(),
            r.files,
            r.folders,
            r.alloc as f64 / (1u64 << 30) as f64,
            r.size as f64 / (1u64 << 30) as f64
        );
        for row in tree.child_rows(0, "alloc", true).iter().take(15) {
            println!("  {:<40} {:>10.2} GiB  {:>9} files", row.name, row.alloc as f64 / (1u64 << 30) as f64, row.files);
        }
        assert!(r.files > 10_000);
    }
}

#[derive(Clone, Debug)]
pub struct DriveInfo {
    pub id: String,
    pub root: String,
    pub label: String,
    pub fs: String,
    pub capacity: u64,
    pub free: u64,
    pub removable: bool,
}

#[cfg(windows)]
mod win {
    use super::DriveInfo;
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOVABLE};

    pub fn enumerate() -> Vec<DriveInfo> {
        let mut out = Vec::new();
        let mask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if mask & (1 << i) == 0 {
                continue;
            }
            let letter = (b'A' + i as u8) as char;
            let root = format!("{letter}:\\");
            let h = HSTRING::from(root.as_str());
            let ty = unsafe { GetDriveTypeW(&h) };
            if ty != DRIVE_FIXED && ty != DRIVE_REMOVABLE {
                continue;
            }
            let mut label = [0u16; 261];
            let mut fs = [0u16; 64];
            let ok = unsafe { GetVolumeInformationW(&h, Some(&mut label), None, None, None, Some(&mut fs)) }.is_ok();
            if !ok {
                continue;
            }
            let (mut free, mut total) = (0u64, 0u64);
            let _ = unsafe { GetDiskFreeSpaceExW(&h, Some(&mut free), Some(&mut total), None) };
            out.push(DriveInfo {
                id: letter.to_string(),
                root,
                label: w2s(&label),
                fs: w2s(&fs),
                capacity: total,
                free,
                removable: ty == DRIVE_REMOVABLE,
            });
        }
        out
    }

    pub fn cluster_size(path: &str) -> u64 {
        let root: String = path.chars().take(3).collect();
        let (mut spc, mut bps, mut fc, mut tc) = (0u32, 0u32, 0u32, 0u32);
        let ok = unsafe {
            GetDiskFreeSpaceW(&HSTRING::from(root.as_str()), Some(&mut spc), Some(&mut bps), Some(&mut fc), Some(&mut tc))
        }
        .is_ok();
        if ok && spc > 0 && bps > 0 { spc as u64 * bps as u64 } else { 4096 }
    }

    fn w2s(b: &[u16]) -> String {
        let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        String::from_utf16_lossy(&b[..n])
    }
}

#[cfg(not(windows))]
mod win {
    pub fn enumerate() -> Vec<super::DriveInfo> {
        Vec::new()
    }
    pub fn cluster_size(_p: &str) -> u64 {
        4096
    }
}

pub use win::*;

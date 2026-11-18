#![allow(non_snake_case)]

use windows::core::{HSTRING, PCWSTR, PROPVARIANT};
use windows::Win32::Foundation::{FILETIME, GENERIC_READ, SIZE, SYSTEMTIME};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};
use windows::Win32::System::Time::{SystemTimeToFileTime, TzSpecificLocalTimeToSystemTime};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_THUMBNAILONLY};

pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

pub struct ImageInfo {
    pub w: u32,
    pub h: u32,
    pub taken: i64,
    pub camera: String,
    pub exif: bool,
    pub gray: Gray,
}

pub struct VideoInfo {
    pub w: u32,
    pub h: u32,
    pub duration_ms: u32,
    pub frames: Vec<[f32; 1024]>,
}

pub fn com_mta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

pub fn com_sta() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

thread_local! {
    static WIC: Option<IWICImagingFactory> = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok() };
}

fn factory() -> Option<IWICImagingFactory> {
    WIC.with(|f| f.clone())
}

fn query(q: &IWICMetadataQueryReader, paths: &[&str]) -> Option<PROPVARIANT> {
    for p in paths {
        let mut v = PROPVARIANT::default();
        if unsafe { q.GetMetadataByName(&HSTRING::from(*p), &mut v) }.is_ok() && !v.is_empty() {
            return Some(v);
        }
    }
    None
}

pub fn exif_local_to_unix_ms(s: &str) -> Option<i64> {
    let d: Vec<u16> = s.trim().split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect();
    if d.len() < 5 || d[0] < 1900 || d[1] == 0 || d[1] > 12 || d[2] == 0 || d[2] > 31 {
        return None;
    }
    let local = SYSTEMTIME { wYear: d[0], wMonth: d[1], wDay: d[2], wHour: d[3], wMinute: d[4], wSecond: *d.get(5).unwrap_or(&0), ..Default::default() };
    let mut utc = SYSTEMTIME::default();
    let mut ft = FILETIME::default();
    unsafe {
        TzSpecificLocalTimeToSystemTime(None, &local, &mut utc).ok()?;
        SystemTimeToFileTime(&utc, &mut ft).ok()?;
    }
    let t = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    Some(crate::tree::filetime_to_unix_ms(t))
}

pub fn orient<T: Copy>(src: &[T], w: usize, h: usize, o: u8) -> (Vec<T>, usize, usize) {
    let swap = matches!(o, 5..=8);
    let (ow, oh) = if swap { (h, w) } else { (w, h) };
    let mut out = Vec::with_capacity(ow * oh);
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = match o {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                8 => (w - 1 - y, x),
                _ => (x, y),
            };
            out.push(src[sy * w + sx]);
        }
    }
    (out, ow, oh)
}

fn fit(w: u32, h: u32, max: u32) -> (u32, u32) {
    if w <= max && h <= max {
        return (w.max(1), h.max(1));
    }
    let s = max as f64 / w.max(h) as f64;
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

pub fn analyze_image(path: &str, max_side: u32) -> Option<ImageInfo> {
    let f = factory()?;
    unsafe {
        let dec = f.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let mut taken = 0;
        let mut camera = String::new();
        let mut exif = false;
        let mut orientation = 1u8;
        if let Ok(q) = frame.GetMetadataQueryReader() {
            if let Some(v) = query(&q, &["/app1/ifd/exif/{ushort=36867}", "/ifd/exif/{ushort=36867}", "/app1/ifd/exif/{ushort=36868}", "/ifd/exif/{ushort=36868}", "/xmp/exif:DateTimeOriginal", "/app1/ifd/{ushort=306}", "/ifd/{ushort=306}"]) {
                if let Some(t) = exif_local_to_unix_ms(&v.to_string()) {
                    taken = t;
                    exif = true;
                }
            }
            if let Some(v) = query(&q, &["/app1/ifd/{ushort=274}", "/ifd/{ushort=274}"]) {
                orientation = u16::try_from(&v).unwrap_or(1).clamp(1, 8) as u8;
                exif = true;
            }
            let make = query(&q, &["/app1/ifd/{ushort=271}", "/ifd/{ushort=271}"]).map(|v| v.to_string()).unwrap_or_default();
            let model = query(&q, &["/app1/ifd/{ushort=272}", "/ifd/{ushort=272}"]).map(|v| v.to_string()).unwrap_or_default();
            let (make, model) = (make.trim().trim_end_matches('\0').to_string(), model.trim().trim_end_matches('\0').to_string());
            camera = if model.to_lowercase().starts_with(&make.to_lowercase().split(' ').next().unwrap_or_default().to_string()) || make.is_empty() {
                model
            } else {
                format!("{make} {model}")
            };
            if !camera.is_empty() {
                exif = true;
            }
        }
        let (sw, sh) = fit(w, h, max_side);
        let scaler = f.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, sw, sh, WICBitmapInterpolationModeFant).ok()?;
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat8bppGray, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let mut px = vec![0u8; (sw * sh) as usize];
        conv.CopyPixels(std::ptr::null(), sw, &mut px).ok()?;
        let (px, gw, gh) = orient(&px, sw as usize, sh as usize, orientation);
        let (w, h) = if matches!(orientation, 5..=8) { (h, w) } else { (w, h) };
        Some(ImageInfo { w, h, taken, camera, exif, gray: Gray { w: gw, h: gh, px } })
    }
}

fn wic_transform(o: u8) -> WICBitmapTransformOptions {
    match o {
        2 => WICBitmapTransformFlipHorizontal,
        3 => WICBitmapTransformRotate180,
        4 => WICBitmapTransformFlipVertical,
        5 => WICBitmapTransformOptions(WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0),
        6 => WICBitmapTransformRotate90,
        7 => WICBitmapTransformOptions(WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0),
        8 => WICBitmapTransformRotate270,
        _ => WICBitmapTransformRotate0,
    }
}

pub fn image_rgba(path: &str, max_side: u32) -> Option<(usize, usize, Vec<u8>)> {
    let f = factory()?;
    unsafe {
        let dec = f.CreateDecoderFromFilename(&HSTRING::from(path), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        let mut orientation = 1u8;
        if let Ok(q) = frame.GetMetadataQueryReader() {
            if let Some(v) = query(&q, &["/app1/ifd/{ushort=274}", "/ifd/{ushort=274}"]) {
                orientation = u16::try_from(&v).unwrap_or(1).clamp(1, 8) as u8;
            }
        }
        let (sw, sh) = fit(w, h, max_side);
        let scaler = f.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, sw, sh, WICBitmapInterpolationModeFant).ok()?;
        let rot = f.CreateBitmapFlipRotator().ok()?;
        rot.Initialize(&scaler, wic_transform(orientation)).ok()?;
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&rot, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let (mut ow, mut oh) = (0u32, 0u32);
        conv.GetSize(&mut ow, &mut oh).ok()?;
        let mut px = vec![0u8; (ow * oh * 4) as usize];
        conv.CopyPixels(std::ptr::null(), ow * 4, &mut px).ok()?;
        Some((ow as usize, oh as usize, px))
    }
}

pub fn shell_thumb(path: &str, px: i32) -> Option<(usize, usize, Vec<u8>)> {
    unsafe {
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path), None).ok()?;
        let hbm = item.GetImage(SIZE { cx: px, cy: px }, SIIGBF_THUMBNAILONLY).ok()?;
        let mut bm = BITMAP::default();
        let got = GetObjectW(hbm, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        let out = if got > 0 && bm.bmWidth > 0 && bm.bmHeight > 0 {
            let (w, h) = (bm.bmWidth as usize, bm.bmHeight as usize);
            let mut info = BITMAPINFO::default();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut buf = vec![0u8; w * h * 4];
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(dc, hbm, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
            let _ = DeleteDC(dc);
            if lines > 0 {
                let opaque = buf.chunks_exact(4).all(|p| p[3] == 0);
                for p in buf.chunks_exact_mut(4) {
                    p.swap(0, 2);
                    if opaque {
                        p[3] = 255;
                    }
                }
                Some((w, h, buf))
            } else {
                None
            }
        } else {
            None
        };
        let _ = DeleteObject(hbm);
        out
    }
}

pub fn tiny(src: &[u8], w: usize, h: usize, stride: usize) -> [f32; 1024] {
    let mut out = [0f32; 1024];
    let step_x = (w / 96).max(1);
    let step_y = (h / 96).max(1);
    for cy in 0..32 {
        let y0 = cy * h / 32;
        let y1 = ((cy + 1) * h / 32).max(y0 + 1).min(h);
        for cx in 0..32 {
            let x0 = cx * w / 32;
            let x1 = ((cx + 1) * w / 32).max(x0 + 1).min(w);
            let (mut sum, mut n) = (0u32, 0u32);
            let mut y = y0;
            while y < y1 {
                let row = &src[y * stride..];
                let mut x = x0;
                while x < x1 {
                    sum += row[x] as u32;
                    n += 1;
                    x += step_x;
                }
                y += step_y;
            }
            out[cy * 32 + cx] = if n > 0 { sum as f32 / n as f32 } else { 0.0 };
        }
    }
    out
}

static MF_START: std::sync::Once = std::sync::Once::new();

pub fn analyze_video(path: &str, cancel: &std::sync::atomic::AtomicBool) -> Option<VideoInfo> {
    MF_START.call_once(|| unsafe {
        let _ = MFStartup(MF_VERSION, MFSTARTUP_LITE);
    });
    unsafe {
        let mut attrs: Option<IMFAttributes> = None;
        MFCreateAttributes(&mut attrs, 2).ok()?;
        let attrs = attrs?;
        attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1).ok()?;
        let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);
        let reader = MFCreateSourceReaderFromURL(PCWSTR(HSTRING::from(path).as_ptr()), &attrs).ok()?;
        let vs = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        let _ = reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false);
        reader.SetStreamSelection(vs, true).ok()?;
        let mt = MFCreateMediaType().ok()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
        mt.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).ok()?;
        reader.SetCurrentMediaType(vs, None, &mt).ok()?;
        let cur = reader.GetCurrentMediaType(vs).ok()?;
        let fs = cur.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
        let (fw, fh) = ((fs >> 32) as usize, (fs & 0xffff_ffff) as usize);
        if fw == 0 || fh == 0 {
            return None;
        }
        let (mut cx, mut cy, mut cw, mut ch) = (0usize, 0usize, fw, fh);
        let mut area = [0u8; std::mem::size_of::<MFVideoArea>()];
        if cur.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, &mut area, None).is_ok() {
            let a: MFVideoArea = std::ptr::read_unaligned(area.as_ptr() as *const MFVideoArea);
            let (ax, ay, aw, ah) = (a.OffsetX.value.max(0) as usize, a.OffsetY.value.max(0) as usize, a.Area.cx.max(0) as usize, a.Area.cy.max(0) as usize);
            if aw > 0 && ah > 0 && ax + aw <= fw && ay + ah <= fh {
                (cx, cy, cw, ch) = (ax, ay, aw, ah);
            }
        }
        let rotation = cur.GetUINT32(&MF_MT_VIDEO_ROTATION).unwrap_or(0);
        let dur = reader.GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION).ok().and_then(|v| u64::try_from(&v).ok()).unwrap_or(0);
        let (w, h) = if rotation == 90 || rotation == 270 { (ch as u32, cw as u32) } else { (cw as u32, ch as u32) };
        let duration_ms = (dur / 10_000).min(u32::MAX as u64) as u32;
        let mut frames = Vec::new();
        if dur == 0 {
            return Some(VideoInfo { w, h, duration_ms, frames });
        }
        let o = match rotation {
            90 => 6,
            180 => 3,
            270 => 8,
            _ => 1,
        };
        for frac in [0.1f64, 0.3, 0.5, 0.7, 0.9] {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return None;
            }
            let target = (dur as f64 * frac) as i64;
            if reader.SetCurrentPosition(&windows::core::GUID::zeroed(), &PROPVARIANT::from(target)).is_err() {
                break;
            }
            let mut got = None;
            for _ in 0..240 {
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample: Option<IMFSample> = None;
                if reader.ReadSample(vs, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)).is_err() {
                    break;
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if let Some(s) = sample {
                    got = Some(s);
                    if ts + 400_000 >= target {
                        break;
                    }
                }
            }
            let Some(s) = got else { continue };
            let Ok(buf) = s.ConvertToContiguousBuffer() else { continue };
            let mut ptr: *mut u8 = std::ptr::null_mut();
            let mut pitch: i32 = fw as i32;
            let two_d: Option<IMF2DBuffer> = windows::core::Interface::cast(&buf).ok();
            let locked2d = two_d.as_ref().map(|b| b.Lock2D(&mut ptr, &mut pitch).is_ok()).unwrap_or(false);
            let mut len = 0u32;
            if !locked2d {
                pitch = fw as i32;
                if buf.Lock(&mut ptr, None, Some(&mut len)).is_err() {
                    continue;
                }
            }
            if !ptr.is_null() && pitch > 0 {
                let stride = pitch as usize;
                let needed = stride * fh;
                if locked2d || len as usize >= needed {
                    let plane = std::slice::from_raw_parts(ptr, needed);
                    let crop = &plane[cy * stride + cx..];
                    let t = tiny(crop, cw, ch, stride);
                    let (t, _, _) = orient(&t, 32, 32, o);
                    let mut a = [0f32; 1024];
                    a.copy_from_slice(&t);
                    frames.push(a);
                }
            }
            if locked2d {
                let _ = two_d.as_ref().map(|b| b.Unlock2D());
            } else {
                let _ = buf.Unlock();
            }
        }
        Some(VideoInfo { w, h, duration_ms, frames })
    }
}

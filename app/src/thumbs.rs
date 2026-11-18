use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::decode;

#[derive(Clone)]
struct Req {
    key: u64,
    path: String,
    px: u32,
    image: bool,
    big: bool,
}

struct Pixels {
    w: usize,
    h: usize,
    rgba: Vec<u8>,
    premultiplied: bool,
}

#[derive(Default)]
struct Queue {
    want: Vec<Req>,
    inflight: HashSet<u64>,
    stop: bool,
}

struct Shared {
    queue: Mutex<Queue>,
    cv: Condvar,
    done: Mutex<Vec<(u64, Option<Pixels>)>>,
    ctx: egui::Context,
}

struct Slot {
    tex: Option<TextureHandle>,
    bytes: usize,
    used: u64,
}

pub struct Thumbs {
    shared: Arc<Shared>,
    slots: HashMap<u64, Slot>,
    wanted: Vec<Req>,
    frame: u64,
    bytes: usize,
    budget: usize,
}

pub fn key(id: u32, px: u32, big: bool) -> u64 {
    ((id as u64) << 24) | ((px as u64) << 1) | big as u64
}

fn load(r: &Req) -> Option<Pixels> {
    if r.big && r.image {
        if let Some((w, h, rgba)) = decode::image_rgba(&r.path, r.px) {
            return Some(Pixels { w, h, rgba, premultiplied: false });
        }
    }
    if let Some((w, h, rgba)) = decode::shell_thumb(&r.path, r.px as i32) {
        return Some(Pixels { w, h, rgba, premultiplied: true });
    }
    if r.image {
        if let Some((w, h, rgba)) = decode::image_rgba(&r.path, r.px) {
            return Some(Pixels { w, h, rgba, premultiplied: false });
        }
    }
    None
}

fn worker(s: Arc<Shared>) {
    decode::com_sta();
    loop {
        let req = {
            let mut q = s.queue.lock().unwrap();
            loop {
                if q.stop {
                    return;
                }
                let pos = q.want.iter().position(|r| !q.inflight.contains(&r.key));
                if let Some(i) = pos {
                    let r = q.want.remove(i);
                    q.inflight.insert(r.key);
                    break r;
                }
                q = s.cv.wait(q).unwrap();
            }
        };
        let px = load(&req);
        s.done.lock().unwrap().push((req.key, px));
        s.queue.lock().unwrap().inflight.remove(&req.key);
        s.ctx.request_repaint();
    }
}

impl Thumbs {
    pub fn new(ctx: &egui::Context) -> Self {
        let shared = Arc::new(Shared { queue: Mutex::new(Queue::default()), cv: Condvar::new(), done: Mutex::new(Vec::new()), ctx: ctx.clone() });
        for _ in 0..3 {
            let s = shared.clone();
            std::thread::spawn(move || worker(s));
        }
        Thumbs { shared, slots: HashMap::new(), wanted: Vec::new(), frame: 0, bytes: 0, budget: 64 << 20 }
    }

    pub fn begin(&mut self) {
        self.frame += 1;
        self.wanted.clear();
        let done: Vec<(u64, Option<Pixels>)> = std::mem::take(&mut *self.shared.done.lock().unwrap());
        for (k, px) in done {
            let (tex, bytes) = match px {
                Some(p) if p.w > 0 && p.h > 0 && p.rgba.len() >= p.w * p.h * 4 => {
                    let img = if p.premultiplied {
                        ColorImage::from_rgba_premultiplied([p.w, p.h], &p.rgba[..p.w * p.h * 4])
                    } else {
                        ColorImage::from_rgba_unmultiplied([p.w, p.h], &p.rgba[..p.w * p.h * 4])
                    };
                    (Some(self.shared.ctx.load_texture(format!("thumb-{k}"), img, TextureOptions::LINEAR)), p.w * p.h * 4)
                }
                _ => (None, 0),
            };
            if let Some(old) = self.slots.insert(k, Slot { tex, bytes, used: self.frame }) {
                self.bytes -= old.bytes;
            }
            self.bytes += bytes;
        }
    }

    pub fn get(&mut self, id: u32, path: &str, px: u32, image: bool, big: bool) -> Option<Option<TextureHandle>> {
        let k = key(id, px, big);
        if let Some(s) = self.slots.get_mut(&k) {
            s.used = self.frame;
            return Some(s.tex.clone());
        }
        if !self.wanted.iter().any(|r| r.key == k) {
            self.wanted.push(Req { key: k, path: path.to_string(), px, image, big });
        }
        None
    }

    pub fn peek(&mut self, id: u32, px: u32, big: bool) -> Option<TextureHandle> {
        let s = self.slots.get_mut(&key(id, px, big))?;
        s.used = self.frame;
        s.tex.clone()
    }

    pub fn any_small(&mut self, id: u32) -> Option<TextureHandle> {
        let frame = self.frame;
        self.slots
            .iter_mut()
            .filter(|(k, s)| (*k >> 24) as u32 == id && *k & 1 == 0 && s.tex.is_some())
            .max_by_key(|(k, _)| (*k >> 1) & 0x7f_ffff)
            .and_then(|(_, s)| {
                s.used = frame;
                s.tex.clone()
            })
    }

    pub fn end(&mut self) {
        {
            let mut q = self.shared.queue.lock().unwrap();
            let mut want = std::mem::take(&mut self.wanted);
            if let Some(i) = want.iter().position(|r| r.big) {
                let r = want.remove(i);
                want.insert(0, r);
            }
            q.want = want;
        }
        self.shared.cv.notify_all();
        if self.bytes > self.budget {
            let mut by_age: Vec<(u64, u64)> = self.slots.iter().filter(|(_, s)| s.used + 2 < self.frame).map(|(k, s)| (s.used, *k)).collect();
            by_age.sort_unstable();
            for (_, k) in by_age {
                if self.bytes <= self.budget * 3 / 4 {
                    break;
                }
                if let Some(s) = self.slots.remove(&k) {
                    self.bytes -= s.bytes;
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.bytes = 0;
        self.wanted.clear();
        self.shared.queue.lock().unwrap().want.clear();
    }

    pub fn forget(&mut self, ids: &HashSet<u32>) {
        let gone: Vec<u64> = self.slots.keys().copied().filter(|k| ids.contains(&((k >> 24) as u32))).collect();
        for k in gone {
            if let Some(s) = self.slots.remove(&k) {
                self.bytes -= s.bytes;
            }
        }
    }
}

impl Drop for Thumbs {
    fn drop(&mut self) {
        self.shared.queue.lock().unwrap().stop = true;
        self.shared.cv.notify_all();
    }
}

mod docking;
mod store;
mod water;
use rune::{
    AppSpec, ClipboardContent, ClipboardImage, Color, Cx, DragItem, El, Family, Image, ImageCache,
    MotionIntent, Shape, Spring, SystemMaterial, TextStyle, Transition, Tx, Waker, el, text,
};
use std::{
    path::{Path as FsPath, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

struct Entry {
    path: PathBuf,
    // Demo fixtures only. Saved history stays on disk and is read on drag/copy.
    png: Option<Arc<[u8]>>,
    // None until a thumbnail worker finishes.
    image: Option<Image>,
    size: (u32, u32),
    bytes_len: u64,
    ambient: u32,
    has_alpha: bool,
}
impl Entry {
    fn bytes(&self) -> Option<Arc<[u8]>> {
        self.png
            .clone()
            .or_else(|| std::fs::read(&self.path).ok().map(Into::into))
    }
    fn title(&self, idx: usize) -> String {
        let stem = self
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Capture");
        let clean = if stem.len() > 20 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
            format!("Capture_{:02}.png", idx + 1)
        } else {
            format!("{stem}.png")
        };
        if clean.chars().count() > 16 {
            let prefix: String = clean.chars().take(13).collect();
            format!("{prefix}…")
        } else {
            clean
        }
    }
    fn size_badge(&self) -> String {
        let kb = (self.bytes_len as f64 / 1024.).max(1.);
        if kb >= 1024. {
            format!("{:.1} MB", kb / 1024.)
        } else {
            format!("{:.0} KB", kb)
        }
    }
}

const THUMB_PX: u32 = 560;
type Thumb = (u32, u32, Vec<u8>, u32, bool);
type ThumbJob = (PathBuf, Option<image::DynamicImage>);

struct Thumbs {
    jobs: mpsc::Sender<ThumbJob>,
    done: mpsc::Receiver<(PathBuf, Thumb)>,
}
impl Thumbs {
    fn spawn(wake: Waker) -> Self {
        let (jobs, queue) = mpsc::channel::<ThumbJob>();
        let (finished, done) = mpsc::channel();
        let queue = Arc::new(Mutex::new(queue));
        let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
        for _ in 0..workers {
            let (queue, finished, wake) = (queue.clone(), finished.clone(), wake.clone());
            std::thread::spawn(move || {
                loop {
                    let Ok((path, decoded)) =
                        queue.lock().unwrap_or_else(|e| e.into_inner()).recv()
                    else {
                        return;
                    };
                    let decoded = decoded
                        .or_else(|| image::load_from_memory(&std::fs::read(&path).ok()?).ok());
                    if let Some(img) = decoded {
                        if finished.send((path, thumbnail(&img))).is_err() {
                            return;
                        }
                        wake();
                    }
                }
            });
        }
        Self { jobs, done }
    }
}

fn analyze_rgba(px: &[u8]) -> (u32, bool) {
    let (mut r_sum, mut g_sum, mut b_sum, mut weight) = (0f64, 0f64, 0f64, 0f64);
    let mut has_alpha = false;
    for chunk in px.as_chunks::<16>().0 {
        let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        if a < 245 {
            has_alpha = true;
        }
        if a > 32 {
            let max = r.max(g).max(b) as f64;
            let min = r.min(g).min(b) as f64;
            let sat = if max > 1. { (max - min) / max } else { 0. };
            let w = (a as f64 / 255.) * (0.35 + sat * 1.65);
            r_sum += r as f64 * w;
            g_sum += g as f64 * w;
            b_sum += b as f64 * w;
            weight += w;
        }
    }
    if weight < 1. {
        return (0x38bdf8, has_alpha);
    }
    let (mut r, mut g, mut b) = (r_sum / weight, g_sum / weight, b_sum / weight);
    let avg = (r + g + b) / 3.;
    r = (avg + (r - avg) * 1.35).clamp(48., 245.);
    g = (avg + (g - avg) * 1.35).clamp(48., 245.);
    b = (avg + (b - avg) * 1.35).clamp(48., 245.);
    (
        ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
        has_alpha,
    )
}

fn thumbnail(img: &image::DynamicImage) -> Thumb {
    let t = img.thumbnail(THUMB_PX, THUMB_PX).into_rgba8();
    let (w, h) = (t.width(), t.height());
    let raw = t.into_raw();
    let (ambient, has_alpha) = analyze_rgba(&raw);
    (w, h, raw, ambient, has_alpha)
}

fn png_size(path: &FsPath) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    ClipboardImage::from_png(head.to_vec()).map(|c| c.size())
}

// Exact 1:1 proportions from the 02 · Liquid Glass prototype:
// Top capsule: 312x50 compact -> 404x272 expanded, floating 8pt below the menu bar.
const TOP_PAD_X: f32 = 12.;
const TOP_PAD_TOP: f32 = 8.;
const TOP_PAD_BOTTOM: f32 = 14.;
const COMPACT_CARD_W: f32 = 312.;
const COMPACT_CARD_H: f32 = 50.;
const EXPANDED_CARD_W: f32 = 404.;
const EXPANDED_CARD_H: f32 = 272.;
const COMPACT_WIN_W: f32 = COMPACT_CARD_W + TOP_PAD_X * 2.;
const COMPACT_WIN_H: f32 = COMPACT_CARD_H + TOP_PAD_TOP + TOP_PAD_BOTTOM;
const EXPANDED_WIN_W: f32 = EXPANDED_CARD_W + TOP_PAD_X * 2.;
const EXPANDED_WIN_H: f32 = EXPANDED_CARD_H + TOP_PAD_TOP + TOP_PAD_BOTTOM;

// Side Liquid Glass capsule: 76x296 compact -> 364x296 expanded, floating 8pt from edge.
const SIDE_MARGIN_EDGE: f32 = 8.;
const SIDE_MARGIN_INWARD: f32 = 12.;
const SIDE_MARGIN_Y: f32 = 10.;
const SIDE_RAIL_W: f32 = 76.;
const SIDE_EXPANDED_CARD_W: f32 = 364.;
const SIDE_CARD_H: f32 = 296.;
const SIDE_COMPACT_WIN_W: f32 = SIDE_RAIL_W + SIDE_MARGIN_EDGE + SIDE_MARGIN_INWARD;
const SIDE_EXPANDED_WIN_W: f32 = SIDE_EXPANDED_CARD_W + SIDE_MARGIN_EDGE + SIDE_MARGIN_INWARD;
const SIDE_WIN_H: f32 = SIDE_CARD_H + SIDE_MARGIN_Y * 2.;

struct State {
    dock: docking::Dock,
    entries: Vec<Entry>,
    pointer: [f32; 2],
    clock: f64,
    ripples: Vec<([f32; 2], f64, [f32; 4])>,
    cache: ImageCache,
    thumbs: Option<Thumbs>,
    dir: PathBuf,
    selected: usize,
    scroll: f32,
    wheel_last: f64,
    wheel_total: f32,
    wheel_latched: bool,
    expanded: bool,
    copied_until: Option<f64>,
    paused: bool,
    pending: bool,
    menu_restore_compact: bool,
    menu_host: bool,
    menu_at: Option<(f64, f32, f32)>,
    change: isize,
    status: String,
    demo: bool,
}

impl State {
    fn new() -> Self {
        let demo = rune::is_headless() || std::env::args().any(|a| a == "--demo");
        let dir = std::env::var_os("IMAGE_SHELF_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap())
                    .join("Library/Application Support/Image Shelf/images")
            });
        let mut s = Self {
            dock: docking::Dock::default(),
            entries: vec![],
            pointer: [160., 30.],
            clock: 0.,
            ripples: vec![],
            cache: ImageCache::new(Arc::new(|| {})),
            thumbs: None,
            dir,
            selected: 0,
            scroll: 0.,
            wheel_last: -1.,
            wheel_total: 0.,
            wheel_latched: false,
            expanded: false,
            copied_until: None,
            paused: std::env::args().any(|a| a == "--paused"),
            pending: false,
            menu_restore_compact: false,
            menu_host: false,
            menu_at: None,
            change: -1,
            status: "Copy an image to start your shelf".into(),
            demo,
        };
        if demo {
            s.fixtures();
        } else {
            match store::scan(&s.dir) {
                Ok(files) => {
                    for (path, len) in files.into_iter().take(store::MAX_ITEMS) {
                        if len > store::MAX_IMAGE as u64 {
                            continue;
                        }
                        if let Some(size) = png_size(&path) {
                            s.entries.push(Entry {
                                path,
                                png: None,
                                image: None,
                                size,
                                bytes_len: len,
                                ambient: 0x38bdf8,
                                has_alpha: false,
                            });
                        }
                    }
                }
                Err(e) => s.status = format!("Cannot load history: {e}"),
            }
        }
        s
    }

    fn capture(&mut self, content: &ClipboardContent) {
        if self.paused {
            return;
        }
        let Some(image) = content.image() else {
            return;
        };
        let raw_png = image.png();
        let bytes_len = raw_png.len() as u64;
        match store::save(&self.dir, raw_png) {
            Ok((path, decoded)) => {
                if self.entries.iter().any(|e| e.path == path) {
                    return;
                }
                let (thumb, ambient, has_alpha) = match &self.thumbs {
                    Some(t) => {
                        let _ = t.jobs.send((path.clone(), decoded));
                        (None, 0x38bdf8, false)
                    }
                    None => match decoded {
                        Some(d) => {
                            let (w, h, px, amb, alpha) = thumbnail(&d);
                            (Some(self.cache.rgba(w, h, px)), amb, alpha)
                        }
                        None => (None, 0x38bdf8, false),
                    },
                };
                self.entries.insert(
                    0,
                    Entry {
                        path,
                        png: None,
                        image: thumb,
                        size: image.size(),
                        bytes_len,
                        ambient,
                        has_alpha,
                    },
                );
                self.selected = 0;
                self.scroll = 0.;
                self.status = "Image saved locally. Drag it into another app.".into();
            }
            Err(e) => self.status = e.to_string(),
        }
    }

    fn fixtures(&mut self) {
        self.entries.clear();
        let specs: [(&str, (u32, u32)); 6] = [
            ("Alpine_Sunset.png", (640, 400)),
            ("Dashboard_Card.png", (640, 400)),
            ("AppIcon_3D.png", (512, 512)),
            ("Shader_Study.png", (640, 400)),
            ("Botanical.png", (640, 400)),
            ("Type_Specimen.png", (640, 400)),
        ];
        for (n, (name, (w, h))) in specs.into_iter().enumerate() {
            let img = image::RgbaImage::from_fn(w, h, |x, y| {
                let xf = x as f32 / w as f32;
                let yf = y as f32 / h as f32;
                let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t.clamp(0., 1.);
                match n {
                    0 => {
                        let (mut r, mut g, mut b) = if yf < 0.55 {
                            let t = yf / 0.55;
                            (lerp(30., 190., t), lerp(27., 24., t), lerp(75., 93., t))
                        } else {
                            let t = (yf - 0.55) / 0.45;
                            (lerp(190., 253., t), lerp(24., 224., t), lerp(93., 71., t))
                        };
                        let sun_d = ((xf - 0.66).powi(2) * 2.4 + (yf - 0.44).powi(2)).sqrt();
                        if sun_d < 0.14 {
                            let glow = (1. - sun_d / 0.14).powf(0.6);
                            r = lerp(r, 254., glow);
                            g = lerp(g, 240., glow);
                            b = lerp(b, 138., glow);
                        }
                        let ridge1 = 0.68
                            - 0.22 * (1. - ((xf - 0.25).abs() * 3.8).min(1.))
                            - 0.17 * (1. - ((xf - 0.72).abs() * 3.4).min(1.));
                        let ridge2 = 0.78
                            - 0.18 * (1. - ((xf - 0.52).abs() * 3.6).min(1.))
                            - 0.10 * (1. - ((xf - 0.88).abs() * 4.5).min(1.));
                        if yf > ridge2 {
                            (r, g, b) = (22., 10., 38.);
                        } else if yf > ridge1 {
                            (r, g, b) = (49., 16., 66.);
                        }
                        image::Rgba([r as u8, g as u8, b as u8, 255])
                    }
                    1 => {
                        let (mut r, mut g, mut b) = (15f32, 23f32, 42f32);
                        if (0.07..0.93).contains(&xf) && (0.10..0.90).contains(&yf) {
                            (r, g, b) = (30., 41., 59.);
                            let curve = 0.68 - 0.32 * xf - 0.11 * (xf * 11.).sin()
                                + 0.05 * (xf * 23.).cos();
                            let dist = (yf - curve).abs();
                            if dist < 0.018 {
                                let k = 1. - dist / 0.018;
                                r = lerp(r, 56., k);
                                g = lerp(g, 189., k);
                                b = lerp(b, 248., k);
                            } else if yf > curve && yf < 0.84 {
                                let fill_k = (1. - (yf - curve) / 0.45).clamp(0., 1.) * 0.26;
                                r = lerp(r, 56., fill_k);
                                g = lerp(g, 189., fill_k);
                                b = lerp(b, 248., fill_k);
                            }
                        }
                        image::Rgba([r as u8, g as u8, b as u8, 255])
                    }
                    2 => {
                        let dx = (xf - 0.5).abs();
                        let dy = (yf - 0.5).abs();
                        let q = ((dx - 0.18).max(0.).powi(2) + (dy - 0.18).max(0.).powi(2)).sqrt();
                        if q > 0.165 {
                            return image::Rgba([0, 0, 0, 0]);
                        }
                        let edge_aa = ((0.165 - q) / 0.012).clamp(0., 1.);
                        let mut r = lerp(192., 14., (xf + yf) * 0.5);
                        let mut g = lerp(132., 165., yf);
                        let mut b = lerp(252., 233., xf);
                        let ring = (((xf - 0.5).powi(2) + (yf - 0.5).powi(2)).sqrt() - 0.16).abs();
                        if ring < 0.024 {
                            let rk = (1. - ring / 0.024) * 0.92;
                            r = lerp(r, 255., rk);
                            g = lerp(g, 255., rk);
                            b = lerp(b, 255., rk);
                        }
                        image::Rgba([r as u8, g as u8, b as u8, (edge_aa * 255.) as u8])
                    }
                    3 => {
                        let d = ((xf - 0.5).powi(2) * 1.8 + (yf - 0.5).powi(2)).sqrt();
                        let wave = ((d * 28.).cos() * 0.5 + 0.5) * (-d * 2.4).exp();
                        let r = lerp(24., 244., wave);
                        let g = lerp(20., 63., wave * 0.85);
                        let b = lerp(27., 94., wave);
                        image::Rgba([r as u8, g as u8, b as u8, 255])
                    }
                    4 => {
                        let d1 = ((xf - 0.40).powi(2) * 1.6 + (yf - 0.55).powi(2)).sqrt();
                        let d2 = ((xf - 0.62).powi(2) * 1.6 + (yf - 0.42).powi(2)).sqrt();
                        let (mut r, mut g, mut b) = (6f32, 40f32, 30f32);
                        if d1 < 0.34 {
                            let k = (1. - d1 / 0.34).powf(0.7) * 0.7;
                            r = lerp(r, 16., k);
                            g = lerp(g, 185., k);
                            b = lerp(b, 129., k);
                        }
                        if d2 < 0.28 {
                            let k = (1. - d2 / 0.28).powf(0.7) * 0.82;
                            r = lerp(r, 110., k);
                            g = lerp(g, 231., k);
                            b = lerp(b, 183., k);
                        }
                        image::Rgba([r as u8, g as u8, b as u8, 255])
                    }
                    _ => {
                        let (mut r, mut g, mut b) = (245f32, 245f32, 240f32);
                        if (0.10..0.42).contains(&xf) && (0.24..0.76).contains(&yf) {
                            (r, g, b) = (17., 17., 17.);
                        } else if (0.50..0.86).contains(&xf) && (0.30..0.37).contains(&yf) {
                            (r, g, b) = (34., 34., 34.);
                        } else if (0.50..0.78).contains(&xf) && (0.44..0.50).contains(&yf) {
                            (r, g, b) = (120., 120., 120.);
                        }
                        let dot = ((xf - 0.80).powi(2) * 2.3 + (yf - 0.68).powi(2)).sqrt();
                        if dot < 0.085 {
                            (r, g, b) = (239., 68., 68.);
                        }
                        image::Rgba([r as u8, g as u8, b as u8, 255])
                    }
                }
            });
            let mut out = std::io::Cursor::new(vec![]);
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            let png_bytes: Arc<[u8]> = out.into_inner().into();
            let bytes_len = png_bytes.len() as u64;
            let (tw, th, px, ambient, has_alpha) = thumbnail(&img.into());
            self.entries.push(Entry {
                path: PathBuf::from(name),
                png: Some(png_bytes),
                image: Some(self.cache.rgba(tw, th, px)),
                size: (w, h),
                bytes_len,
                ambient,
                has_alpha,
            });
        }
        for e in &self.entries {
            if let Some(image) = &e.image {
                let _ = image.wait();
            }
        }
        self.status = "Demo images • clipboard monitoring disabled".into();
    }

    fn drag(&self, i: usize) -> Option<DragItem> {
        let e = self.entries.get(i)?;
        let item = DragItem::new().png(e.bytes()?.to_vec());
        Some(if self.demo {
            item
        } else {
            item.file(e.path.clone())
        })
    }
}

#[cfg(target_os = "macos")]
fn pasteboard_state() -> (isize, bool) {
    use objc2::{
        class, msg_send,
        runtime::{AnyObject, Bool},
    };
    use objc2_foundation::NSString;
    unsafe {
        let board: *mut AnyObject = msg_send![class!(NSPasteboard), generalPasteboard];
        let count: isize = msg_send![board, changeCount];
        let types: *mut AnyObject = msg_send![board, types];
        let private = [
            "org.nspasteboard.ConcealedType",
            "org.nspasteboard.TransientType",
            "org.nspasteboard.AutoGeneratedType",
        ]
        .iter()
        .any(|name| {
            let name = NSString::from_str(name);
            let found: Bool = msg_send![types, containsObject: &*name];
            found.as_bool()
        });
        (count, private)
    }
}
#[cfg(not(target_os = "macos"))]
fn pasteboard_state() -> (isize, bool) {
    (0, false)
}

fn setup(s: &mut State, wake: Waker) {
    if s.demo {
        return;
    }
    s.cache = ImageCache::new(wake.clone());
    let thumbs = Thumbs::spawn(wake.clone());
    for e in &s.entries {
        let _ = thumbs.jobs.send((e.path.clone(), None));
    }
    s.thumbs = Some(thumbs);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(700));
            wake();
        }
    });
}

fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.clock = t;
    let mut loaded = false;
    if let Some(thumbs) = &s.thumbs {
        while let Ok((path, (w, h, px, ambient, has_alpha))) = thumbs.done.try_recv() {
            if let Some(e) = s.entries.iter_mut().find(|e| e.path == path) {
                e.image = Some(s.cache.rgba(w, h, px));
                e.ambient = ambient;
                e.has_alpha = has_alpha;
                loaded = true;
            }
        }
    }
    if let Some(due) = s.copied_until {
        if t >= due {
            s.copied_until = None;
            loaded = true;
        } else {
            tx.wake_after(Duration::from_millis(80));
        }
    }
    if let Some((due, x, y)) = s.menu_at {
        if t >= due {
            s.menu_at = None;
            tx.popup_menu(menu(s), (x, y), menu_action);
        } else {
            tx.wake_after(Duration::from_millis(30));
        }
    }
    let old_edge = s.dock.edge;
    if docking::tick(&mut s.dock) {
        tx.wake_after(Duration::from_millis(16));
    }
    let edge_changed = old_edge != s.dock.edge;
    if edge_changed {
        tx.resize_window_instant(shelf_size(s));
        tx.wake_after(Duration::from_millis(16));
    }
    let animating = loaded || edge_changed || !s.ripples.is_empty();
    s.ripples.retain(|(_, start, _)| t - start < 0.8);
    if !s.ripples.is_empty() {
        tx.wake_after(Duration::from_millis(16));
    }
    if s.demo || s.paused || s.pending {
        return animating;
    }
    let (count, private) = pasteboard_state();
    if count != s.change {
        s.change = count;
        if !private {
            s.pending = true;
            tx.read_clipboard(|s: &mut State, c, _| {
                s.pending = false;
                s.capture(c);
            });
        }
    }
    animating
}

fn c(n: u32) -> Color {
    rune::color::hex(&format!("#{n:06x}")).unwrap()
}

fn sans_label(v: impl AsRef<str>, size: f32, weight: f32, color: Color) -> El {
    text(v.as_ref())
        .font(TextStyle::new(size, weight).features("'tnum' 1"))
        .color(color)
}

fn mono_label(v: impl AsRef<str>, size: f32, weight: f32, color: Color) -> El {
    text(v.as_ref())
        .font(
            TextStyle::new(size, weight)
                .family(Family::SystemMono)
                .features("'tnum' 1"),
        )
        .color(color)
}

macro_rules! glyph {
    ($name:ident, $id:literal, $d:literal) => {
        static $name: rune::Icon = rune::Icon {
            name: $id,
            view: 16.,
            body: concat!(
                r#"<path d=""#,
                $d,
                r#"" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
            ),
        };
    };
}
glyph!(UP, "shelf-up", "M4.5 10L8 6.5L11.5 10");
glyph!(DOWN, "shelf-down", "M4.5 6.5L8 10L11.5 6.5");
glyph!(LEFT, "shelf-left", "M9.5 4.5L6 8L9.5 11.5");
glyph!(RIGHT, "shelf-right", "M6.5 4.5L10 8L6.5 11.5");
glyph!(PLUS, "shelf-plus", "M8 4.5V11.5M4.5 8H11.5");
glyph!(MINUS, "shelf-minus", "M4.5 8H11.5");
glyph!(
    EMPTY,
    "shelf-empty",
    "M2.5 3.5H13.5V12.5H2.5ZM4.5 10.5L6.8 8.2L8.6 10L10.4 8.5L12 10.5"
);

fn copy_selected(s: &mut State, tx: &mut Tx) {
    if let Some(e) = s.entries.get(s.selected)
        && let Some(img) = e.bytes().and_then(ClipboardImage::from_png)
    {
        tx.copy_rich(ClipboardContent::default().with_image(img));
        s.copied_until = Some(s.clock + 1.0);
        s.status = format!("Copied {}", e.title(s.selected));
        tx.wake_after(Duration::from_millis(60));
    }
}

fn copy_button(s: &State, x: f32, y: f32, w: f32, h: f32, full: bool) -> El {
    let enabled = !s.entries.is_empty();
    let copied = s.copied_until.is_some_and(|due| s.clock < due);
    let txt = if copied {
        "Copied!"
    } else if full {
        "Copy Image"
    } else {
        "Copy"
    };
    let bg = if copied { c(0xd1fae5) } else { c(0xffffff) };
    let fg = if copied { c(0x065f46) } else { c(0x020617) };
    let mut b = el()
        .key("copy-action")
        .absolute(x, y)
        .size(w, h)
        .radius(if full { 10. } else { 11. })
        .continuous_corners()
        .fill(bg)
        .shadow(2., 6., c(0x000000).alpha(0.24))
        .hit()
        .label("Copy image")
        .help("Copy selected image to clipboard")
        .center()
        .child(sans_label(txt, 11., 600., fg));
    if enabled {
        b = b
            .cursor(rune::Cursor::Pointer)
            .hover(if copied { c(0xa7f3d0) } else { c(0xf1f5f9) })
            .press(c(0xe2e8f0))
            .press_scale(0.95)
            .on_press_tx(copy_selected);
    } else {
        b = b.disabled(true).opacity(0.38);
    }
    b
}

fn checker_shape(w: f32, h: f32, cell: f32) -> Shape {
    let mut s = Shape::new();
    let col = c(0xffffff).alpha(0.055);
    let cols = (w / cell).ceil() as usize;
    let rows = (h / cell).ceil() as usize;
    for r in 0..rows {
        for c_idx in 0..cols {
            if (r + c_idx) % 2 == 0 {
                let x0 = c_idx as f32 * cell;
                let y0 = r as f32 * cell;
                let x1 = (x0 + cell).min(w);
                let y1 = (y0 + cell).min(h);
                s = s.fill([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], col);
            }
        }
    }
    s
}

// Full-bleed cover card for photos, or alpha-checkerboard centered card for transparent PNGs,
// matching the 02 · Liquid Glass prototype thumbnails and loupe stage.
fn media_card(
    e: &Entry,
    [x, y, w, h]: [f32; 4],
    r: f32,
    (border_w, border_col): (f32, Color),
) -> El {
    let (ew, eh) = (e.size.0.max(1) as f32, e.size.1.max(1) as f32);
    let ratio = if e.has_alpha {
        (w / ew).min(h / eh) * 0.84
    } else {
        (w / ew).max(h / eh)
    };
    let (pw, ph) = (ew * ratio, eh * ratio);
    let mut frame = el()
        .absolute(x, y)
        .size(w, h)
        .fill(c(0x090c14).alpha(if e.has_alpha { 0.34 } else { 0.65 }))
        .radius(r)
        .continuous_corners()
        .clip_rounded(r);
    if e.has_alpha {
        frame = frame.shape(checker_shape(w, h, (r * 0.65).clamp(5.5, 10.)));
    }
    if let Some(image) = &e.image {
        frame = frame.child(
            el().absolute((w - pw) / 2., (h - ph) / 2.)
                .size(pw, ph)
                .pixels(image.pixels()),
        );
    }
    frame.child(
        el().absolute(0., 0.)
            .size(w, h)
            .radius(r)
            .continuous_corners()
            .border(border_w, border_col),
    )
}

fn wheel(s: &mut State, event: rune::WheelEvent, tx: &mut Tx) -> bool {
    if event.precise && event.phase == rune::GesturePhase::Ended && !event.momentum {
        s.wheel_total = 0.;
        s.wheel_latched = false;
        return true;
    }
    let delta = if event.dx.abs() > event.dy.abs() {
        event.dx
    } else {
        event.dy
    };
    if !delta.is_finite() || delta == 0. || s.entries.is_empty() {
        return true;
    }
    if event.momentum {
        return true;
    }
    if event.precise {
        if event.phase == rune::GesturePhase::Began || s.clock - s.wheel_last > 0.2 {
            s.wheel_total = 0.;
            s.wheel_latched = false;
        }
        s.wheel_last = s.clock;
        if event.phase == rune::GesturePhase::Ended {
            s.wheel_total = 0.;
            s.wheel_latched = false;
            return true;
        }
        if s.wheel_latched {
            return true;
        }
        s.wheel_total += delta;
        if s.wheel_total.abs() < 10. {
            return true;
        }
        s.wheel_latched = true;
    } else {
        if s.clock - s.wheel_last < 0.12 {
            return true;
        }
        s.wheel_last = s.clock;
        s.wheel_total = delta;
    }
    if s.wheel_total < 0. {
        s.selected = (s.selected + 1).min(s.entries.len().saturating_sub(1));
    } else {
        s.selected = s.selected.saturating_sub(1);
    }
    s.scroll = s.selected as f32;
    tx.animated();
    true
}

fn side_rail_x(s: &State) -> f32 {
    let win_w = shelf_size(s).0;
    if s.dock.edge == docking::Edge::Right {
        win_w - SIDE_MARGIN_EDGE - SIDE_RAIL_W
    } else {
        SIDE_MARGIN_EDGE
    }
}

// Coordinates are in window points. Only thumbnail areas refract;
// loupe preview, buttons, headers and outer margins never refract.
fn thumbnail_area(s: &State) -> Option<[f32; 4]> {
    if s.entries.is_empty() {
        return None;
    }
    let p = s.pointer;
    let side = matches!(s.dock.edge, docking::Edge::Left | docking::Edge::Right);
    let areas = if side {
        let rail_x = side_rail_x(s);
        vec![[rail_x + 10., SIDE_MARGIN_Y + 38., 56., 206.]]
    } else {
        let mut areas = vec![[TOP_PAD_X + 12., TOP_PAD_TOP + 10., 110., 30.]];
        if s.expanded {
            areas.push([TOP_PAD_X + 276., TOP_PAD_TOP + 132., 106., 60.]);
        }
        areas
    };
    areas
        .into_iter()
        .find(|r| p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3])
}

fn ripple(s: &mut State) {
    let Some(area) = thumbnail_area(s) else {
        return;
    };
    if s.ripples.len() >= 5 {
        s.ripples.remove(0);
    }
    s.ripples.push((s.pointer, s.clock, area));
}

fn toggle(s: &mut State, tx: &mut Tx) {
    s.ripples.clear();
    s.expanded = !s.expanded;
    tx.motion(Spring::bouncy(0.38, 0.12));
    tx.resize_window(shelf_size(s));
    tx.wake_after(Duration::from_millis(16));
}

fn menu(s: &State) -> rune::PopupMenu {
    rune::PopupMenu::new()
        .item(rune::PopupItem::new("copy", "Copy image").enabled(!s.entries.is_empty()))
        .item(rune::PopupItem::new(
            "pause",
            if s.paused {
                "Resume capture"
            } else {
                "Pause capture"
            },
        ))
        .item(rune::PopupItem::new("folder", "Open history folder"))
        .separator()
        .item(rune::PopupItem::new("snap-left", "Snap left"))
        .item(rune::PopupItem::new("snap-right", "Snap right"))
        .item(rune::PopupItem::new("snap-top", "Return to top"))
        .separator()
        .item(rune::PopupItem::new("status", &s.status).enabled(false))
        .separator()
        .item(rune::PopupItem::new("quit", "Quit Image Shelf"))
}

fn menu_action(s: &mut State, picked: Option<&str>, tx: &mut Tx) {
    if s.menu_host {
        s.menu_host = false;
        tx.resize_window_instant(shelf_size(s));
        tx.wake_after(Duration::from_millis(16));
    }
    if s.menu_restore_compact {
        s.menu_restore_compact = false;
        s.expanded = false;
        tx.motion(Spring::bouncy(0.38, 0.12));
        tx.resize_window(shelf_size(s));
        tx.wake_after(Duration::from_millis(16));
    }
    match picked {
        Some("copy") => copy_selected(s, tx),
        Some("pause") => s.paused = !s.paused,
        Some("folder") => {
            if !s.demo {
                let _ = std::process::Command::new("open").arg(&s.dir).spawn();
            }
        }
        Some("snap-left") => set_edge(s, docking::Edge::Left, tx),
        Some("snap-right") => set_edge(s, docking::Edge::Right, tx),
        Some("snap-top") => set_edge(s, docking::Edge::Top, tx),
        Some("quit") => tx.quit(),
        _ => {}
    }
}

fn shelf_size(s: &State) -> (f32, f32) {
    if matches!(s.dock.edge, docking::Edge::Left | docking::Edge::Right) {
        (
            if s.expanded {
                SIDE_EXPANDED_WIN_W
            } else {
                SIDE_COMPACT_WIN_W
            },
            SIDE_WIN_H,
        )
    } else if s.menu_host {
        (
            if s.expanded {
                EXPANDED_WIN_W
            } else {
                COMPACT_WIN_W
            },
            420.,
        )
    } else if s.expanded {
        (EXPANDED_WIN_W, EXPANDED_WIN_H)
    } else {
        (COMPACT_WIN_W, COMPACT_WIN_H)
    }
}

fn set_edge(s: &mut State, edge: docking::Edge, tx: &mut Tx) {
    s.ripples.clear();
    docking::place(&mut s.dock, edge);
    tx.resize_window_instant(shelf_size(s));
    tx.wake_after(Duration::from_millis(16));
}

// Smoked-quartz Liquid Glass surface:
// Note that on native macOS, `SystemMaterial::Glass` intentionally skips the host element's own
// `.fill(...)` so `NSGlassEffectView` shines through; we therefore paint the smoked-quartz gradient
// (`rgba(28,32,44,0.72) -> rgba(15,18,26,0.82)`) on an explicit child tint layer so live macOS and
// headless renders share the exact dark-quartz color from the demo while refracting the desktop!
fn glass_shell(x: f32, y: f32, w: f32, h: f32, r: f32, ambient: u32) -> El {
    el().key("liquid-shell")
        .absolute(x, y)
        .size(w, h)
        .radius(r)
        .continuous_corners()
        .clip_rounded(r)
        .system_material(SystemMaterial::Glass)
        .fill(c(0x141824).alpha(0.76))
        .shadow(10., 26., c(0x000000).alpha(0.48))
        .motion(Spring::bouncy(0.38, 0.12))
        .motion_intent(MotionIntent::OVERSHOOT)
        // Explicit smoked-quartz tint over native NSGlassEffectView.
        .child(
            el().absolute(0., 0.)
                .size(w, h)
                .gradient(c(0x1c202c).alpha(0.72), c(0x0f121a).alpha(0.82)),
        )
        // Top specular sheen (`radial-gradient(90% 80% at 50% 0%, rgba(255,255,255,0.11), transparent)`).
        .child(el().absolute(0., 0.).size(w, h).radial_gradient(
            [0.5, 0.0],
            [w * 0.85, 95.],
            [(0., c(0xffffff).alpha(0.12)), (1., c(0xffffff).alpha(0.0))],
        ))
        // Bottom ambient color bleed from active image.
        .child(el().absolute(0., 0.).size(w, h).radial_gradient(
            [0.5, 1.0],
            [w * 0.85, 170.],
            [(0., c(ambient).alpha(0.22)), (1., c(ambient).alpha(0.0))],
        ))
}

fn glass_rim(w: f32, h: f32, r: f32) -> El {
    el().key("liquid-rim")
        .absolute(0., 0.)
        .size(w, h)
        .radius(r)
        .continuous_corners()
        .border_gradient(0.85, c(0xffffff).alpha(0.28), c(0xffffff).alpha(0.10))
}

fn side_view(s: &State) -> El {
    let (win_w, win_h) = shelf_size(s);
    let right = s.dock.edge == docking::Edge::Right;
    let card_w = if s.expanded {
        SIDE_EXPANDED_CARD_W
    } else {
        SIDE_RAIL_W
    };
    let card_r = 22.;
    let shell_x = if right {
        win_w - SIDE_MARGIN_EDGE - card_w
    } else {
        SIDE_MARGIN_EDGE
    };
    let rail_local_x = if right { card_w - SIDE_RAIL_W } else { 0. };
    let ambient = s.entries.get(s.selected).map_or(0x38bdf8, |e| e.ambient);
    let has_prev = s.selected > 0;
    let has_next = s.selected + 1 < s.entries.len();

    let mut shell = glass_shell(shell_x, SIDE_MARGIN_Y, card_w, SIDE_CARD_H, card_r, ambient);

    // 1. Top Rail Header (status dot + native drag grip + monospace counter + click-to-toggle).
    let status_col = c(if s.paused { 0xf59e0b } else { 0x34d399 });
    let count_str = if s.entries.is_empty() {
        "0/0".into()
    } else {
        format!("{}/{}", s.selected + 1, s.entries.len())
    };
    shell = shell
        .child(
            el().key("side-header-toggle")
                .absolute(rail_local_x + 6., 8.)
                .size(64., 24.)
                .radius(7.)
                .hit()
                .cursor(rune::Cursor::Pointer)
                .hover(c(0xffffff).alpha(0.06))
                .on_press_tx(toggle)
                .child(
                    el().absolute(6., 8.)
                        .size(7., 7.)
                        .radius(3.5)
                        .fill(status_col)
                        .shadow(0., 6., status_col.alpha(0.75)),
                )
                .child(
                    mono_label(count_str, 9.5, 500., c(0xffffff).alpha(0.58)).absolute(40., 5.5),
                ),
        )
        .child(
            el().key("notch-header")
                .absolute(rail_local_x + 23., 8.)
                .size(22., 24.)
                .hit()
                .window_drag()
                .cursor(rune::Cursor::Grab)
                .center()
                .child(
                    el().size(16., 3.5)
                        .radius(1.75)
                        .fill(c(0xffffff).alpha(0.28)),
                ),
        );

    // 2. Vertical 4-Up Thumbnail Strip (56x44 rounded-xl cards, exact match to demo).
    let pitch = 52.;
    let strip_h = 206.;
    let offset = if s.entries.is_empty() {
        0.
    } else {
        (s.scroll * pitch).min((s.entries.len() as f32 * pitch - strip_h).max(0.))
    };
    let fade_t = if offset > 0.5 { 8. } else { 0. };
    let fade_b = if offset + strip_h < s.entries.len() as f32 * pitch - 0.5 {
        8.
    } else {
        0.
    };
    let mut strip = el()
        .key("thumbnail-strip")
        .absolute(rail_local_x + 10., 38.)
        .size(56., strip_h)
        .clip_fade_linear(0., fade_t, 0., fade_b);
    if s.entries.is_empty() {
        strip = strip.child(
            el().absolute(0., 0.)
                .size(56., 44.)
                .radius(11.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.06))
                .border(0.8, c(0xffffff).alpha(0.15))
                .center()
                .child(rune::icon(&EMPTY, 16.).color(c(0xffffff).alpha(0.45))),
        );
    } else {
        for slot in 0..s.entries.len() {
            let y = slot as f32 * pitch - offset;
            if y < -pitch || y > strip_h + pitch {
                continue;
            }
            let e = &s.entries[slot];
            let active = slot == s.selected;
            let mut card = media_card(
                e,
                [0., y, 56., 44.],
                11.,
                (
                    if active { 1.8 } else { 0.8 },
                    if active {
                        c(0xffffff)
                    } else {
                        c(0xffffff).alpha(0.16)
                    },
                ),
            )
            .key(format!("image-{slot}"))
            .opacity(if active { 1. } else { 0.56 })
            .hit()
            .cursor(rune::Cursor::Grab)
            .on_press(move |s: &mut State| {
                ripple(s);
                s.selected = slot;
                s.scroll = slot as f32;
            })
            .drag_out(move |s: &State| s.drag(slot))
            .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}"));
            if active {
                card = card.shadow(2., 8., c(0x000000).alpha(0.38));
            }
            strip = strip.child(card);
        }
    }
    shell = shell.child(strip);

    // 3. Bottom Rail Controls: compact ↑ / ↓ / expand when collapsed, or full-width `‹ Hide` when expanded!
    if s.expanded {
        let hide_label = if right { "Hide ›" } else { "‹ Hide" };
        shell = shell.child(
            el().key("expand")
                .absolute(rail_local_x + 10., 256.)
                .size(56., 28.)
                .radius(9.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.11))
                .border(0.8, c(0xffffff).alpha(0.16))
                .hit()
                .label("Collapse shelf")
                .cursor(rune::Cursor::Pointer)
                .hover(c(0xffffff).alpha(0.19))
                .press_scale(0.95)
                .on_press_tx(toggle)
                .center()
                .child(sans_label(hide_label, 10.5, 500., c(0xf8fafc))),
        );
    } else {
        let nav_btn = |icon: &'static rune::Icon, x: f32, enabled: bool, tip: &'static str| {
            let mut b = el()
                .absolute(x, 256.)
                .size(17., 28.)
                .radius(8.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.10))
                .border(0.7, c(0xffffff).alpha(0.15))
                .hit()
                .label(tip)
                .center()
                .child(rune::icon(icon, 13.).color(c(0xf8fafc)));
            if enabled {
                b = b
                    .cursor(rune::Cursor::Pointer)
                    .hover(c(0xffffff).alpha(0.18))
                    .press_scale(0.94);
            } else {
                b = b.disabled(true).opacity(0.35);
            }
            b
        };
        shell = shell
            .child(
                nav_btn(&UP, rail_local_x + 9., has_prev, "Previous image")
                    .key("select-previous")
                    .on_press(|s: &mut State| {
                        s.selected = s.selected.saturating_sub(1);
                        s.scroll = s.selected as f32;
                    }),
            )
            .child(
                nav_btn(&DOWN, rail_local_x + 29., has_next, "Next image")
                    .key("select-next")
                    .on_press(|s: &mut State| {
                        if s.selected + 1 < s.entries.len() {
                            s.selected += 1;
                            s.scroll = s.selected as f32;
                        }
                    }),
            )
            .child(
                nav_btn(
                    if right { &LEFT } else { &RIGHT },
                    rail_local_x + 49.,
                    true,
                    "Expand shelf",
                )
                .key("expand")
                .on_press_tx(toggle),
            );
    }

    // 4. Expanded Side Studio (matches demo: title + dim/size + tag, full-bleed preview, Copy Image + ↑ ↓).
    if s.expanded {
        let studio_x = if right { 0. } else { SIDE_RAIL_W };
        let mut studio = el()
            .key("side-studio")
            .absolute(studio_x, 0.)
            .size(SIDE_EXPANDED_CARD_W - SIDE_RAIL_W, SIDE_CARD_H)
            .transition(
                Transition::blur_replace()
                    .combined(Transition::offset(if right { 12. } else { -12. }, 0.)),
            );

        if let Some(e) = s.entries.get(s.selected) {
            studio = studio
                .child(sans_label(e.title(s.selected), 12., 600., c(0xf8fafc)).absolute(12., 11.))
                .child(
                    mono_label(
                        format!("{}×{} · {}", e.size.0, e.size.1, e.size_badge()),
                        9.5,
                        500.,
                        c(0xffffff).alpha(0.52),
                    )
                    .absolute(12., 27.),
                )
                .child(
                    el().absolute(236., 13.)
                        .size(40., 18.)
                        .radius(5.)
                        .continuous_corners()
                        .fill(c(0xffffff).alpha(0.10))
                        .center()
                        .child(sans_label(
                            if e.has_alpha { "Alpha" } else { "PNG" },
                            9.5,
                            500.,
                            c(0xffffff).alpha(0.68),
                        )),
                )
                .child(
                    media_card(
                        e,
                        [12., 48., 264., 198.],
                        13.,
                        (1., c(0xffffff).alpha(0.18)),
                    )
                    .key("preview")
                    .shadow(5., 18., c(0x000000).alpha(0.45))
                    .hit()
                    .cursor(rune::Cursor::Grab)
                    .drag_out(|s: &State| s.drag(s.selected))
                    .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
                );
        } else {
            studio = studio.child(
                el().key("preview")
                    .absolute(12., 48.)
                    .size(264., 198.)
                    .radius(13.)
                    .continuous_corners()
                    .fill(c(0x000000).alpha(0.28))
                    .border(1., c(0xffffff).alpha(0.12))
                    .center()
                    .child(sans_label(
                        "Copy an image (⌘C) to capture",
                        11.,
                        500.,
                        c(0xffffff).alpha(0.55),
                    )),
            );
        }

        let step_sq = |icon: &'static rune::Icon, x: f32, enabled: bool, tip: &'static str| {
            let mut b = el()
                .absolute(x, 256.)
                .size(28., 28.)
                .radius(9.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.10))
                .border(0.8, c(0xffffff).alpha(0.14))
                .hit()
                .label(tip)
                .center()
                .child(rune::icon(icon, 14.).color(c(0xf8fafc)));
            if enabled {
                b = b
                    .cursor(rune::Cursor::Pointer)
                    .hover(c(0xffffff).alpha(0.18))
                    .press_scale(0.94);
            } else {
                b = b.disabled(true).opacity(0.35);
            }
            b
        };

        studio = studio
            .child(copy_button(s, 12., 256., 200., 28., true))
            .child(
                step_sq(&UP, 218., has_prev, "Previous image")
                    .key("select-previous")
                    .on_press(|s: &mut State| {
                        s.selected = s.selected.saturating_sub(1);
                        s.scroll = s.selected as f32;
                    }),
            )
            .child(
                step_sq(&DOWN, 250., has_next, "Next image")
                    .key("select-next")
                    .on_press(|s: &mut State| {
                        if s.selected + 1 < s.entries.len() {
                            s.selected += 1;
                            s.scroll = s.selected as f32;
                        }
                    }),
            );

        shell = shell.child(studio);
    }

    shell = shell.child(glass_rim(card_w, SIDE_CARD_H, card_r));

    el().key("side-dock")
        .size(win_w, win_h)
        .on_pointer_move(|s: &mut State, p| s.pointer = [p.x, p.y])
        .on_wheel(wheel)
        .on_secondary_press(|s: &mut State, press: rune::PointerPress, tx| {
            s.ripples.clear();
            if !s.expanded {
                s.menu_restore_compact = true;
                s.expanded = true;
                tx.resize_window_instant(shelf_size(s));
                tx.wake_after(Duration::from_millis(16));
            }
            let x = if s.dock.edge == docking::Edge::Left {
                92.
            } else {
                16.
            };
            s.menu_at = Some((s.clock + 0.12, x, press.y.min(80.)));
            tx.wake_after(Duration::from_millis(150));
        })
        .child(shell)
        .child(rune::effect(77).size(win_w, win_h).effect_backdrop(true))
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    if matches!(s.dock.edge, docking::Edge::Left | docking::Edge::Right) {
        return side_view(s);
    }
    let (win_w, win_h) = shelf_size(s);
    let card_w = if s.expanded {
        EXPANDED_CARD_W
    } else {
        COMPACT_CARD_W
    };
    let card_h = if s.expanded {
        EXPANDED_CARD_H
    } else {
        COMPACT_CARD_H
    };
    let card_r = 22.;
    let ambient = s.entries.get(s.selected).map_or(0x38bdf8, |e| e.ambient);
    let has_prev = s.selected > 0;
    let has_next = s.selected + 1 < s.entries.len();

    let mut shell = glass_shell(TOP_PAD_X, TOP_PAD_TOP, card_w, card_h, card_r, ambient);

    // 1. Click-anywhere Capsule Header Background (clicking the capsule expands/collapses, just like the demo!).
    shell = shell.child(
        el().key("capsule-toggle")
            .absolute(0., 0.)
            .size(card_w, COMPACT_CARD_H)
            .hit()
            .cursor(rune::Cursor::Pointer)
            .help(if s.expanded {
                "Click capsule to collapse"
            } else {
                "Click capsule to expand"
            })
            .on_press_tx(toggle),
    );

    // Subtle top-rim window-drag grip pill so dragging the top edge moves/snaps the window natively
    // without blocking click-to-expand across the capsule body.
    shell = shell.child(
        el().key("notch-header")
            .absolute((card_w - 48.) / 2., 0.)
            .size(48., 11.)
            .hit()
            .window_drag()
            .cursor(rune::Cursor::Grab)
            .help("Drag to snap to left, right, or top screen edge")
            .center()
            .child(el().size(24., 3.).radius(1.5).fill(c(0xffffff).alpha(0.22))),
    );

    // 2. Left 3-Up Landscape Quick Strip + Monospace `+N` Counter (1:1 match to 02 · Liquid Glass demo).
    let mut quick = el()
        .key("compact-images")
        .absolute(12., 10.)
        .size(148., 30.);
    if s.entries.is_empty() {
        for slot in 0..3 {
            let mut ghost = el()
                .absolute(slot as f32 * 38., 1.)
                .size(32., 28.)
                .radius(8.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(if slot == 0 { 0.09 } else { 0.04 }))
                .border(0.8, c(0xffffff).alpha(if slot == 0 { 0.20 } else { 0.10 }))
                .center();
            if slot == 0 {
                ghost = ghost.child(rune::icon(&EMPTY, 14.).color(c(0xffffff).alpha(0.50)));
            }
            quick = quick.child(ghost);
        }
        quick = quick.child(
            mono_label(
                if s.paused { "Paused" } else { "Empty" },
                10.5,
                500.,
                c(if s.paused { 0xfbbf24 } else { 0xffffff }).alpha(0.62),
            )
            .absolute(118., 7.5),
        );
    } else {
        let count = s.entries.len().min(3);
        let start = s
            .selected
            .saturating_sub(2)
            .min(s.entries.len().saturating_sub(count));
        for slot in 0..count {
            let i = start + slot;
            let e = &s.entries[i];
            let active = i == s.selected;
            let (tw, th, ty) = if active {
                (34., 30., 0.)
            } else {
                (32., 28., 1.)
            };
            let tx_pos = slot as f32 * 38. - if active { 1. } else { 0. };
            let mut mini = media_card(
                e,
                [tx_pos, ty, tw, th],
                8.,
                (
                    if active { 1.8 } else { 0.8 },
                    if active {
                        c(0xffffff)
                    } else {
                        c(0xffffff).alpha(0.20)
                    },
                ),
            )
            .key(format!("compact-image-{i}"))
            .opacity(if active { 1. } else { 0.56 })
            .hit()
            .cursor(rune::Cursor::Grab)
            .on_press(move |s: &mut State| {
                ripple(s);
                s.selected = i;
                s.scroll = i as f32;
            })
            .drag_out(move |s: &State| s.drag(i))
            .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}"));
            if active {
                mini = mini.shadow(2., 6., c(0x000000).alpha(0.34));
            }
            quick = quick.child(mini);
        }
        let extra_txt = if s.paused {
            "Paused".into()
        } else if s.entries.len() > 3 {
            format!("+{}", s.entries.len() - 3)
        } else {
            format!("{}/{}", s.selected + 1, s.entries.len())
        };
        let extra_x = count as f32 * 38. + 4.;
        quick = quick.child(
            mono_label(
                extra_txt,
                11.,
                500.,
                if s.paused {
                    c(0xfbbf24)
                } else {
                    c(0xffffff).alpha(0.66)
                },
            )
            .absolute(extra_x, 7.5),
        );
    }
    shell = shell.child(quick);

    // 3. Right Capsule Controls: crisp white `Copy` button + square frosted `+`/`−` button (exact match to demo!).
    shell = shell
        .child(copy_button(s, card_w - 98., 11., 52., 28., false))
        .child(
            el().key("expand")
                .absolute(card_w - 40., 11.)
                .size(28., 28.)
                .radius(11.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.12))
                .border(0.8, c(0xffffff).alpha(0.16))
                .hit()
                .label(if s.expanded {
                    "Collapse shelf"
                } else {
                    "Expand shelf"
                })
                .help(if s.expanded {
                    "Collapse shelf"
                } else {
                    "Expand shelf"
                })
                .cursor(rune::Cursor::Pointer)
                .hover(c(0xffffff).alpha(0.20))
                .press(c(0xffffff).alpha(0.08))
                .press_scale(0.94)
                .on_press_tx(toggle)
                .center()
                .child(rune::icon(if s.expanded { &MINUS } else { &PLUS }, 14.).color(c(0xf8fafc))),
        );

    // 4. Expanded Split Glass Studio (2-column Loupe + Inspector with 3x2 Mini-Pager & ‹ Prev / Next ›).
    if s.expanded {
        let mut studio = el()
            .key("expanded-studio")
            .absolute(0., COMPACT_CARD_H)
            .size(EXPANDED_CARD_W, EXPANDED_CARD_H - COMPACT_CARD_H)
            .transition(Transition::blur_replace().combined(Transition::offset(0., -10.)))
            // Specular horizontal divider (`from-transparent via-white/20 to-transparent`).
            .child(
                el().absolute(12., 0.)
                    .size(EXPANDED_CARD_W - 24., 0.8)
                    .linear_gradient(
                        std::f32::consts::FRAC_PI_2,
                        [
                            (0., c(0xffffff).alpha(0.0)),
                            (0.5, c(0xffffff).alpha(0.22)),
                            (1., c(0xffffff).alpha(0.0)),
                        ],
                    ),
            );

        // Left Column: Full-bleed rounded-2xl Loupe Stage + top-left `1 of 6` frosted pill.
        if let Some(e) = s.entries.get(s.selected) {
            let badge_txt = format!("{} of {}", s.selected + 1, s.entries.len());
            studio = studio.child(
                media_card(
                    e,
                    [12., 11., 244., 198.],
                    16.,
                    (1., c(0xffffff).alpha(0.20)),
                )
                .key("preview")
                .shadow(6., 20., c(0x000000).alpha(0.45))
                .hit()
                .cursor(rune::Cursor::Grab)
                .drag_out(|s: &State| s.drag(s.selected))
                .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}"))
                .child(
                    el().absolute(8., 8.)
                        .size(52., 20.)
                        .radius(10.)
                        .continuous_corners()
                        .backdrop_blur(8.)
                        .fill(c(0x000000).alpha(0.56))
                        .border(0.7, c(0xffffff).alpha(0.18))
                        .center()
                        .child(mono_label(badge_txt, 9.5, 500., c(0xffffff).alpha(0.90))),
                ),
            );
        } else {
            studio = studio.child(
                el().key("preview")
                    .absolute(12., 11.)
                    .size(244., 198.)
                    .radius(16.)
                    .continuous_corners()
                    .fill(c(0x000000).alpha(0.30))
                    .border(1., c(0xffffff).alpha(0.14))
                    .center()
                    .child(sans_label(
                        "Copy any image (⌘C) to capture",
                        11.,
                        500.,
                        c(0xffffff).alpha(0.56),
                    )),
            );
        }

        // Right Column: Recessed Inspector Card (`126 x 198 pt`, exact match to demo).
        let mut inspector = el()
            .key("inspector-card")
            .absolute(266., 11.)
            .size(126., 198.)
            .radius(16.)
            .continuous_corners()
            .fill(c(0x000000).alpha(0.26))
            .border(0.8, c(0xffffff).alpha(0.13));

        if let Some(e) = s.entries.get(s.selected) {
            let sub = if e.has_alpha {
                format!("{} · Alpha", e.size_badge())
            } else {
                format!("{} · PNG", e.size_badge())
            };
            inspector = inspector
                .child(sans_label(e.title(s.selected), 11.5, 600., c(0xffffff)).absolute(10., 10.))
                .child(
                    mono_label(
                        format!("{} × {}", e.size.0, e.size.1),
                        10.,
                        500.,
                        c(0xffffff).alpha(0.62),
                    )
                    .absolute(10., 28.),
                )
                .child(sans_label(sub, 9.5, 400., c(0xffffff).alpha(0.45)).absolute(10., 44.));
        } else {
            inspector = inspector
                .child(
                    sans_label("No Captures", 11.5, 600., c(0xffffff).alpha(0.75))
                        .absolute(10., 10.),
                )
                .child(
                    sans_label("Clipboard active", 9.5, 400., c(0xffffff).alpha(0.45))
                        .absolute(10., 28.),
                );
        }

        // Middle of Inspector: 3x2 Mini-Pager Grid (`grid-cols-3` from 02 · Liquid Glass demo!).
        let mut grid = el()
            .key("thumbnail-strip")
            .absolute(10., 74.)
            .size(106., 60.);
        if s.entries.is_empty() {
            for cell in 0..6 {
                let col = cell % 3;
                let row = cell / 3;
                grid = grid.child(
                    el().absolute(col as f32 * 36.5, row as f32 * 31.)
                        .size(33., 26.)
                        .radius(6.)
                        .continuous_corners()
                        .fill(c(0xffffff).alpha(0.05))
                        .border(0.6, c(0xffffff).alpha(0.09)),
                );
            }
        } else {
            let count = s.entries.len().min(6);
            let page_start = s
                .selected
                .saturating_sub(5)
                .min(s.entries.len().saturating_sub(count));
            for cell in 0..count {
                let i = page_start + cell;
                let col = cell % 3;
                let row = cell / 3;
                let e = &s.entries[i];
                let active = i == s.selected;
                let mini = media_card(
                    e,
                    [col as f32 * 36.5, row as f32 * 31., 33., 26.],
                    6.,
                    (
                        if active { 1.6 } else { 0.6 },
                        if active {
                            c(0xffffff)
                        } else {
                            c(0xffffff).alpha(0.14)
                        },
                    ),
                )
                .key(format!("image-{i}"))
                .opacity(if active { 1. } else { 0.48 })
                .hit()
                .cursor(rune::Cursor::Grab)
                .on_press(move |s: &mut State| {
                    ripple(s);
                    s.selected = i;
                    s.scroll = i as f32;
                })
                .drag_out(move |s: &State| s.drag(i))
                .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}"));
                grid = grid.child(mini);
            }
        }
        inspector = inspector.child(grid);

        // Bottom of Inspector: `‹ Prev` and `Next ›` buttons (exact match to 02 · Liquid Glass demo!).
        let pager_btn = |txt: &'static str, x: f32, enabled: bool, tip: &'static str| {
            let mut b = el()
                .absolute(x, 162.)
                .size(51., 25.)
                .radius(8.)
                .continuous_corners()
                .fill(c(0xffffff).alpha(0.10))
                .border(0.7, c(0xffffff).alpha(0.13))
                .hit()
                .label(tip)
                .help(tip)
                .center()
                .child(sans_label(txt, 10.5, 500., c(0xf8fafc)));
            if enabled {
                b = b
                    .cursor(rune::Cursor::Pointer)
                    .hover(c(0xffffff).alpha(0.18))
                    .press(c(0xffffff).alpha(0.08))
                    .press_scale(0.95);
            } else {
                b = b.disabled(true).opacity(0.35);
            }
            b
        };
        inspector = inspector
            .child(
                pager_btn("‹ Prev", 10., has_prev, "Previous image")
                    .key("select-previous")
                    .on_press(|s: &mut State| {
                        s.selected = s.selected.saturating_sub(1);
                        s.scroll = s.selected as f32;
                    }),
            )
            .child(
                pager_btn("Next ›", 65., has_next, "Next image")
                    .key("select-next")
                    .on_press(|s: &mut State| {
                        if s.selected + 1 < s.entries.len() {
                            s.selected += 1;
                            s.scroll = s.selected as f32;
                        }
                    }),
            );

        studio = studio.child(inspector);
        shell = shell.child(studio);
    }

    shell = shell.child(glass_rim(card_w, card_h, card_r));

    el().key("notch")
        .size(win_w, win_h)
        .on_pointer_move(|s: &mut State, p| s.pointer = [p.x, p.y])
        .on_wheel(wheel)
        .on_secondary_press(|s: &mut State, press: rune::PointerPress, tx| {
            s.ripples.clear();
            s.menu_host = true;
            tx.resize_window_instant(shelf_size(s));
            tx.wake_after(Duration::from_millis(16));
            s.menu_at = Some((s.clock + 0.12, press.x.min(56.), 64.));
            tx.wake_after(Duration::from_millis(150));
        })
        .child(shell)
        .child(rune::effect(77).size(win_w, win_h).effect_backdrop(true))
}

fn main() {
    rune::App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        min_size: Some((SIDE_COMPACT_WIN_W, COMPACT_WIN_H)),
        scenes: &[
            ("expanded", |s| s.expanded = true),
            ("left", |s| s.dock.edge = docking::Edge::Left),
            ("right", |s| s.dock.edge = docking::Edge::Right),
            ("left-expanded", |s| {
                s.dock.edge = docking::Edge::Left;
                s.expanded = true;
            }),
            ("right-expanded", |s| {
                s.dock.edge = docking::Edge::Right;
                s.expanded = true;
            }),
            ("water", |s| {
                s.expanded = true;
                s.pointer = [TOP_PAD_X + 28., TOP_PAD_TOP + 24.];
                ripple(s);
            }),
        ],
        ..AppSpec::new(
            "Image Shelf",
            (COMPACT_WIN_W, COMPACT_WIN_H),
            State::new,
            view,
        )
    })
    .window_options(rune::WindowOptions {
        transparent: true,
        borderless: true,
        always_on_top: true,
        resizable: false,
        min_size: Some((SIDE_COMPACT_WIN_W, COMPACT_WIN_H)),
        ..Default::default()
    })
    .macos_window(
        rune::MacWindowOptions::default()
            .level(25)
            .shadow(false)
            .nonactivating_panel(true),
    )
    .on_ready(|window| {
        docking::register(window);
        let mut d = docking::Dock::default();
        docking::place(&mut d, docking::Edge::Top);
        let _ = window;
    })
    .compositor(water::Water::default())
    .run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_capsule_body_expands_and_collapses_with_spring() {
        let mut s = State::new();
        s.demo = true;
        s.fixtures();
        let mut rt = rune::Runtime::new(
            s,
            view,
            (EXPANDED_WIN_W, EXPANDED_WIN_H),
            rune::Env::new(true, 1.),
        );
        rt.frame(0.);

        // Click open capsule space (center-right of header, away from thumbnails/Copy/+):
        // must toggle expand just like clicking the island in the demo!
        let body_x = TOP_PAD_X + 185.;
        let body_y = TOP_PAD_TOP + 28.;
        rt.pointer_move(body_x, body_y);
        rt.pointer_down(body_x, body_y);
        rt.pointer_up(body_x, body_y);
        rt.frame(1. / 60.);
        assert!(rt.state.expanded, "clicking capsule header expands shelf");
        let mid_open = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("liquid-shell"))
            .unwrap()
            .rect;
        assert!(
            mid_open[2] > COMPACT_CARD_W && mid_open[2] < EXPANDED_CARD_W,
            "shell width springs smoothly on expand: {mid_open:?}"
        );
        assert!(
            mid_open[3] > COMPACT_CARD_H && mid_open[3] < EXPANDED_CARD_H,
            "shell height springs smoothly on expand: {mid_open:?}"
        );

        for _ in 0..40 {
            rt.frame(1. / 60.);
        }

        // Click capsule header again to collapse smoothly.
        rt.pointer_move(body_x, body_y);
        rt.pointer_down(body_x, body_y);
        rt.pointer_up(body_x, body_y);
        rt.frame(1. / 60.);
        assert!(
            !rt.state.expanded,
            "clicking capsule header again collapses shelf"
        );
        let mid_close = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("liquid-shell"))
            .unwrap()
            .rect;
        assert!(
            mid_close[2] > COMPACT_CARD_W && mid_close[2] < EXPANDED_CARD_W,
            "shell width springs smoothly on collapse: {mid_close:?}"
        );
    }

    #[test]
    fn water_is_thumbnail_only_in_every_layout() {
        for edge in [
            docking::Edge::Top,
            docking::Edge::Left,
            docking::Edge::Right,
        ] {
            for expanded in [false, true] {
                let mut s = State::new();
                s.demo = true;
                s.fixtures();
                s.dock.edge = edge;
                s.expanded = expanded;
                let size = shelf_size(&s);
                let mut rt = rune::Runtime::new(s, view, size, rune::Env::new(true, 1.));
                rt.frame(0.);
                let side = edge != docking::Edge::Top;
                let rail = side_rail_x(&rt.state);

                // 1. Clicking a thumbnail triggers water refraction.
                let (x, y) = if side {
                    (rail + 32., SIDE_MARGIN_Y + 58.)
                } else {
                    (TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                };
                rt.pointer_move(x, y);
                rt.pointer_down(x, y);
                rt.pointer_up(x, y);
                rt.frame(0.016);
                assert_eq!(rt.state.ripples.len(), 1, "thumbnail {edge:?}/{expanded}");
                let area = rt.state.ripples[0].2;
                assert!(
                    x >= area[0] && x < area[0] + area[2] && y >= area[1] && y < area[1] + area[3]
                );
                rt.state.ripples.clear();

                // 2. Clicking Copy / navigation controls never triggers water.
                let card_w = if expanded {
                    EXPANDED_CARD_W
                } else {
                    COMPACT_CARD_W
                };
                let (x, y) = if side {
                    (rail + 36., SIDE_MARGIN_Y + 268.)
                } else {
                    (TOP_PAD_X + card_w - 72., TOP_PAD_TOP + 24.)
                };
                rt.pointer_move(x, y);
                rt.pointer_down(x, y);
                rt.pointer_up(x, y);
                rt.frame(0.016);
                assert!(rt.state.ripples.is_empty(), "controls {edge:?}/{expanded}");

                // 3. Clicking the expanded preview loupe never triggers water.
                if expanded {
                    let (x, y) = if side {
                        (
                            if edge == docking::Edge::Right {
                                SIDE_MARGIN_INWARD + 120.
                            } else {
                                SIDE_MARGIN_EDGE + SIDE_RAIL_W + 120.
                            },
                            140.,
                        )
                    } else {
                        (TOP_PAD_X + 120., TOP_PAD_TOP + 140.)
                    };
                    rt.pointer_move(x, y);
                    rt.pointer_down(x, y);
                    rt.pointer_up(x, y);
                    rt.frame(0.016);
                    assert!(rt.state.ripples.is_empty(), "preview {edge:?}");
                }

                // 4. Drag grip never triggers water.
                rt.state.pointer = if side {
                    [rail + 34., SIDE_MARGIN_Y + 18.]
                } else {
                    [TOP_PAD_X + card_w / 2., TOP_PAD_TOP + 5.]
                };
                ripple(&mut rt.state);
                assert!(rt.state.ripples.is_empty(), "header {edge:?}");
            }
        }
    }

    #[test]
    fn demo_drag_has_png_and_never_fake_file() {
        let mut s = State::new();
        s.demo = true;
        s.fixtures();
        assert!(s.entries[2].has_alpha, "orb icon fixture must carry alpha");
        let mut rt = rune::Runtime::new(
            s,
            view,
            (EXPANDED_WIN_W, EXPANDED_WIN_H),
            rune::Env::new(true, 1.),
        );
        rt.state.expanded = true;
        rt.frame(0.);

        // Top-rim grip starts native window drag.
        let grip_x = TOP_PAD_X + EXPANDED_CARD_W / 2.;
        let grip_y = TOP_PAD_TOP + 5.;
        rt.pointer_move(grip_x, grip_y);
        rt.pointer_down(grip_x, grip_y);
        assert!(
            rt.take_titlebar_action().is_some(),
            "top grip must start a native window drag"
        );
        rt.pointer_up(grip_x, grip_y);
        rt.frame(0.016);

        rt.wheel_xy(TOP_PAD_X + 300., TOP_PAD_TOP + 150., 0., -48.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 1);

        // Inspector `Next ›` button advances selection without triggering water.
        let next_btn_x = TOP_PAD_X + 266. + 90.;
        let next_btn_y = TOP_PAD_TOP + COMPACT_CARD_H + 11. + 174.;
        rt.pointer_move(next_btn_x, next_btn_y);
        rt.pointer_down(next_btn_x, next_btn_y);
        rt.pointer_up(next_btn_x, next_btn_y);
        rt.frame(1. / 60.);
        assert_eq!(
            rt.state.selected, 2,
            "inspector Next button advances selection"
        );
        assert!(rt.state.ripples.is_empty());

        rt.state.clock += 0.3;
        rt.wheel_xy(TOP_PAD_X + 300., TOP_PAD_TOP + 150., -99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 3, "large delta moves only one image");
        rt.state.clock += 0.3;
        rt.wheel_xy(TOP_PAD_X + 300., TOP_PAD_TOP + 150., 99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 2);

        rt.state.pointer = [TOP_PAD_X + 28., TOP_PAD_TOP + 24.];
        for _ in 0..10 {
            ripple(&mut rt.state);
        }
        assert_eq!(rt.state.ripples.len(), 5, "bounded transient effects");

        rt.state.clock += 0.3;
        rt.wheel_event(
            rune::WheelEvent::new(0., -3.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true)
                .phase(rune::GesturePhase::Began),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 2, "small trackpad noise is ignored");
        rt.wheel_event(
            rune::WheelEvent::new(0., -20.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3);
        rt.wheel_event(
            rune::WheelEvent::new(0., -400.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3, "same swipe cannot skip images");
        rt.wheel_event(
            rune::WheelEvent::new(0., 0.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true)
                .phase(rune::GesturePhase::Ended),
        );
        rt.frame(0.016);
        rt.wheel_event(
            rune::WheelEvent::new(0., -999.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true)
                .phase(rune::GesturePhase::Began),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3, "inertia must not change selection");
        rt.wheel_event(
            rune::WheelEvent::new(0., 0.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true)
                .phase(rune::GesturePhase::Ended),
        );
        rt.frame(0.3);
        rt.state.clock += 0.3;
        rt.wheel_event(
            rune::WheelEvent::new(0., 12.)
                .at(TOP_PAD_X + 28., TOP_PAD_TOP + 24.)
                .precise(true)
                .phase(rune::GesturePhase::Began),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 2, "next reverse swipe moves one image");

        let mut s = rt.state;
        let item = s.drag(0).unwrap();
        assert!(item.png_data().is_some());
        assert!(item.paths().is_empty());
        assert!(!item.allows_move());
        s.demo = false;
        assert_eq!(s.drag(0).unwrap().paths().len(), 1);
        s.paused = true;
        let before = s.entries.len();
        s.capture(&ClipboardContent::default());
        assert_eq!(before, s.entries.len());
        assert!(s.drag(999).is_none());
    }
}

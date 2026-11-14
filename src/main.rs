mod docking;
mod store;
mod water;
use rune::{
    AppSpec, ClipboardContent, ClipboardImage, Cx, DragItem, El, Image, ImageCache, TextStyle, Tx,
    Waker, el, text,
};
use std::{
    path::{Path, PathBuf},
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
}
impl Entry {
    fn bytes(&self) -> Option<Arc<[u8]>> {
        self.png
            .clone()
            .or_else(|| std::fs::read(&self.path).ok().map(Into::into))
    }
}
// Largest preview is 234x280pt at 2x; fit both previews without decoding more pixels.
const THUMB_PX: u32 = 560;
type Thumb = (u32, u32, Vec<u8>);
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
fn thumbnail(img: &image::DynamicImage) -> Thumb {
    let t = img.thumbnail(THUMB_PX, THUMB_PX).into_rgba8();
    (t.width(), t.height(), t.into_raw())
}
// Dimensions from the PNG header alone, without reading or decoding the file.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    ClipboardImage::from_png(head.to_vec()).map(|c| c.size())
}
struct State {
    dock: docking::Dock,
    entries: Vec<Entry>,
    shell: [Image; 2],
    side_shell: [Image; 4],
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
            shell: notch_surfaces(),
            side_shell: side_surfaces(),
            pointer: [110., 26.],
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
            // Only headers are read here; setup() queues thumbnails off the main thread.
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
        match store::save(&self.dir, image.png()) {
            Ok((path, decoded)) => {
                if self.entries.iter().any(|e| e.path == path) {
                    return;
                }
                let thumb = match &self.thumbs {
                    Some(t) => {
                        let _ = t.jobs.send((path.clone(), decoded));
                        None
                    }
                    None => decoded.map(|d| {
                        let (w, h, px) = thumbnail(&d);
                        self.cache.rgba(w, h, px)
                    }),
                };
                self.entries.insert(
                    0,
                    Entry {
                        path,
                        png: None,
                        image: thumb,
                        size: image.size(),
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
        for n in 0..6u8 {
            let img = image::RgbaImage::from_fn(640, 400, |x, y| {
                let glow = (((x as f32 / 640.0 * std::f32::consts::PI).sin()
                    * (y as f32 / 400.0 * std::f32::consts::PI).sin())
                    * 100.0) as u8;
                image::Rgba([20 + n * 18, 35 + glow, 80 + glow / 2, 255])
            });
            let mut out = std::io::Cursor::new(vec![]);
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            let (w, h, px) = thumbnail(&img.into());
            self.entries.push(Entry {
                path: PathBuf::from(format!("demo-{n}.png")),
                png: Some(out.into_inner().into()),
                image: Some(self.cache.rgba(w, h, px)),
                size: (640, 400),
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
        while let Ok((path, (w, h, px))) = thumbs.done.try_recv() {
            if let Some(e) = s.entries.iter_mut().find(|e| e.path == path) {
                e.image = Some(s.cache.rgba(w, h, px));
                loaded = true;
            }
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
        tx.resize_window(shelf_size(s));
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
fn c(n: u32) -> rune::Color {
    rune::color::hex(&format!("#{n:06x}")).unwrap()
}
fn label(v: impl AsRef<str>, size: f32, color: u32) -> El {
    text(v.as_ref())
        .font(TextStyle::new(size, 400.))
        .color(c(color))
}
// One stroked icon set so every button glyph shares weight, size and optical centre.
// Text glyphs (↑ ‹ + −) come from different fallback fonts and never match.
macro_rules! glyph {
    ($name:ident, $id:literal, $d:literal) => {
        static $name: rune::Icon = rune::Icon {
            name: $id,
            view: 16.,
            body: concat!(
                r#"<path d=""#,
                $d,
                r#"" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#
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
fn button(icon: &'static rune::Icon, x: f32, y: f32, w: f32) -> El {
    el().absolute(x, y)
        .size(w, 24.)
        .radius(8.)
        .gradient(c(0x24262d), c(0x13151a))
        .border(0.5, c(0x373a44))
        .hit()
        .cursor(rune::Cursor::Pointer)
        .label(match icon.name {
            "shelf-up" | "shelf-left" => "Previous image",
            "shelf-down" | "shelf-right" => "Next image",
            "shelf-plus" => "Expand",
            _ => "Collapse",
        })
        .center()
        .child(rune::icon(icon, 14.).color(c(0xe3e9f2)))
}
fn picture(e: &Entry, x: f32, y: f32, w: f32, h: f32) -> El {
    let ratio = (w / e.size.0 as f32).min(h / e.size.1 as f32);
    let (pw, ph) = (e.size.0 as f32 * ratio, e.size.1 as f32 * ratio);
    let frame = el()
        .absolute(x, y)
        .size(w, h)
        .gradient(c(0x191b22), c(0x0e0f13))
        .radius(8.)
        .clip_rounded(8.);
    match &e.image {
        Some(image) => frame.child(
            el().absolute((w - pw) / 2., (h - ph) / 2.)
                .size(pw, ph)
                .pixels(image.pixels()),
        ),
        None => frame,
    }
}
// Cells a strip can show, plus one neighbour each side so a one-step move still slides in.
fn visible(offset: f32, pitch: f32, span: f32, len: usize) -> std::ops::Range<usize> {
    let first = ((offset / pitch).floor() as isize - 1).max(0) as usize;
    let last = (((offset + span) / pitch).ceil().max(0.) as usize + 1).min(len);
    first.min(last)..last
}
const ISLAND_W: f32 = 324.;
const BODY_W: f32 = 300.;
const SHOULDER: f32 = 12.;
const COMPACT_H: f32 = 52.;
const EXPANDED_H: f32 = 230.;
// Both horizontal trackpad motion and a conventional vertical wheel browse history.
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
    // One deliberate gesture moves exactly one image. Never replay momentum or
    // carry fractional deltas into the next gesture/selection.
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
// Coordinates are in window points. Only the thumbnail viewport may refract;
// previews, buttons, headers and transparent window hosting space are excluded.
fn thumbnail_area(s: &State) -> Option<[f32; 4]> {
    if s.entries.is_empty() {
        return None;
    }
    let side = matches!(s.dock.edge, docking::Edge::Left | docking::Edge::Right);
    let p = [
        s.pointer[0] + if side { 0. } else { SHOULDER },
        s.pointer[1],
    ];
    let areas = if side {
        let rail = if s.dock.edge == docking::Edge::Right {
            shelf_size(s).0 - 76.
        } else {
            0.
        };
        vec![[rail + 10., 34., 56., 280.]]
    } else {
        let mut areas = vec![[SHOULDER + 10., 10., 36., 32.]];
        if s.expanded {
            areas.push([SHOULDER + 10., 60., 280., 44.]);
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
        tx.resize_window(shelf_size(s));
        tx.wake_after(Duration::from_millis(16));
    }
    if s.menu_restore_compact {
        s.menu_restore_compact = false;
        s.expanded = false;
        tx.resize_window(shelf_size(s));
        tx.wake_after(Duration::from_millis(16));
    }
    match picked {
        Some("copy") => {
            if let Some(e) = s.entries.get(s.selected)
                && let Some(img) = e.bytes().and_then(ClipboardImage::from_png)
            {
                tx.copy_rich(ClipboardContent::default().with_image(img));
                s.status = "Copied image".into();
            }
        }
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
        (if s.expanded { 330. } else { 76. }, 360.)
    } else {
        (
            ISLAND_W,
            if s.menu_host {
                400.
            } else if s.expanded {
                EXPANDED_H
            } else {
                COMPACT_H
            },
        )
    }
}
fn set_edge(s: &mut State, edge: docking::Edge, tx: &mut Tx) {
    s.ripples.clear();
    docking::place(&mut s.dock, edge);
    tx.resize_window(shelf_size(s));
    tx.wake_after(Duration::from_millis(16));
}
fn side_view(s: &State) -> El {
    let (w, h) = shelf_size(s);
    let right = s.dock.edge == docking::Edge::Right;
    let rail = if right { w - 76. } else { 0. };
    let mut root = el()
        .key("side-dock")
        .size(w, h)
        .on_pointer_move(|s: &mut State, p| s.pointer = [p.x, p.y])
        .on_wheel(wheel)
        .on_secondary_press(|s: &mut State, press: rune::PointerPress, tx| {
            s.ripples.clear();
            // AppKit clips a panel-attached menu to the narrow host on macOS 26.
            // Temporarily open the inward preview so the menu has readable width;
            // menu_action restores the compact rail on selection or cancellation.
            if !s.expanded {
                s.menu_restore_compact = true;
                s.expanded = true;
                tx.resize_window(shelf_size(s));
                tx.wake_after(Duration::from_millis(16));
            }
            let x = if s.dock.edge == docking::Edge::Left {
                86.
            } else {
                10.
            };
            s.menu_at = Some((s.clock + 0.12, x, press.y.min(80.)));
            tx.wake_after(Duration::from_millis(150));
        })
        .child(
            el().key("notch-header")
                .absolute(rail + 8., 12.)
                .size(60., 26.)
                .hit()
                .window_drag()
                .cursor(rune::Cursor::Grab)
                .center()
                .child(el().size(22., 3.).radius(1.5).fill(c(0x777d89))),
        )
        .child(
            button(&UP, rail + 6., 320., 20.)
                .key("select-previous")
                .on_press(|s: &mut State| {
                    s.selected = s.selected.saturating_sub(1);
                    s.scroll = s.selected as f32;
                }),
        )
        .child(
            button(&DOWN, rail + 28., 320., 20.)
                .key("select-next")
                .on_press(|s: &mut State| {
                    if s.selected + 1 < s.entries.len() {
                        s.selected += 1;
                        s.scroll = s.selected as f32;
                    }
                }),
        )
        .child(
            button(
                if s.expanded { &MINUS } else { &PLUS },
                rail + 50.,
                320.,
                20.,
            )
            .key("expand")
            .on_press_tx(toggle),
        );
    let mut strip = el()
        .key("thumbnail-strip")
        .absolute(rail + 10., 34.)
        .size(56., 280.)
        .clip_rounded(0.);
    let offset = (s.scroll * 66.).min((s.entries.len() as f32 * 66. - 280.).max(0.));
    for i in visible(offset, 66., 280., s.entries.len()) {
        let e = &s.entries[i];
        let y = i as f32 * 66. - offset;
        strip = strip.child(
            picture(e, 0., y, 56., 56.)
                .key(format!("image-{i}"))
                .opacity(if i == s.selected { 1. } else { 0.62 })
                .hit()
                .cursor(rune::Cursor::Grab)
                .on_press(move |s: &mut State| {
                    ripple(s);
                    s.selected = i;
                    s.scroll = i as f32;
                })
                .drag_out(move |s: &State| s.drag(i))
                .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
        );
        if i == s.selected {
            strip = strip.child(
                el().key("selection-marker")
                    .absolute(20., y + 59.)
                    .size(16., 2.)
                    .radius(1.)
                    .fill(c(0x9ee8d9)),
            );
        }
    }
    root = root.child(strip);
    if s.expanded
        && let Some(e) = s.entries.get(s.selected)
    {
        root = root.child(
            picture(e, if right { 10. } else { 86. }, 34., 234., 280.)
                .key("preview")
                .hit()
                .cursor(rune::Cursor::Grab)
                .drag_out(|s: &State| s.drag(s.selected))
                .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
        );
    }
    el().w_full()
        .h_full()
        .child(
            el().absolute(0., 0.)
                .size(w, h)
                .pixels(s.side_shell[usize::from(right) * 2 + usize::from(s.expanded)].pixels()),
        )
        .child(root)
        .child(rune::effect(77).size(w, h).effect_backdrop(true))
}
fn side_surfaces() -> [Image; 4] {
    let cache = ImageCache::new(Arc::new(|| {}));
    std::array::from_fn(|i| {
        let w = if i % 2 == 0 { 76. } else { 330. };
        let r = if i % 2 == 0 { 20. } else { 24. };
        // Flush against the screen edge with square corners there; only the inward corners
        // round. Concave shoulders read as stray tabs against wallpaper, unlike the menu bar.
        let path = format!(
            "M0 12 H{} Q{w} 12 {w} {} V{} Q{w} 348 {} 348 H0 Z",
            w - r,
            12. + r,
            348. - r,
            w - r
        );
        let transform = if i >= 2 {
            format!("translate({w} 0) scale(-1 1)")
        } else {
            String::new()
        };
        let svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="360"><defs><linearGradient id="g"><stop stop-color="#090a0d"/><stop offset="1" stop-color="#111319"/></linearGradient></defs><path transform="{transform}" d="{path}" fill="url(#g)" fill-opacity="0.62"/></svg>"##
        );
        // svg() fits the drawing into a size_pt square, so pass the longest side (the 360pt
        // height). Passing the width rasterized the rail at 32px wide and stretched it ~5x,
        // which smeared its edges into a pale band.
        let image = cache.svg(svg.into_bytes(), 360., 2.);
        let _ = image.wait();
        image
    })
}
fn notch_surfaces() -> [Image; 2] {
    let cache = ImageCache::new(Arc::new(|| {}));
    [COMPACT_H, EXPANDED_H].map(|h| {
        let r = if h == COMPACT_H { 20. } else { 24. };
        // Concave shoulders flow from the screen edge into the vertical sides.
        // Transparent cutouts are part of the window, never painted with a wallpaper color.
        let path = format!("M0 0 H324 C317.373 0 312 5.373 312 12 V{} Q312 {} {} {} H{} Q12 {} 12 {} V12 C12 5.373 6.627 0 0 0 Z", h-r, h, 312.-r, h, 12.+r, h, h-r);
        let svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="324" height="{h}" viewBox="0 0 324 {h}"><defs><linearGradient id="g" x2="0" y2="1"><stop stop-color="#090a0d"/><stop offset="1" stop-color="#111319"/></linearGradient></defs><path d="{path}" fill="url(#g)" fill-opacity="0.62"/></svg>"##);
        let image = cache.svg(svg.into_bytes(), ISLAND_W, 2.);
        let _ = image.wait();
        image
    })
}
fn view(s: &State, _: &mut Cx<State>) -> El {
    if matches!(s.dock.edge, docking::Edge::Left | docking::Edge::Right) {
        return side_view(s);
    }
    let mut root = el()
        .key("notch")
        .on_pointer_move(|s: &mut State, p| s.pointer = [p.x - SHOULDER, p.y])
        .on_wheel(wheel)
        .absolute(SHOULDER, 0.)
        .size(BODY_W, if s.expanded { EXPANDED_H } else { COMPACT_H })
        .on_secondary_press(|s: &mut State, press: rune::PointerPress, tx| {
            s.ripples.clear();
            // Reserve transparent space below the island before showing AppKit's
            // view-attached menu; otherwise a 52pt panel clips all menu rows.
            s.menu_host = true;
            tx.resize_window(shelf_size(s));
            tx.wake_after(Duration::from_millis(16));
            s.menu_at = Some((s.clock + 0.12, press.x.min(40.), 54.));
            tx.wake_after(Duration::from_millis(150));
        })
        .child(
            el().absolute(56., 10.)
                .size(144., 32.)
                .key("notch-header")
                .cursor(rune::Cursor::Grab)
                .hit()
                .window_drag()
                .center()
                .child(label(
                    if s.paused {
                        "Paused".into()
                    } else {
                        format!(
                            "{} / {}",
                            if s.entries.is_empty() {
                                0
                            } else {
                                s.selected + 1
                            },
                            s.entries.len()
                        )
                    },
                    11.,
                    0xb7bbc6,
                )),
        )
        .child(
            button(&LEFT, 206., 14., 24.)
                .key("select-previous")
                .on_press(|s: &mut State| {
                    s.selected = s.selected.saturating_sub(1);
                    s.scroll = s.selected as f32;
                }),
        )
        .child(
            button(&RIGHT, 234., 14., 24.)
                .key("select-next")
                .on_press(|s: &mut State| {
                    if s.selected + 1 < s.entries.len() {
                        s.selected += 1;
                        s.scroll = s.selected as f32;
                    }
                }),
        )
        .child(
            button(if s.expanded { &MINUS } else { &PLUS }, 262., 14., 24.)
                .key("expand")
                .on_press_tx(toggle),
        );
    // Restrained edge lighting and a live capture indicator, without extra chrome.
    root = root.child(
        el().absolute(51., 24.)
            .size(3., 3.)
            .radius(1.5)
            .fill(c(if s.paused { 0xe3ac68 } else { 0x8ce6d4 })),
    );
    if s.expanded {
        root = root.child(el().absolute(10., 51.).size(280., 0.5).linear_gradient(
            0.,
            [(0., c(0x15171d)), (0.5, c(0x353b48)), (1., c(0x15171d))],
        ));
    }
    if !s.entries.is_empty() {
        let mut compact = el()
            .absolute(10., 10.)
            .size(36., 32.)
            .clip_rounded(8.)
            .key("compact-images");
        for i in visible(s.selected as f32 * 42., 42., 36., s.entries.len()) {
            let e = &s.entries[i];
            compact = compact.child(
                picture(e, (i as f32 - s.selected as f32) * 42., 0., 36., 32.)
                    .key(format!("compact-image-{i}"))
                    .on_press(ripple)
                    .hit()
                    .cursor(rune::Cursor::Grab)
                    .drag_out(move |s: &State| s.drag(i))
                    .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
            );
        }
        root = root.child(compact);
    } else {
        root = root.child(label("▧", 20., 0x777777).absolute(17., 13.));
    }
    if s.expanded {
        let offset = (s.scroll * 69.).min((s.entries.len() as f32 * 69. - 287.).max(0.));
        let mut strip = el()
            .absolute(10., 60.)
            .size(280., 49.)
            .clip_rounded(0.)
            .key("thumbnail-strip");
        for i in visible(offset, 69., 280., s.entries.len()) {
            let e = &s.entries[i];
            strip = strip.child(
                picture(e, i as f32 * 69. - offset, 0., 62., 44.)
                    .key(format!("image-{i}"))
                    .hit()
                    .cursor(rune::Cursor::Grab)
                    .opacity(if i == s.selected { 1. } else { 0.65 })
                    .on_press(move |s: &mut State| {
                        ripple(s);
                        s.selected = i;
                        s.scroll = i as f32;
                    })
                    .drag_out(move |s: &State| s.drag(i))
                    .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
            );
        }
        if !s.entries.is_empty() {
            strip = strip.child(
                el().absolute(s.selected as f32 * 69. - offset + 22., 47.)
                    .size(18., 2.)
                    .radius(1.)
                    .fill(c(0x9ee8d9))
                    .key("selection-marker"),
            );
        }
        root = root.child(strip);
        if let Some(e) = s.entries.get(s.selected) {
            root = root.child(
                picture(e, 10., 112., 280., 110.)
                    .key("preview")
                    .border(0.6, c(0x353944))
                    .hit()
                    .cursor(rune::Cursor::Grab)
                    .drag_out(|s: &State| s.drag(s.selected))
                    .on_drag_end(|s: &mut State, o, _| s.status = format!("Drag: {o:?}")),
            );
        }
    }
    el().w_full()
        .h_full()
        .child(
            el().absolute(0., 0.)
                .size(ISLAND_W, if s.expanded { EXPANDED_H } else { COMPACT_H })
                .pixels(s.shell[usize::from(s.expanded)].pixels()),
        )
        .child(root)
        .child(
            rune::effect(77)
                .absolute(0., 0.)
                .size(ISLAND_W, if s.expanded { EXPANDED_H } else { COMPACT_H })
                .effect_backdrop(true),
        )
}
fn main() {
    rune::App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        min_size: Some((76., COMPACT_H)),
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
                s.pointer = [40., 80.];
                ripple(s);
            }),
        ],
        ..AppSpec::new("Image Shelf", (ISLAND_W, COMPACT_H), State::new, view)
    })
    .window_options(rune::WindowOptions {
        transparent: true,
        borderless: true,
        always_on_top: true,
        resizable: false,
        min_size: Some((76., COMPACT_H)),
        ..Default::default()
    })
    .macos_window(
        rune::MacWindowOptions::default()
            .level(25)
            .shadow(true)
            .nonactivating_panel(true),
    )
    .on_ready(|window| {
        docking::register(window);
        window.place(
            rune::DisplayTarget::Primary,
            rune::OverlayPlacement::Edge {
                edge: rune::ScreenEdge::Top,
                margin: 0.,
                align: rune::Align::Center,
                inset: 0.,
            },
        )
    })
    .compositor(water::Water::default())
    .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shells_rasterize_at_full_2x_resolution() {
        for (i, image) in side_surfaces().iter().enumerate() {
            let w = if i % 2 == 0 { 152 } else { 660 };
            assert_eq!(image.wait().unwrap(), [w, 720], "side shell {i}");
        }
        for (image, h) in notch_surfaces().iter().zip([104, 460]) {
            assert_eq!(image.wait().unwrap(), [648, h]);
        }
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
                let rail = if edge == docking::Edge::Right {
                    size.0 - 76.
                } else {
                    0.
                };
                let (x, y) = if side { (rail + 35., 60.) } else { (35., 25.) };
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
                let (x, y) = if side {
                    (rail + 38., 330.)
                } else {
                    (258., 26.)
                };
                rt.pointer_move(x, y);
                rt.pointer_down(x, y);
                rt.pointer_up(x, y);
                rt.frame(0.016);
                assert!(
                    rt.state.ripples.is_empty(),
                    "navigation {edge:?}/{expanded}"
                );
                if expanded {
                    let (x, y) = if side {
                        (
                            if edge == docking::Edge::Right {
                                120.
                            } else {
                                180.
                            },
                            160.,
                        )
                    } else {
                        (150., 160.)
                    };
                    rt.pointer_move(x, y);
                    rt.pointer_down(x, y);
                    rt.pointer_up(x, y);
                    rt.frame(0.016);
                    assert!(rt.state.ripples.is_empty(), "preview {edge:?}");
                }
                rt.state.pointer = if side { [rail + 30., 20.] } else { [110., 26.] };
                ripple(&mut rt.state);
                assert!(rt.state.ripples.is_empty(), "header {edge:?}");
            }
        }
    }
    #[test]
    fn demo_drag_has_png_and_never_fake_file() {
        let mut s = State {
            dock: docking::Dock::default(),
            entries: vec![],
            shell: notch_surfaces(),
            side_shell: side_surfaces(),
            pointer: [110., 26.],
            clock: 0.,
            ripples: vec![],
            cache: ImageCache::new(Arc::new(|| {})),
            thumbs: None,
            dir: PathBuf::new(),
            selected: 0,
            scroll: 0.,
            wheel_last: -1.,
            wheel_total: 0.,
            wheel_latched: false,
            expanded: false,
            paused: false,
            pending: false,
            menu_restore_compact: false,
            menu_host: false,
            menu_at: None,
            change: 0,
            status: String::new(),
            demo: true,
        };
        s.fixtures();
        let mut rt = rune::Runtime::new(s, view, (324., 230.), rune::Env::new(true, 1.));
        rt.state.expanded = true;
        rt.frame(0.);
        rt.pointer_move(130., 26.);
        rt.pointer_down(130., 26.);
        assert!(
            rt.take_titlebar_action().is_some(),
            "header must start a native window drag"
        );
        rt.pointer_up(130., 26.);
        rt.frame(0.016);
        rt.wheel_xy(30., 80., 0., -48.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 1);
        let drawn = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("image-0"))
            .unwrap()
            .rect[0];
        assert!(
            drawn > -47. && drawn < 22.,
            "must animate between endpoints, got {drawn}"
        );
        for _ in 0..120 {
            rt.frame(1. / 60.);
        }
        rt.state.clock += 0.3;
        rt.wheel_xy(30., 80., -99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 2, "large delta moves only one image");
        rt.state.clock += 0.3;
        rt.wheel_xy(30., 80., 99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 1);
        rt.state.selected = 0;
        rt.state.scroll = 0.;
        rt.state.clock += 0.3;
        rt.state.expanded = false;
        rt.wheel_xy(30., 20., 0., -48.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 1);
        rt.pointer_move(258., 26.);
        rt.pointer_down(258., 26.);
        rt.pointer_up(258., 26.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 2, "ripple must not block navigation");
        assert!(
            rt.state.ripples.is_empty(),
            "navigation must not trigger water"
        );
        rt.state.pointer = [20., 20.];
        for _ in 0..10 {
            ripple(&mut rt.state);
        }
        assert_eq!(rt.state.ripples.len(), 5, "bounded transient effects");
        rt.state.clock += 0.3;
        rt.wheel_event(
            rune::WheelEvent::new(0., -3.)
                .at(30., 20.)
                .precise(true)
                .phase(rune::GesturePhase::Began),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 2, "small trackpad noise is ignored");
        rt.wheel_event(rune::WheelEvent::new(0., -20.).at(30., 20.).precise(true));
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3);
        rt.wheel_event(rune::WheelEvent::new(0., -400.).at(30., 20.).precise(true));
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3, "same swipe cannot skip images");
        rt.wheel_event(
            rune::WheelEvent::new(0., 0.)
                .at(30., 20.)
                .precise(true)
                .phase(rune::GesturePhase::Ended),
        );
        rt.frame(0.016);
        // Rune recognizes a new phase immediately after finger lift as momentum.
        rt.wheel_event(
            rune::WheelEvent::new(0., -999.)
                .at(30., 20.)
                .precise(true)
                .phase(rune::GesturePhase::Began),
        );
        rt.frame(0.016);
        assert_eq!(rt.state.selected, 3, "inertia must not change selection");
        rt.wheel_event(
            rune::WheelEvent::new(0., 0.)
                .at(30., 20.)
                .precise(true)
                .phase(rune::GesturePhase::Ended),
        );
        rt.frame(0.3);
        rt.state.clock += 0.3;
        rt.wheel_event(
            rune::WheelEvent::new(0., 12.)
                .at(30., 20.)
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

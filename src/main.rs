mod store;
mod water;
use rune::{
    AppSpec, ClipboardContent, ClipboardImage, Cx, DragItem, El, Image, ImageCache, TextStyle, Tx,
    Waker, el, text,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
struct Entry {
    path: PathBuf,
    png: Arc<[u8]>,
    image: Image,
    size: (u32, u32),
}
struct State {
    entries: Vec<Entry>,
    shell: [Image; 2],
    pointer: [f32; 2],
    clock: f64,
    ripples: Vec<([f32; 2], f64)>,
    cache: ImageCache,
    dir: PathBuf,
    selected: usize,
    scroll: f32,
    expanded: bool,
    paused: bool,
    pending: bool,
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
            entries: vec![],
            shell: notch_surfaces(),
            pointer: [110., 26.],
            clock: 0.,
            ripples: vec![],
            cache: ImageCache::new(Arc::new(|| {})),
            dir,
            selected: 0,
            scroll: 0.,
            expanded: false,
            paused: std::env::args().any(|a| a == "--paused"),
            pending: false,
            change: -1,
            status: "Copy an image to start your shelf".into(),
            demo,
        };
        if demo {
            s.fixtures();
        } else {
            match store::list(&s.dir) {
                Ok(paths) => {
                    for path in paths.into_iter().take(store::MAX_ITEMS) {
                        if std::fs::metadata(&path)
                            .map_or(true, |m| m.len() > store::MAX_IMAGE as u64)
                        {
                            continue;
                        }
                        if let Ok(bytes) = std::fs::read(&path) {
                            s.insert(path, bytes.into());
                        }
                    }
                }
                Err(e) => s.status = format!("Cannot load history: {e}"),
            }
        }
        s
    }
    fn insert(&mut self, path: PathBuf, png: Arc<[u8]>) {
        if self.entries.iter().any(|e| e.path == path) {
            return;
        }
        if let Some(clip) = ClipboardImage::from_png(png.clone()) {
            let image = self.cache.load(thumbnail(&png));
            self.entries.push(Entry {
                path,
                png,
                image,
                size: clip.size(),
            });
        }
    }
    fn capture(&mut self, content: &ClipboardContent) {
        if self.paused {
            return;
        }
        let Some(image) = content.image() else {
            return;
        };
        let png: Arc<[u8]> = image.png().into();
        match store::save(&self.dir, &png) {
            Ok(path) => {
                if self.entries.iter().any(|e| e.path == path) {
                    return;
                }
                self.insert(path, png);
                let e = self.entries.pop().unwrap();
                self.entries.insert(0, e);
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
            self.insert(
                PathBuf::from(format!("demo-{n}.png")),
                out.into_inner().into(),
            );
        }
        for e in &self.entries {
            let _ = e.image.wait();
        }
        self.status = "Demo images • clipboard monitoring disabled".into();
    }
    fn drag(&self, i: usize) -> Option<DragItem> {
        let e = self.entries.get(i)?;
        let item = DragItem::new().png(e.png.to_vec());
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
    for e in &mut s.entries {
        e.image = s.cache.load(thumbnail(&e.png));
    }
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(700));
            wake();
        }
    });
}
fn poll(s: &mut State, t: f64, tx: &mut Tx) -> bool {
    s.clock = t;
    let animating = !s.ripples.is_empty();
    s.ripples.retain(|(_, start)| t - start < 1.1);
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
fn thumbnail(png: &[u8]) -> Arc<[u8]> {
    if let Ok(img) = image::load_from_memory(png) {
        let mut out = std::io::Cursor::new(vec![]);
        if img
            .thumbnail(1200, 900)
            .write_to(&mut out, image::ImageFormat::Png)
            .is_ok()
        {
            return out.into_inner().into();
        }
    }
    png.into()
}
fn c(n: u32) -> rune::Color {
    rune::color::hex(&format!("#{n:06x}")).unwrap()
}
fn label(v: impl AsRef<str>, size: f32, color: u32) -> El {
    text(v.as_ref())
        .font(TextStyle::new(size, 400.))
        .color(c(color))
}
fn button(v: &str, x: f32, y: f32, w: f32) -> El {
    el().absolute(x, y)
        .size(w, 24.)
        .radius(8.)
        .gradient(c(0x24262d), c(0x13151a))
        .border(0.5, c(0x373a44))
        .hit()
        .cursor(rune::Cursor::Pointer)
        .center()
        .child(label(v, 13., 0xe3e9f2))
}
fn picture(e: &Entry, x: f32, y: f32, w: f32, h: f32) -> El {
    let ratio = (w / e.size.0 as f32).min(h / e.size.1 as f32);
    let (pw, ph) = (e.size.0 as f32 * ratio, e.size.1 as f32 * ratio);
    el().absolute(x, y)
        .size(w, h)
        .gradient(c(0x191b22), c(0x0e0f13))
        .radius(8.)
        .clip_rounded(8.)
        .child(
            el().absolute((w - pw) / 2., (h - ph) / 2.)
                .size(pw, ph)
                .pixels(e.image.pixels()),
        )
}
const ISLAND_W: f32 = 324.;
const BODY_W: f32 = 300.;
const SHOULDER: f32 = 12.;
const COMPACT_H: f32 = 52.;
const EXPANDED_H: f32 = 230.;
// Both horizontal trackpad motion and a conventional vertical wheel browse history.
fn wheel(s: &mut State, event: rune::WheelEvent, tx: &mut Tx) -> bool {
    let delta = if event.dx.abs() > event.dy.abs() {
        event.dx
    } else {
        event.dy
    };
    if !delta.is_finite() || delta == 0. || s.entries.is_empty() {
        return true;
    }
    s.scroll = (s.scroll - delta / 48.).clamp(0., s.entries.len().saturating_sub(1) as f32);
    s.selected = s.scroll.round() as usize;
    tx.animated();
    true
}
fn ripple(s: &mut State) {
    if s.ripples.len() >= 5 {
        s.ripples.remove(0);
    }
    s.ripples.push((s.pointer, s.clock));
}
fn toggle(s: &mut State, tx: &mut Tx) {
    ripple(s);
    s.expanded = !s.expanded;
    tx.resize_window((ISLAND_W, if s.expanded { EXPANDED_H } else { COMPACT_H }));
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
        .item(rune::PopupItem::new("status", &s.status).enabled(false))
        .separator()
        .item(rune::PopupItem::new("quit", "Quit Image Shelf"))
}
fn menu_action(s: &mut State, picked: Option<&str>, tx: &mut Tx) {
    match picked {
        Some("copy") => {
            if let Some(e) = s.entries.get(s.selected)
                && let Some(img) = ClipboardImage::from_png(e.png.clone())
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
        Some("quit") => tx.quit(),
        _ => {}
    }
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
    let mut root = el()
        .key("notch")
        .on_pointer_move(|s: &mut State, p| s.pointer = [p.x - SHOULDER, p.y])
        .on_press(ripple)
        .on_wheel(wheel)
        .absolute(SHOULDER, 0.)
        .size(BODY_W, if s.expanded { EXPANDED_H } else { COMPACT_H })
        .on_secondary_press(|s: &mut State, press: rune::PointerPress, tx| {
            tx.popup_menu(menu(s), press, menu_action)
        })
        .child(
            el().absolute(56., 10.)
                .size(144., 32.)
                .key("notch-header")
                .on_press_tx(toggle)
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
            button("‹", 206., 14., 24.)
                .key("select-previous")
                .on_press(|s: &mut State| {
                    ripple(s);
                    s.selected = s.selected.saturating_sub(1);
                    s.scroll = s.selected as f32;
                }),
        )
        .child(
            button("›", 234., 14., 24.)
                .key("select-next")
                .on_press(|s: &mut State| {
                    ripple(s);
                    if s.selected + 1 < s.entries.len() {
                        s.selected += 1;
                        s.scroll = s.selected as f32;
                    }
                }),
        )
        .child(
            button(if s.expanded { "−" } else { "+" }, 262., 14., 24.)
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
        for (i, e) in s.entries.iter().enumerate() {
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
        for (i, e) in s.entries.iter().enumerate() {
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
                    .on_press(ripple)
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
        min_size: Some((ISLAND_W, COMPACT_H)),
        scenes: &[
            ("expanded", |s| s.expanded = true),
            ("water", |s| {
                s.expanded = true;
                s.ripples.push(([110., 155.], 0.));
            }),
        ],
        ..AppSpec::new("Image Shelf", (ISLAND_W, COMPACT_H), State::new, view)
    })
    .window_options(rune::WindowOptions {
        transparent: true,
        borderless: true,
        always_on_top: true,
        resizable: false,
        min_size: Some((ISLAND_W, COMPACT_H)),
        ..Default::default()
    })
    .macos_window(
        rune::MacWindowOptions::default()
            .level(25)
            .shadow(true)
            .nonactivating_panel(true),
    )
    .on_ready(|window| {
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
    fn demo_drag_has_png_and_never_fake_file() {
        let mut s = State {
            entries: vec![],
            shell: notch_surfaces(),
            pointer: [110., 26.],
            clock: 0.,
            ripples: vec![],
            cache: ImageCache::new(Arc::new(|| {})),
            dir: PathBuf::new(),
            selected: 0,
            scroll: 0.,
            expanded: false,
            paused: false,
            pending: false,
            change: 0,
            status: String::new(),
            demo: true,
        };
        s.fixtures();
        let mut rt = rune::Runtime::new(s, view, (324., 230.), rune::Env::new(true, 1.));
        rt.state.expanded = true;
        rt.frame(0.);
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
        rt.wheel_xy(30., 80., -99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 5);
        rt.wheel_xy(30., 80., 99999., 0.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 0);
        rt.state.expanded = false;
        rt.wheel_xy(30., 20., 0., -48.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 1);
        rt.pointer_move(258., 26.);
        rt.pointer_down(258., 26.);
        rt.pointer_up(258., 26.);
        rt.frame(1. / 60.);
        assert_eq!(rt.state.selected, 2, "ripple must not block navigation");
        assert_eq!(rt.state.ripples.len(), 1);
        assert_eq!(rt.state.ripples[0].0, [246., 26.]);
        for _ in 0..10 {
            ripple(&mut rt.state);
        }
        assert_eq!(rt.state.ripples.len(), 5, "bounded transient effects");
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

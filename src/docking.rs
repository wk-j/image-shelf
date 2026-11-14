//! Screen-edge placement in AppKit points, independent of Retina scale.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Edge {
    #[default]
    Top,
    Left,
    Right,
    Free,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
fn contains(r: Rect, p: (f64, f64)) -> bool {
    p.0 >= r.x && p.0 <= r.x + r.w && p.1 >= r.y && p.1 <= r.y + r.h
}
fn snapped(mut r: Rect, screen: Rect, visible: Rect, edge: Edge) -> Rect {
    match edge {
        Edge::Top => {
            r.x = screen.x + (screen.w - r.w) / 2.;
            r.y = screen.y + screen.h - r.h;
        }
        Edge::Left => {
            r.x = screen.x;
            r.y = visible.y + (visible.h - r.h) / 2.;
        }
        Edge::Right => {
            r.x = screen.x + screen.w - r.w;
            r.y = visible.y + (visible.h - r.h) / 2.;
        }
        Edge::Free => {}
    }
    if edge != Edge::Top {
        r.y =
            r.y.clamp(visible.y, (visible.y + visible.h - r.h).max(visible.y));
    }
    r
}
fn target(r: Rect, screen: Rect, _visible: Rect, p: (f64, f64)) -> Edge {
    if (p.0 - screen.x).abs() <= 28. || (r.x - screen.x).abs() <= 28. {
        Edge::Left
    } else if (p.0 - screen.x - screen.w).abs() <= 28.
        || (r.x + r.w - screen.x - screen.w).abs() <= 28.
    {
        Edge::Right
    } else if (p.1 - screen.y - screen.h).abs() <= 28. {
        Edge::Top
    } else {
        Edge::Free
    }
}
#[derive(Default)]
pub struct Dock {
    pub edge: Edge,
    anchor: Option<(Rect, Rect)>,
    last_frame: Option<Rect>,
    moving: bool,
}
#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2::{class, msg_send, runtime::AnyObject};
    use objc2_foundation::{NSPoint, NSRect};
    use std::sync::atomic::{AtomicU32, Ordering};
    static WINDOW: AtomicU32 = AtomicU32::new(0);
    pub fn register(w: &rune::WindowHandle) {
        WINDOW.store(w.window_number().unwrap_or(0), Ordering::Relaxed);
    }
    unsafe fn window() -> *mut AnyObject {
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            msg_send![app,windowWithWindowNumber:WINDOW.load(Ordering::Relaxed) as isize]
        }
    }
    fn rect(r: NSRect) -> Rect {
        Rect {
            x: r.origin.x,
            y: r.origin.y,
            w: r.size.width,
            h: r.size.height,
        }
    }
    unsafe fn frame(w: *mut AnyObject) -> Rect {
        unsafe { rect(msg_send![w, frame]) }
    }
    unsafe fn screens(p: (f64, f64)) -> Option<(Rect, Rect)> {
        unsafe {
            let screens: *mut AnyObject = msg_send![class!(NSScreen), screens];
            let n: usize = msg_send![screens, count];
            let mut first = None;
            for i in 0..n {
                let screen: *mut AnyObject = msg_send![screens,objectAtIndex:i];
                let f = rect(msg_send![screen, frame]);
                let v = rect(msg_send![screen, visibleFrame]);
                if first.is_none() {
                    first = Some((f, v));
                }
                if contains(f, p) {
                    return Some((f, v));
                }
            }
            first
        }
    }
    unsafe fn move_to(w: *mut AnyObject, r: Rect) {
        unsafe {
            let _: () = msg_send![w,setFrameOrigin:NSPoint::new(r.x.round(),r.y.round())];
        }
    }
    pub fn place(dock: &mut Dock, edge: Edge) {
        if rune::is_headless() {
            dock.edge = edge;
            return;
        }
        unsafe {
            let w = window();
            if w.is_null() {
                return;
            }
            let r = frame(w);
            if let Some((screen, visible)) = screens((r.x + r.w / 2., r.y + r.h / 2.)) {
                dock.edge = edge;
                dock.anchor = Some((screen, visible));
                let r = snapped(r, screen, visible, edge);
                move_to(w, r);
                dock.last_frame = Some(r);
            }
        }
    }
    pub fn tick(dock: &mut Dock) -> bool {
        if rune::is_headless() {
            return false;
        }
        unsafe {
            let w = window();
            if w.is_null() {
                return false;
            }
            let r = frame(w);
            let buttons: usize = msg_send![class!(NSEvent), pressedMouseButtons];
            let resizing = dock
                .last_frame
                .is_some_and(|old| old.w != r.w || old.h != r.h);
            let moved = dock
                .last_frame
                .is_some_and(|old| old.x != r.x || old.y != r.y);
            dock.last_frame = Some(r);
            if !resizing && moved {
                dock.moving = true;
            }
            if buttons & 1 != 0 {
                return dock.moving;
            }
            let settled = dock.moving;
            if dock.moving {
                dock.moving = false;
                let p: NSPoint = msg_send![class!(NSEvent), mouseLocation];
                if let Some((screen, visible)) = screens((r.x + r.w / 2., r.y + r.h / 2.)) {
                    dock.edge = target(r, screen, visible, (p.x, p.y));
                    dock.anchor = Some((screen, visible));
                }
            }
            if (moved || resizing || settled)
                && let Some((screen, visible)) = dock.anchor
            {
                let placed = snapped(r, screen, visible, dock.edge);
                move_to(w, placed);
                dock.last_frame = Some(placed);
            }
            false
        }
    }
}
#[cfg(target_os = "macos")]
pub use native::{place, register, tick};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snap_negative_display_and_dock_work_area() {
        let screen = Rect {
            x: -1920.,
            y: 0.,
            w: 1920.,
            h: 1080.,
        };
        let visible = Rect {
            x: -1870.,
            y: 60.,
            w: 1870.,
            h: 996.,
        };
        let r = Rect {
            x: -1800.,
            y: 1040.,
            w: 324.,
            h: 230.,
        };
        let left = snapped(r, screen, visible, Edge::Left);
        assert_eq!((left.x, left.y), (-1920., 443.));
        let right = snapped(r, screen, visible, Edge::Right);
        assert_eq!(right.x, -324.);
        assert_eq!(target(r, screen, visible, (-1910., 500.)), Edge::Left);
        assert_eq!(target(r, screen, visible, (-10., 500.)), Edge::Right);
        assert_eq!(target(r, screen, visible, (-1000., 1080.)), Edge::Top);
        assert!(contains(screen, (-1500., 700.)));
    }
    #[test]
    fn resized_side_stays_flush_and_top_centers() {
        let s = Rect {
            x: 0.,
            y: 0.,
            w: 1920.,
            h: 1080.,
        };
        let v = Rect { h: 1055., ..s };
        for h in [52., 230.] {
            let r = Rect {
                x: 800.,
                y: 900.,
                w: 324.,
                h,
            };
            assert_eq!(snapped(r, s, v, Edge::Right).x, 1596.);
            let top = snapped(r, s, v, Edge::Top);
            assert_eq!((top.x, top.y), (798., 1080. - h));
        }
        assert_eq!(
            target(
                Rect {
                    x: 800.,
                    y: 500.,
                    w: 324.,
                    h: 52.
                },
                s,
                v,
                (900., 500.)
            ),
            Edge::Free
        );
    }
}

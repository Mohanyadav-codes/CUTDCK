//! The corner Tray window: placement, mouse pass-through outside its visible
//! parts, and catching file drags that head for the corner.

use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const TRAY_W: f64 = 600.0;
pub const TRAY_H: f64 = 680.0;
const MARGIN: f64 = 14.0;

/// Pin the tray window to the bottom-right of the work area of the monitor
/// that holds the cursor.
pub fn place(app: &AppHandle, win: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|c| app.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| win.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let wa = m.work_area();
        let scale = m.scale_factor();
        let w = (TRAY_W * scale) as i32;
        let h = (TRAY_H * scale) as i32;
        let x = wa.position.x + wa.size.width as i32 - w - (MARGIN * scale) as i32;
        let y = wa.position.y + wa.size.height as i32 - h - (MARGIN * scale) as i32;
        let _ = win.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

pub fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("tray") {
        place(app, &w);
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
}

#[cfg(windows)]
fn left_button_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    unsafe { (GetAsyncKeyState(VK_LBUTTON as i32) as u16 & 0x8000) != 0 }
}

#[cfg(not(windows))]
fn left_button_down() -> bool {
    false
}

/// Poll the cursor and let clicks through everywhere except over the tray's
/// visible elements (reported by the front end as hit rects).
pub fn spawn_hit_loop(app: AppHandle) {
    std::thread::spawn(move || {
        let mut interactive = true;
        let mut pressed_outside = false;
        let mut was_down = false;
        let mut drag_near = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(30));
            let Some(win) = app.get_webview_window("tray") else { continue };
            if !win.is_visible().unwrap_or(false) {
                continue;
            }
            let (Ok(cur), Ok(pos), Ok(scale)) = (app.cursor_position(), win.outer_position(), win.scale_factor()) else { continue };
            let lx = (cur.x - pos.x as f64) / scale;
            let ly = (cur.y - pos.y as f64) / scale;
            let st = app.state::<AppState>();
            let over = st.hit.lock().unwrap().iter().any(|r| r.contains(lx, ly));
            let zone = *st.drop_zone.lock().unwrap();
            let in_zone = zone.map(|z| z.contains(lx, ly)).unwrap_or(false);
            let down = left_button_down();
            if down && !was_down {
                pressed_outside = !(lx >= 0.0 && ly >= 0.0 && lx <= TRAY_W && ly <= TRAY_H);
            }
            if !down {
                pressed_outside = false;
            }
            was_down = down;
            // A press that started elsewhere and is now near the corner is a drag.
            let dragging_in = down && pressed_outside && in_zone;
            if dragging_in != drag_near {
                drag_near = dragging_in;
                let _ = app.emit_to("tray", "cd://drag-near", drag_near);
            }
            let want = over || dragging_in;
            if want != interactive {
                interactive = want;
                let _ = win.set_ignore_cursor_events(!want);
            }
        }
    });
}

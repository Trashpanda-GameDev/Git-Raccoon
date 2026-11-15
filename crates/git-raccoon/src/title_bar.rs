//! The window's title bar follows the theme: light or dark wherever the
//! system allows, and on Windows 11 the theme's exact colours.

use eframe::egui::{self, Color32};

/// Light or dark title bar (Windows 10 and 11, macOS, Wayland). `None`
/// leaves it to the system, for the System theme, so the app still hears
/// when the system switches.
pub fn follow(ctx: &egui::Context, light: Option<bool>) {
    let theme = match light {
        None => egui::SystemTheme::SystemDefault,
        Some(true) => egui::SystemTheme::Light,
        Some(false) => egui::SystemTheme::Dark,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(theme));
}

/// On Windows 11, colours the title bar exactly: its background, text and
/// border. (Older Windows ignores this and keeps the light or dark bar.)
#[cfg(windows)]
pub fn colour(frame: &eframe::Frame, background: Color32, text: Color32, border: Color32) {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute,
    };

    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(window) = handle.as_raw() else {
        return;
    };
    let hwnd = window.hwnd.get() as windows_sys::Win32::Foundation::HWND;
    // Windows wants 0x00BBGGRR.
    let colorref =
        |c: Color32| u32::from(c.r()) | (u32::from(c.g()) << 8) | (u32::from(c.b()) << 16);
    for (attribute, value) in [
        (DWMWA_CAPTION_COLOR, colorref(background)),
        (DWMWA_TEXT_COLOR, colorref(text)),
        (DWMWA_BORDER_COLOR, colorref(border)),
    ] {
        #[expect(
            unsafe_code,
            reason = "a Win32 call with a live window handle and a 4-byte value it reads"
        )]
        // SAFETY: `hwnd` is this app's own window, alive while `frame` is;
        // the value is a u32 on the stack, and its size is passed.
        unsafe {
            DwmSetWindowAttribute(hwnd, attribute as u32, (&raw const value).cast(), 4);
        }
    }
}

#[cfg(not(windows))]
pub fn colour(_frame: &eframe::Frame, _background: Color32, _text: Color32, _border: Color32) {}

//! "Now playing" notifications on a track change (issue #94), behind the `track_notifications`
//! setting. Off by default: Linux desktops already show the MPRIS widget, and a notification per
//! song is a preference, not something everyone wants.
//!
//! Only while no Limusic window has focus: a song change the user is looking at needs no toast.
//! Best-effort like `media.rs`: a missing notification daemon is a `debug!` line.

#[cfg(all(unix, not(target_os = "macos")))]
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// The last song's notification, closed before the next one is shown so a run of skips leaves one
/// up, not a stack. Not `replaces_id`: Plasma updates an expired notification in place in its
/// history and never pops it up again, so only the first skip was visible.
#[cfg(all(unix, not(target_os = "macos")))]
static PREV: Mutex<Option<notify_rust::NotificationHandle>> = Mutex::new(None);

pub fn track_changed(app: &AppHandle, title: &str, artists: &str) {
    if app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false)) {
        return;
    }
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(artists).appname("Limusic");
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // The binary name is the icon name the .deb/.rpm install. An AppImage has no themed icon,
        // so there the daemon shows its generic one.
        n.auto_icon().hint(notify_rust::Hint::SuppressSound(true));
    }
    // A toast is only delivered for an AppUserModelID that a Start menu shortcut registers, and the
    // NSIS installer registers the bundle identifier. A dev build has no shortcut, so it keeps
    // notify-rust's default (PowerShell's).
    #[cfg(target_os = "windows")]
    if !tauri::is_dev() {
        n.app_id(&app.config().identifier);
    }
    // Which app macOS files the notification under. Errors after the first call (it is set once per
    // process), which is fine. A dev binary has no bundle, so it posts as Terminal.
    #[cfg(target_os = "macos")]
    let _ = notify_rust::set_application(if tauri::is_dev() {
        "com.apple.Terminal"
    } else {
        &app.config().identifier
    });
    // A blocking D-Bus / WinRT / AppKit call: keep it off the playback path.
    std::thread::spawn(move || {
        // Held across close + show, so two quick skips can't both close the same one.
        #[cfg(all(unix, not(target_os = "macos")))]
        let mut prev = PREV.lock().unwrap_or_else(|e| e.into_inner());
        #[cfg(all(unix, not(target_os = "macos")))]
        if let Some(h) = prev.take() {
            h.close();
        }
        match n.show() {
            #[cfg(all(unix, not(target_os = "macos")))]
            Ok(h) => *prev = Some(h),
            #[cfg(not(all(unix, not(target_os = "macos"))))]
            Ok(_) => {}
            Err(e) => tracing::debug!("notification: {e}"),
        }
    });
}

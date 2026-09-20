//! The video window's native playback menu (play/pause, stop, speed,
//! fullscreen, subtitles), built with `muda`.

use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

use crate::server::soap::types::av_transport::TransportState;

use super::{MenuState, PlaybackControls};

const PLAY_PAUSE_ID: &str = "play_pause";
const STOP_ID: &str = "stop";
const FULLSCREEN_ID: &str = "fullscreen";
const SUBTITLES_ID: &str = "subtitles";
const SPEED_ID_PREFIX: &str = "speed:";

fn speed_id(speed: f64) -> String {
    format!("{SPEED_ID_PREFIX}{speed}")
}

fn speed_label(speed: f64) -> String {
    if (speed - 1.0).abs() < f64::EPSILON {
        "Normal".to_string()
    } else {
        format!("{speed}x")
    }
}

/// What a menu click needs handled outside this module - everything else
/// (play/pause, stop, speed, subtitles) [`PlaybackMenu::dispatch`] acts on
/// entirely by itself.
pub(super) enum DispatchResult {
    /// The window itself has to change (`winit::window::Window::set_fullscreen`)
    /// - this module never holds a window reference.
    ToggleFullscreen,
    Handled,
}

/// The menu plus the item handles needed to keep it in sync with
/// [`MenuState`] as it changes.
pub(super) struct PlaybackMenu {
    // Never read after `build()` - held only so it doesn't drop. `Menu`
    // tears its native menu bar down when its last handle is dropped (see
    // `muda::Menu`'s `Drop` impl), so a `Menu` that's built, attached and
    // then let go out of scope disappears again immediately.
    _menu: Menu,
    play_pause: MenuItem,
    speed_items: Vec<(f64, CheckMenuItem)>,
    fullscreen: CheckMenuItem,
    subtitles: CheckMenuItem,
}

impl PlaybackMenu {
    /// Builds the menu and attaches it - on macOS, globally to the app
    /// (`Menu::init_for_nsapp`), which works even before any window exists,
    /// so this runs as soon as [`PlaybackControls`] arrives rather than
    /// waiting for the video window to open.
    pub(super) fn build(controls: &PlaybackControls) -> Self {
        let menu = Menu::new();

        // macOS always renames the menu bar's index-0 submenu's title to the
        // process name, regardless of what's set on it - so if "Playback"
        // were index 0, its own title would get silently overwritten.
        // Giving it a dedicated (conventional, Quit-holding) app submenu at
        // index 0 first keeps "Playback" as the title actually shown.
        #[cfg(target_os = "macos")]
        {
            let app_menu = Submenu::with_id("app_menu", "", true);
            let _ = app_menu.append(&PredefinedMenuItem::quit(None));
            let _ = menu.append(&app_menu);
        }

        // macOS's menu bar can only hold top-level `Submenu`s (the "File",
        // "Edit", ... entries) - a bare `MenuItem` appended directly to
        // `menu` is silently invalid there, so everything lives inside this
        // one "Playback" dropdown instead.
        let playback_menu = Submenu::with_id("playback_menu", "Playback", true);

        let play_pause = MenuItem::with_id(PLAY_PAUSE_ID, "Play", true, None);
        let stop = MenuItem::with_id(STOP_ID, "Stop", true, None);

        let speed_menu = Submenu::with_id("speed_menu", "Speed", true);
        let mut speed_items = Vec::with_capacity(controls.supported_speeds.len());
        for &speed in &controls.supported_speeds {
            let checked = (speed - 1.0).abs() < f64::EPSILON;
            let item = CheckMenuItem::with_id(speed_id(speed), speed_label(speed), true, checked, None);
            let _ = speed_menu.append(&item);
            speed_items.push((speed, item));
        }

        let fullscreen = CheckMenuItem::with_id(FULLSCREEN_ID, "Fullscreen", true, false, None);
        let subtitles = CheckMenuItem::with_id(SUBTITLES_ID, "Subtitles", true, false, None);

        let _ = playback_menu.append(&play_pause);
        let _ = playback_menu.append(&stop);
        let _ = playback_menu.append(&PredefinedMenuItem::separator());
        let _ = playback_menu.append(&speed_menu);
        let _ = playback_menu.append(&PredefinedMenuItem::separator());
        let _ = playback_menu.append(&fullscreen);
        let _ = playback_menu.append(&subtitles);
        let _ = menu.append(&playback_menu);

        attach(&menu);

        Self {
            _menu: menu,
            play_pause,
            speed_items,
            fullscreen,
            subtitles,
        }
    }

    /// Reflects `state` in the menu's labels/checkmarks. Doesn't touch
    /// `fullscreen`'s checkmark - that's purely local window state this
    /// module never sees, kept in sync separately via
    /// [`Self::set_fullscreen_checked`].
    pub(super) fn update(&self, state: &MenuState) {
        self.play_pause.set_text(if state.transport_state == TransportState::Playing {
            "Pause"
        } else {
            "Play"
        });

        for (speed, item) in &self.speed_items {
            item.set_checked((speed - state.speed).abs() < f64::EPSILON);
        }

        self.subtitles.set_checked(state.subtitles_enabled);
    }

    pub(super) fn set_fullscreen_checked(&self, checked: bool) {
        self.fullscreen.set_checked(checked);
    }

    /// Matches `event` against this menu's items and acts on it - calling
    /// straight into `controls` for play/pause/stop/speed, and toggling its
    /// own checkbox for subtitles (subtitle state comes back from
    /// `toggle_subtitles` synchronously, unlike everything else here).
    pub(super) fn dispatch(&self, event: &muda::MenuEvent, controls: &PlaybackControls) -> DispatchResult {
        let id = event.id().as_ref();

        if id == PLAY_PAUSE_ID {
            (controls.play_pause)();
        } else if id == STOP_ID {
            (controls.stop)();
        } else if id == FULLSCREEN_ID {
            return DispatchResult::ToggleFullscreen;
        } else if id == SUBTITLES_ID {
            let enabled = (controls.toggle_subtitles)();
            self.subtitles.set_checked(enabled);
        } else if let Some(speed) = id
            .strip_prefix(SPEED_ID_PREFIX)
            .and_then(|s| s.parse::<f64>().ok())
        {
            (controls.set_speed)(speed);
        }

        DispatchResult::Handled
    }
}

#[cfg(target_os = "macos")]
fn attach(menu: &Menu) {
    menu.init_for_nsapp();
}

#[cfg(not(target_os = "macos"))]
fn attach(_menu: &Menu) {
    // Windows/Linux attach per-window (`init_for_hwnd`/`init_for_gtk_window`)
    // rather than app-globally - deferred to the window's own creation in
    // `app.rs` on those platforms, not implemented here yet since this
    // project's tested platform is macOS.
}

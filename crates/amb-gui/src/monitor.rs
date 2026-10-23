// SPDX-License-Identifier: GPL-3.0-or-later
//! The monitor's colour profile, so the preview shows true colours on a
//! wide-gamut or calibrated screen.
//!
//! The preview is made in sRGB. On macOS and Windows the system webview
//! already converts sRGB pictures to the monitor's profile, so nothing is
//! done there. On Linux nothing does, so the editor converts its own
//! pictures (`apply`) to the profile the desktop sets for the monitor the
//! window is on (the `_ICC_PROFILE` property on X11, set by colord and
//! the desktop's colour settings), or to a file the user chose in Settings.
//! Without either, the preview stays sRGB.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use amb_color::display::ToDisplay;
use serde::{Deserialize, Serialize};

/// What the user chose in Settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "mode", content = "path")]
pub(crate) enum Choice {
    /// The profile the desktop gives for the window's monitor.
    #[default]
    Auto,
    /// Plain sRGB: no conversion.
    Off,
    /// An ICC profile file.
    File(PathBuf),
}

/// What Settings shows: the profile in use, if any, and where it came from.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub(crate) struct Status {
    pub(crate) choice: Choice,
    /// The system matches colours itself (macOS, Windows): no choice to make.
    pub(crate) by_system: bool,
    /// The profile's own name, when one is in use.
    pub(crate) profile: Option<String>,
    /// A line for Settings: what is happening, or why not.
    pub(crate) note: String,
}

struct Current {
    transform: Option<ToDisplay>,
    status: Status,
}

static CURRENT: RwLock<Option<Arc<Current>>> = RwLock::new(None);

/// Convert sRGB pixels (packed RGB) to the monitor's colours, when a
/// profile is in use.
pub(crate) fn apply(rgb: &mut [u8]) {
    let current = CURRENT.read().ok().and_then(|c| c.clone());
    if let Some(t) = current.as_ref().and_then(|c| c.transform.as_ref()) {
        t.apply(rgb);
    }
}

/// Use `choice`; `desktop` is the desktop's profile for the window's
/// monitor (`desktop_profile`). Returns what is in use, and whether that
/// changed (the preview is then rendered again).
pub(crate) fn update(choice: &Choice, desktop: Option<(usize, Vec<u8>)>) -> (Status, bool) {
    let (status, transform) = resolve(choice, desktop);
    let mut slot = match CURRENT.write() {
        Ok(slot) => slot,
        Err(e) => e.into_inner(),
    };
    let changed = slot.as_ref().map(|c| &c.status) != Some(&status);
    *slot = Some(Arc::new(Current {
        transform,
        status: status.clone(),
    }));
    (status, changed)
}

fn resolve(choice: &Choice, desktop: Option<(usize, Vec<u8>)>) -> (Status, Option<ToDisplay>) {
    let mut status = Status {
        choice: choice.clone(),
        by_system: !cfg!(target_os = "linux"),
        ..Default::default()
    };
    if status.by_system {
        status.note = "The system shows the preview in your monitor's colours".into();
        return (status, None);
    }
    let (bytes, from) = match choice {
        Choice::Off => {
            status.note = "Off: the preview is plain sRGB".into();
            return (status, None);
        }
        Choice::File(path) => match std::fs::read(path) {
            Ok(b) => (b, path.display().to_string()),
            Err(e) => {
                status.note = format!(
                    "Couldn't read {}: {e}; the preview is plain sRGB",
                    path.display()
                );
                return (status, None);
            }
        },
        Choice::Auto => match desktop {
            Some((monitor, b)) => (
                b,
                format!("the desktop's profile for monitor {}", monitor + 1),
            ),
            None => {
                status.note = "The desktop gives no profile for this monitor; the preview is plain sRGB (choose a profile file to use one)".into();
                return (status, None);
            }
        },
    };
    let name = amb_color::icc::description(&bytes).unwrap_or_else(|| "unnamed profile".into());
    match amb_color::icc::parse(&bytes) {
        Some(p) => {
            status.note = format!("Using {name}, {from}");
            status.profile = Some(name);
            (status, Some(ToDisplay::new(&p)))
        }
        None => {
            status.note = format!(
                "{name} ({from}) is a kind of profile Ambrotype can't read yet (only matrix profiles); the preview is plain sRGB"
            );
            (status, None)
        }
    }
}

/// The desktop's profile for the monitor holding the point (`x`, `y`, in
/// logical pixels): its index and the profile's bytes. Must run on the
/// main (GTK) thread. X11 only; under Wayland the desktop doesn't say.
#[cfg(target_os = "linux")]
pub(crate) fn desktop_profile(x: i32, y: i32) -> Option<(usize, Vec<u8>)> {
    let display = gdk::Display::default()?;
    let at = display.monitor_at_point(x, y)?;
    let index = (0..display.n_monitors()).position(|i| display.monitor(i).as_ref() == Some(&at))?;
    let root = gdk::Screen::default()?.root_window()?;
    // The X ICC profile convention: monitor 0 is `_ICC_PROFILE`, then
    // `_ICC_PROFILE_1`, … The length is counted in 32-bit units.
    let name = match index {
        0 => "_ICC_PROFILE".to_string(),
        n => format!("_ICC_PROFILE_{n}"),
    };
    let (_, _, bytes) = gdk::property_get(
        &root,
        &gdk::Atom::intern(&name),
        &gdk::Atom::intern("CARDINAL"),
        0,
        (16 << 20) / 4,
        0,
    )?;
    (!bytes.is_empty()).then_some((index, bytes))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn desktop_profile(_x: i32, _y: i32) -> Option<(usize, Vec<u8>)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use amb_color::space::OutputSpace;

    #[test]
    fn a_desktop_profile_is_used_and_named() {
        let p3 = OutputSpace::DisplayP3.icc_profile();
        let (status, t) = resolve(&Choice::Auto, Some((1, p3)));
        if cfg!(target_os = "linux") {
            assert!(t.is_some());
            assert!(
                status.profile.as_deref().unwrap().contains("P3"),
                "{status:?}"
            );
            assert!(status.note.contains("monitor 2"), "{}", status.note);
        } else {
            assert!(t.is_none() && status.by_system);
        }
    }

    #[test]
    fn off_and_unreadable_profiles_leave_srgb() {
        let p3 = OutputSpace::DisplayP3.icc_profile();
        assert!(resolve(&Choice::Off, Some((0, p3))).1.is_none());
        assert!(
            resolve(&Choice::Auto, Some((0, b"not a profile".to_vec())))
                .1
                .is_none()
        );
        assert!(resolve(&Choice::Auto, None).1.is_none());
        let missing = Choice::File("/no/such/profile.icc".into());
        assert!(resolve(&missing, None).1.is_none());
    }
}

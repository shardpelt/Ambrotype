// SPDX-License-Identifier: GPL-3.0-or-later
//! The built-in effects, run natively: the same Rust their modules are
//! built from, linked into the app, so they run many times faster than in
//! the sandbox (and on all cores). Only for the built-in plugin itself,
//! checked by its module: a user's plugin always runs in the sandbox.

use amb_plugin_sdk::{Effect, Pixels};

type ColorFn = fn(&Effect, [f32; 3]) -> [f32; 3];
type PixelsFn = fn(&Effect, &mut Pixels) -> Result<(), String>;

pub(crate) enum Native {
    Color(ColorFn),
    Pixels(PixelsFn),
}

/// The native code of built-in effect `plugin`, if it is the built-in one.
pub(crate) fn of(plugin: &crate::Plugin) -> Option<Native> {
    let id = plugin.manifest.id.as_str();
    let builtin = crate::BUILTIN
        .iter()
        .any(|(_, b, wasm)| *b == id && *wasm == plugin.wasm.as_slice());
    if !builtin {
        return None;
    }
    Some(match id {
        "duotone" => Native::Color(amb_plugin_duotone::duotone),
        "hue-trip" => Native::Color(amb_plugin_hue_trip::trip),
        "light-leak" => Native::Pixels(amb_plugin_light_leak::leak),
        "holographic" => Native::Pixels(amb_plugin_holographic::foil),
        "paint" => Native::Pixels(amb_plugin_paint::paint),
        "tilt-shift" => Native::Pixels(amb_plugin_tilt_shift::tilt),
        "kaleidoscope" => Native::Pixels(amb_plugin_kaleidoscope::kaleidoscope),
        "heat-camera" => Native::Color(amb_plugin_heat_camera::heat),
        _ => return None,
    })
}

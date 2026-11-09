// SPDX-License-Identifier: GPL-3.0-or-later
//! Text contexts for feedback, the assistant and plugins.

use super::*;

impl Session {
    /// Context for a feedback report: app, system, photo and the edit.
    pub fn feedback_context(&self, include_settings: bool) -> String {
        let info = self.info();
        let state = self.state_info();
        let mut out = format!(
            "- Ambrotype {} on {} ({})\n- Camera: {}\n- Lens: {}\n- Size: {} x {}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            info.camera,
            info.lens.as_deref().unwrap_or("unknown"),
            info.width,
            info.height
        );
        if !state.not_rendered.is_empty() {
            out.push_str(&format!(
                "- Not rendered yet: {}\n",
                state.not_rendered.join(", ")
            ));
        }
        if include_settings {
            let settings: Vec<String> = self
                .state
                .settings()
                .iter()
                .map(|(k, v)| {
                    let text = serde_json::to_string(v).unwrap_or_default();
                    format!("{k}={}", text.trim_matches('"'))
                })
                .collect();
            out.push_str(&format!("- Settings: {}\n", settings.join("; ")));
        }
        out
    }

    /// What the assistant is told about the photo: what it is, its settings
    /// that differ from the defaults, and its numbers as edited now.
    pub fn assistant_context(&mut self, lens: &LensSources) -> String {
        let info = self.info();
        let settings = self.state.settings().clone();
        let mut set = Vec::new();
        let mut other = Vec::new();
        // Bookkeeping that says nothing about the look.
        let skip = [
            "Version",
            "ProcessVersion",
            "CompatibleVersion",
            "HasSettings",
            "AlreadyApplied",
        ];
        for (name, v) in settings.iter().filter(|(n, _)| !skip.contains(&n.as_str())) {
            match v {
                Value::Seq(_) | Value::Struct(_) => other.push(name.as_str()),
                _ => set.push(format!(
                    "{name}={}",
                    amb_protocol::convert::display(name, Some(v))
                )),
            }
        }
        let small = raw::downscale(&self.full, 512);
        let develop = self.develop(&settings, lens);
        let stats = amb_engine::stats::measure(&pipeline::render(&small, &develop));
        let mut out = format!(
            "{} ({}{}), {} x {}; as shot {:.0} K, tint {:+.0}
",
            info.name,
            info.camera,
            if info.rendered {
                ", a JPEG/TIFF, not a raw"
            } else {
                ""
            },
            info.width,
            info.height,
            info.as_shot_temperature,
            info.as_shot_tint,
        );
        out.push_str(&if set.is_empty() {
            "Settings: all at defaults\n".to_string()
        } else {
            format!("Settings: {}\n", set.join(" "))
        });
        if !other.is_empty() {
            out.push_str(&format!("Also has: {}\n", other.join(", ")));
        }
        out.push_str(&stats.to_brief());
        out
    }

    /// What a plugin with these permissions may see of this photo.
    pub fn plugin_context(
        &self,
        permissions: &[amb_plugin::Permission],
        seed: u64,
    ) -> amb_plugin::Context {
        use amb_plugin::Permission;
        let settings = self.state.settings();
        amb_plugin::Context {
            settings: amb_plugin::Context::settings_json(settings),
            photo: permissions
                .contains(&Permission::PhotoInfo)
                .then(|| amb_plugin::photo::info(&self.full)),
            stats: permissions
                .contains(&Permission::PhotoStats)
                .then(|| amb_plugin::photo::stats(&self.full, settings)),
            seed,
        }
    }
}

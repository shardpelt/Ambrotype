// SPDX-License-Identifier: GPL-3.0-or-later
//! Checking layouts and themes: problems, colour contrast, completing a theme.

use super::*;

/// What a layout can refer to, from the editor schema.
pub(super) struct Catalog {
    pub(super) panels: Vec<schema::Panel>,
    pub(super) kinds: HashMap<String, &'static str>,
}

pub(super) fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let panels = schema::panels();
        let kinds = panels
            .iter()
            .flat_map(|p| p.sections.iter())
            .flat_map(|s| s.controls.iter())
            .map(|c| (c.name.clone(), c.kind))
            .collect();
        Catalog { panels, kinds }
    })
}

/// Sections of the Color Grading panel that the colour wheels draw.
pub fn is_wheel_section(panel: &str, section: &str) -> bool {
    panel == "color_grading" && ["Shadow", "Midtone", "Highlight", "Global"].contains(&section)
}

/// The composite widget a whole schema panel brings with it.
fn panel_composite(panel: &str) -> Option<&'static str> {
    match panel {
        "tone_curve" => Some("point-curve"),
        "color_grading" => Some("color-wheels"),
        _ => None,
    }
}

#[derive(Default)]
pub(super) struct Checker {
    problems: Vec<Problem>,
    pub(super) controls: HashSet<String>,
    pub(super) composites: HashSet<&'static str>,
    pub(super) blocks: HashSet<String>,
}

impl Checker {
    fn error(&mut self, message: impl Into<String>) {
        self.problems.push(Problem {
            error: true,
            message: message.into(),
        });
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.problems.push(Problem {
            error: false,
            message: message.into(),
        });
    }

    fn known_keys(&mut self, obj: &Map<String, Value>, keys: &[&str], at: &str) {
        for k in obj.keys() {
            if !keys.contains(&k.as_str()) {
                self.warn(format!("{at}: unknown key \"{k}\" is ignored"));
            }
        }
    }

    fn header(&mut self, obj: &Map<String, Value>, format: &str) {
        if obj.get("format").and_then(Value::as_str) != Some(format) {
            self.error(format!("\"format\" must be \"{format}\""));
        }
        match obj.get("name").and_then(Value::as_str) {
            Some(n) if !n.trim().is_empty() => {}
            _ => self.error("\"name\" must be a non-empty string"),
        }
        for key in ["description", "author", "based_on", "license"] {
            if obj.get(key).is_some_and(|v| !v.is_string() && !v.is_null()) {
                self.error(format!("\"{key}\" must be a string"));
            }
        }
        // Where an installed look came from (the hub fills this in).
        if obj.get("source").is_some_and(|v| !v.is_object()) {
            self.error("\"source\" must be an object");
        }
    }

    /// A control shown once; `widget` checked against its kind.
    fn control(&mut self, name: &str, widget: Option<&Value>, at: &str) {
        let Some(kind) = catalog().kinds.get(name).copied() else {
            self.warn(format!(
                "{at}: unknown setting \"{name}\" (shown as a placeholder)"
            ));
            return;
        };
        if !self.controls.insert(name.to_string()) {
            self.error(format!(
                "{at}: \"{name}\" appears more than once; each setting can be shown once"
            ));
        }
        self.widget(kind, name, widget, at);
    }

    fn widget(&mut self, kind: &str, name: &str, widget: Option<&Value>, at: &str) {
        let Some(w) = widget else { return };
        let Some(w) = w.as_str() else {
            self.error(format!("{at}: the widget for \"{name}\" must be a string"));
            return;
        };
        if widgets_for(kind).contains(&w) {
            return;
        }
        if all_widgets().any(|x| x == w) {
            self.error(format!(
                "{at}: \"{name}\" is a {kind} setting and can't use the \"{w}\" widget (use one of: {})",
                widgets_for(kind).join(", ")
            ));
        } else {
            self.warn(format!(
                "{at}: unknown widget \"{w}\" for \"{name}\" (the default is used)"
            ));
        }
    }

    fn composite(&mut self, name: &'static str, at: &str) {
        if !self.composites.insert(name) {
            self.error(format!("{at}: the {name} widget appears more than once"));
        }
    }

    /// Every control of a schema panel (or one section), minus `hide`.
    fn schema_controls(
        &mut self,
        panel: &schema::Panel,
        section: Option<&str>,
        widgets: Option<&Map<String, Value>>,
        hide: &[&str],
        at: &str,
    ) {
        let whole = section.is_none();
        if whole {
            if let Some(c) = panel_composite(panel.id) {
                self.composite(c, at);
            }
        }
        for s in &panel.sections {
            if section.is_some_and(|x| x != s.label)
                || (whole && is_wheel_section(panel.id, &s.label))
            {
                continue;
            }
            for c in &s.controls {
                if !hide.contains(&c.name.as_str()) {
                    self.control(&c.name, widgets.and_then(|w| w.get(&c.name)), at);
                }
            }
        }
    }

    pub(super) fn column(&mut self, side: &str, v: &Value) {
        let Some(obj) = v.as_object() else {
            self.error(format!(
                "\"{side}\" must be an object with \"width\" and \"items\""
            ));
            return;
        };
        self.known_keys(obj, &["width", "items"], side);
        match obj.get("width") {
            None => {}
            Some(w) if w.as_f64().is_some_and(|w| (0.0..=MAX_WIDTH).contains(&w)) => {}
            Some(_) => self.error(format!(
                "{side}.width must be a number from 0 to {MAX_WIDTH}"
            )),
        }
        let Some(items) = obj.get("items") else {
            return;
        };
        let Some(items) = items.as_array() else {
            self.error(format!("{side}.items must be a list"));
            return;
        };
        let mut tabs = HashSet::new();
        for (i, item) in items.iter().enumerate() {
            let at = format!("{side}.items[{i}]");
            match item {
                // A tab: what follows it shows while it's chosen.
                Value::Object(t) if t.contains_key("tab") => {
                    self.known_keys(t, &["tab"], &at);
                    match t["tab"].as_str().map(str::trim) {
                        Some(name) if !name.is_empty() && name.chars().count() <= 24 => {
                            if !tabs.insert(name.to_string()) {
                                self.error(format!("{at}: tab \"{name}\" appears twice"));
                            }
                        }
                        _ => self.error(format!(
                            "{at}: \"tab\" must be a name of 1 to 24 characters"
                        )),
                    }
                }
                Value::String(b) if BLOCKS.contains(&b.as_str()) => {
                    if !self.blocks.insert(b.clone()) {
                        self.error(format!("{at}: \"{b}\" appears more than once"));
                    }
                }
                Value::String(b) => self.warn(format!(
                    "{at}: unknown block \"{b}\" is ignored (blocks: {})",
                    BLOCKS.join(", ")
                )),
                Value::Object(p) => self.panel(p, &at),
                _ => self.error(format!("{at}: must be a block name or a panel object")),
            }
        }
    }

    fn panel(&mut self, p: &Map<String, Value>, at: &str) {
        if p.get("title").is_some_and(|t| !t.is_string()) {
            self.error(format!("{at}: \"title\" must be a string"));
        }
        if p.get("open").is_some_and(|o| !o.is_boolean()) {
            self.error(format!("{at}: \"open\" must be true or false"));
        }
        if let Some(id) = p.get("panel") {
            self.known_keys(p, &["panel", "title", "open", "widgets", "hide"], at);
            let Some(id) = id.as_str() else {
                self.error(format!("{at}: \"panel\" must be a panel id"));
                return;
            };
            let widgets = match p.get("widgets") {
                None => None,
                Some(Value::Object(w)) => Some(w),
                Some(_) => {
                    self.error(format!(
                        "{at}: \"widgets\" must map setting names to widgets"
                    ));
                    None
                }
            };
            let hide: Vec<&str> = match p.get("hide") {
                None => Vec::new(),
                Some(Value::Array(h)) if h.iter().all(Value::is_string) => {
                    h.iter().filter_map(Value::as_str).collect()
                }
                Some(_) => {
                    self.error(format!("{at}: \"hide\" must be a list of setting names"));
                    Vec::new()
                }
            };
            let Some(panel) = catalog().panels.iter().find(|x| x.id == id) else {
                self.warn(format!("{at}: unknown panel \"{id}\" is ignored"));
                return;
            };
            let in_panel = |n: &str| {
                panel
                    .sections
                    .iter()
                    .any(|s| s.controls.iter().any(|c| c.name == n))
            };
            let named: Vec<&str> = widgets
                .into_iter()
                .flat_map(|w| w.keys().map(String::as_str))
                .chain(hide.iter().copied())
                .collect();
            for n in named {
                if !in_panel(n) {
                    self.warn(format!("{at}: \"{n}\" is not in the {id} panel"));
                }
            }
            self.schema_controls(panel, None, widgets, &hide, at);
            return;
        }
        self.known_keys(p, &["title", "open", "columns", "items"], at);
        if !p.get("title").is_some_and(Value::is_string) {
            self.error(format!(
                "{at}: a panel needs \"panel\" (a built-in panel) or a \"title\""
            ));
        }
        if let Some(c) = p.get("columns") {
            if !c.as_u64().is_some_and(|c| (1..=4).contains(&c)) {
                self.error(format!("{at}: \"columns\" must be 1 to 4"));
            }
        }
        let Some(items) = p.get("items").and_then(Value::as_array) else {
            self.error(format!("{at}: \"items\" must be a list"));
            return;
        };
        for (i, entry) in items.iter().enumerate() {
            let at = format!("{at}.items[{i}]");
            let Some(e) = entry.as_object() else {
                self.error(format!("{at}: must be an object"));
                continue;
            };
            if let Some(name) = e.get("setting") {
                self.known_keys(e, &["setting", "widget", "label"], &at);
                match name.as_str() {
                    Some(name) => self.control(name, e.get("widget"), &at),
                    None => self.error(format!("{at}: \"setting\" must be a setting name")),
                }
                if e.get("label").is_some_and(|l| !l.is_string()) {
                    self.error(format!("{at}: \"label\" must be a string"));
                }
            } else if let Some(include) = e.get("include") {
                self.known_keys(e, &["include"], &at);
                let Some(include) = include.as_str() else {
                    self.error(format!(
                        "{at}: \"include\" must be \"panel\" or \"panel/Section\""
                    ));
                    continue;
                };
                let (id, section) = match include.split_once('/') {
                    Some((p, s)) => (p, Some(s)),
                    None => (include, None),
                };
                match catalog().panels.iter().find(|x| x.id == id) {
                    None => self.warn(format!("{at}: unknown panel \"{id}\" is ignored")),
                    Some(panel) => {
                        if section.is_some_and(|s| !panel.sections.iter().any(|x| x.label == s)) {
                            self.warn(format!(
                                "{at}: the {id} panel has no section \"{}\"",
                                section.unwrap_or_default()
                            ));
                        } else {
                            self.schema_controls(panel, section, None, &[], &at);
                        }
                    }
                }
            } else if let Some(w) = e.get("widget") {
                self.known_keys(e, &["widget"], &at);
                match COMPOSITES.iter().find(|c| Some(**c) == w.as_str()) {
                    Some(c) => self.composite(c, &at),
                    None => self.warn(format!(
                        "{at}: unknown widget {w} is ignored (widgets without a setting: {})",
                        COMPOSITES.join(", ")
                    )),
                }
            } else if let Some(l) = e.get("label") {
                self.known_keys(e, &["label"], &at);
                if !l.is_string() {
                    self.error(format!("{at}: \"label\" must be a string"));
                }
            } else {
                self.error(format!(
                    "{at}: needs one of \"setting\", \"include\", \"widget\" or \"label\""
                ));
            }
        }
    }
}

/// Problems with a layout (empty when it's fine).
pub fn check_layout(v: &Value) -> Vec<Problem> {
    let mut c = Checker::default();
    let Some(obj) = v.as_object() else {
        c.error("a layout must be a JSON object");
        return c.problems;
    };
    c.header(obj, LAYOUT_FORMAT);
    c.known_keys(
        obj,
        &[
            "format",
            "name",
            "description",
            "author",
            "based_on",
            "license",
            "source",
            "theme",
            "density",
            "toolbar",
            "toolbar_style",
            "filmstrip",
            "chrome",
            "left",
            "right",
        ],
        "layout",
    );
    if let Some(t) = obj.get("toolbar_style") {
        if !matches!(t.as_str(), Some("icons" | "words")) {
            c.warn(format!(
                "unknown toolbar_style {t} (\"icons\" or \"words\"); \"icons\" is used"
            ));
        }
    }
    if let Some(t) = obj.get("chrome") {
        if !matches!(t.as_str(), Some("normal" | "hidden")) {
            c.warn(format!(
                "unknown chrome {t} (\"normal\" or \"hidden\"); \"normal\" is used"
            ));
        }
    }
    if obj.get("filmstrip").is_some_and(|f| !f.is_boolean()) {
        c.error("\"filmstrip\" must be true or false");
    }
    if obj.get("theme").is_some_and(|t| !t.is_string()) {
        c.error("\"theme\" must be a theme id such as \"builtin:dark\"");
    }
    if let Some(d) = obj.get("density") {
        if !d.as_str().is_some_and(|d| DENSITIES.contains(&d)) {
            c.warn(format!(
                "unknown density {d} (one of: {}); \"normal\" is used",
                DENSITIES.join(", ")
            ));
        }
    }
    if let Some(t) = obj.get("toolbar") {
        match t.as_array() {
            None => c.error("\"toolbar\" must be a list of button names"),
            Some(items) => {
                let mut seen = HashSet::new();
                for b in items {
                    match b.as_str() {
                        Some("|") => {}
                        Some(b) if TOOLBAR.contains(&b) => {
                            if !seen.insert(b) {
                                c.error(format!("toolbar: \"{b}\" appears more than once"));
                            }
                        }
                        Some(b) => c.warn(format!(
                            "toolbar: unknown button \"{b}\" is ignored (buttons: {})",
                            TOOLBAR.join(", ")
                        )),
                        None => c.error("toolbar: entries must be strings"),
                    }
                }
            }
        }
    }
    for side in ["left", "right"] {
        if let Some(col) = obj.get(side) {
            c.column(side, col);
        }
    }
    c.problems
}

pub(super) fn is_hex_colour(s: &str) -> bool {
    s.strip_prefix('#').is_some_and(|h| {
        [3, 4, 6, 8].contains(&h.len()) && h.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Problems with a theme (empty when it's fine). Themes hold only values for
/// known tokens, never CSS, so a shared theme can't change anything else.
pub fn check_theme(v: &Value) -> Vec<Problem> {
    let mut c = Checker::default();
    let Some(obj) = v.as_object() else {
        c.error("a theme must be a JSON object");
        return c.problems;
    };
    c.header(obj, THEME_FORMAT);
    c.known_keys(
        obj,
        &[
            "format",
            "name",
            "description",
            "author",
            "based_on",
            "license",
            "source",
            "scheme",
            "tokens",
        ],
        "theme",
    );
    if let Some(s) = obj.get("scheme") {
        if !matches!(s.as_str(), Some("dark" | "light")) {
            c.error("\"scheme\" must be \"dark\" or \"light\"");
        }
    }
    let Some(tokens) = obj.get("tokens").and_then(Value::as_object) else {
        c.error("\"tokens\" must be an object of token: value");
        return c.problems;
    };
    for (k, v) in tokens {
        if COLOR_TOKENS.contains(&k.as_str()) {
            if !v.as_str().is_some_and(is_hex_colour) {
                c.error(format!(
                    "token \"{k}\" must be a hex colour such as \"#1b1c1f\""
                ));
            }
        } else if let Some((_, lo, hi)) = NUMBER_TOKENS.iter().find(|(n, ..)| n == k) {
            if !v.as_f64().is_some_and(|n| (*lo..=*hi).contains(&n)) {
                c.error(format!("token \"{k}\" must be a number from {lo} to {hi}"));
            }
        } else {
            c.warn(format!("unknown token \"{k}\" is ignored"));
        }
    }
    c.problems
}

/// Parse `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` into 0–255 RGB (alpha is dropped).
pub(super) fn parse_hex(s: &str) -> Option<[f64; 3]> {
    let h = s.strip_prefix('#').filter(|_| is_hex_colour(s))?;
    let digit = |i: usize, n: usize| u8::from_str_radix(&h[i * n..i * n + n], 16).ok();
    let mut rgb = [0.0; 3];
    for (i, c) in rgb.iter_mut().enumerate() {
        *c = if h.len() <= 4 {
            f64::from(digit(i, 1)? * 17)
        } else {
            f64::from(digit(i, 2)?)
        };
    }
    Some(rgb)
}

fn hex(c: [f64; 3]) -> String {
    let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Relative luminance (WCAG), 0–1.
fn luminance(c: [f64; 3]) -> f64 {
    let lin = |v: f64| {
        let v = v / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Fill in the tokens a theme leaves out, from its `bg`, `text` and `accent`
/// (each taken from Dark, or Light for a light theme, when missing too), so
/// three colours make a whole theme. Steps between background and text were
/// matched to the Dark and Light themes. `scheme` follows the background
/// when it's missing. Tokens the theme sets are kept.
pub fn complete_theme(data: &mut Value) {
    let Some(obj) = data.as_object_mut() else {
        return;
    };
    let scheme = obj
        .get("scheme")
        .and_then(Value::as_str)
        .map(str::to_string);
    let tokens = obj
        .entry("tokens")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(tokens) = tokens.as_object_mut() else {
        return;
    };
    let get = |k: &str| tokens.get(k).and_then(Value::as_str).and_then(parse_hex);
    let light = match scheme.as_deref() {
        Some(s) => s == "light",
        None => get("bg").is_some_and(|c| luminance(c) > 0.4),
    };
    let (bg, text, accent) = if light {
        (
            [236.0, 238.0, 241.0],
            [29.0, 31.0, 35.0],
            [184.0, 116.0, 26.0],
        )
    } else {
        (
            [27.0, 28.0, 31.0],
            [230.0, 230.0, 232.0],
            [232.0, 176.0, 92.0],
        )
    };
    let bg = get("bg").unwrap_or(bg);
    let text = get("text").unwrap_or(text);
    let accent = get("accent").unwrap_or(accent);
    let toward = |t: f64| hex(mix(bg, text, t));
    let white = [255.0; 3];
    let derived: [(&str, String); 16] = if light {
        [
            ("bg", hex(bg)),
            ("bg-deep", toward(0.05)),
            ("viewer", toward(0.115)),
            ("panel", hex(mix(bg, white, 0.55))),
            ("panel-2", hex(mix(bg, white, 1.0))),
            ("line", toward(0.17)),
            ("hover", hex(mix(bg, white, 0.15))),
            ("tab-active", toward(0.07)),
            ("text", hex(text)),
            ("muted", toward(0.69)),
            ("faint", toward(0.47)),
            ("accent", hex(accent)),
            ("accent-ink", String::new()),
            ("danger", "#c4402f".into()),
            ("thumb", toward(0.78)),
            ("track", toward(0.26)),
        ]
    } else {
        [
            ("bg", hex(bg)),
            ("bg-deep", hex(mix(bg, [0.0; 3], 0.35))),
            ("viewer", hex(mix(bg, [0.0; 3], 0.35))),
            ("panel", toward(0.04)),
            ("panel-2", toward(0.075)),
            ("line", toward(0.12)),
            ("hover", toward(0.12)),
            ("tab-active", toward(0.15)),
            ("text", hex(text)),
            ("muted", toward(0.62)),
            ("faint", toward(0.40)),
            ("accent", hex(accent)),
            ("accent-ink", String::new()),
            ("danger", "#e5675a".into()),
            ("thumb", toward(0.93)),
            ("track", toward(0.23)),
        ]
    };
    for (k, v) in derived {
        if tokens.contains_key(k) {
            continue;
        }
        let v = if k == "accent-ink" {
            // Whichever of background and text reads better on the accent.
            let ink = if contrast(accent, bg) >= contrast(accent, text) {
                bg
            } else {
                text
            };
            hex(ink)
        } else {
            v
        };
        tokens.insert(k.into(), Value::String(v));
    }
    tokens.entry("radius").or_insert(Value::from(6));
    obj.entry("scheme")
        .or_insert_with(|| Value::String(if light { "light" } else { "dark" }.into()));
}

pub fn check(kind: Kind, v: &Value) -> Vec<Problem> {
    match kind {
        Kind::Layout => check_layout(v),
        Kind::Theme => check_theme(v),
    }
}

/// `ambrotype --check-look FILE...`: print each file's problems; false if any has errors.
pub fn check_files(paths: &[String]) -> bool {
    let mut ok = true;
    for path in paths {
        let result = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                serde_json::from_str::<Value>(&t).map_err(|e| format!("not valid JSON: {e}"))
            })
            .and_then(|data| {
                Kind::of(&data)
                    .map(|k| (k, check(k, &data)))
                    .ok_or_else(|| {
                        format!("\"format\" must be \"{LAYOUT_FORMAT}\" or \"{THEME_FORMAT}\"")
                    })
            });
        match result {
            Err(e) => {
                ok = false;
                println!("{path}: error: {e}");
            }
            Ok((kind, problems)) => {
                let kind = kind.folder().trim_end_matches('s');
                if problems.is_empty() {
                    println!("{path}: ok ({kind})");
                }
                for p in problems {
                    ok &= !p.error;
                    let level = if p.error { "error" } else { "warning" };
                    println!("{path}: {level}: {}", p.message);
                }
            }
        }
    }
    ok
}

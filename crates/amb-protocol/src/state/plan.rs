// SPDX-License-Identifier: GPL-3.0-or-later
//! Planning a command: what it would change, before any of it is applied.

use super::*;

impl DevelopState {
    pub(super) fn plan(
        &self,
        cmd: &Command,
        next: &mut BTreeMap<String, Value>,
        clamped: &mut Vec<Clamp>,
        notes: &mut Vec<String>,
    ) -> Result<(), Error> {
        match cmd {
            Command::SetParam { name, value } => {
                let info = known(name)?;
                let v = convert::value_from_command(&info, value, clamped)?;
                next.insert(name.clone(), v);
                after_set(name, next, notes)?;
            }
            Command::AdjustParam { name, delta } => {
                let info = known(name)?;
                if info.kind != "number" {
                    return Err(Error::WrongKind {
                        name: name.clone(),
                        expected: "a numeric setting for adjust_param".into(),
                        got: format!("a {} setting", info.kind),
                    });
                }
                if !delta.is_finite() {
                    return Err(Error::InvalidValue {
                        name: name.clone(),
                        reason: "delta must be a finite number".into(),
                    });
                }
                // A non-numeric or non-finite stored value ("NaN") counts as unset.
                let base = match self.settings.get(name).and_then(convert::finite) {
                    Some(v) => v,
                    None => Registry::builtin()
                        .numeric_default(name)
                        .ok_or_else(|| Error::NoBase { name: name.clone() })?,
                };
                let sum = base + delta;
                if !sum.is_finite() {
                    return Err(Error::InvalidValue {
                        name: name.clone(),
                        reason: format!("{base} + {delta} is not a finite number"),
                    });
                }
                let v = convert::clamp_number(&info, sum, clamped);
                next.insert(name.clone(), Value::Number(v));
                after_set(name, next, notes)?;
            }
            Command::ResetParam { name } => {
                if !self.settings.contains_key(name) {
                    known(name)?;
                }
                next.remove(name);
            }
            Command::ResetGroup { group } => {
                known_group(group)?;
                next.retain(|name, _| group_of(name).as_deref() != Some(group.as_str()));
            }
            Command::ApplySnapshot { name } => {
                *next = self.snapshot(name)?.settings.clone();
            }
            Command::ApplyPreset {
                path,
                amount,
                include_groups,
                exclude_groups,
            } => {
                if !amount.is_finite() || !(0.0..=200.0).contains(amount) {
                    return Err(Error::InvalidValue {
                        name: "amount".into(),
                        reason: format!("{amount} is outside 0-200 (percent)"),
                    });
                }
                for g in include_groups.iter().chain(exclude_groups).flatten() {
                    known_group(g)?;
                }
                let preset = Preset::from_path(Path::new(path))
                    .map_err(|e| Error::Load(format!("{path}: {e}")))?;
                if *amount != 100.0 && !preset.supports_amount() {
                    notes.push(format!(
                        "preset does not declare SupportsAmount; applied at {amount}% anyway"
                    ));
                }
                let mut filtered = BTreeMap::new();
                let mut unknown = Vec::new();
                let mut rejected = Vec::new();
                for (name, value) in &preset.settings {
                    let group = group_of(name);
                    let keep = match group.as_deref() {
                        Some(g) if PRESET_SKIPPED_GROUPS.contains(&g) => false,
                        Some("process") => PRESET_PROCESS_TAGS.contains(&name.as_str()),
                        _ => true,
                    };
                    let included = match (include_groups, group.as_deref()) {
                        (Some(inc), Some(g)) => inc.iter().any(|i| i == g),
                        (Some(_), None) => false,
                        (None, _) => true,
                    };
                    let excluded = exclude_groups.as_ref().is_some_and(|ex| {
                        group.as_deref().is_some_and(|g| ex.iter().any(|e| e == g))
                    });
                    if !(keep && included && !excluded) {
                        continue;
                    }
                    let value = convert::normalise(name, value.clone());
                    match Registry::builtin().lookup(name) {
                        None => {
                            unknown.push(name.clone());
                            filtered.insert(name.clone(), value);
                        }
                        // Same kind checks and range clamping as set_param.
                        Some(info) => match preset_value(&info, value, clamped) {
                            Ok(v) => {
                                filtered.insert(name.clone(), v);
                            }
                            Err(e) => rejected.push(e.to_string()),
                        },
                    }
                }
                if !unknown.is_empty() {
                    notes.push(format!(
                        "copied {} setting(s) unknown to the registry without validation: {}",
                        unknown.len(),
                        unknown.join(", ")
                    ));
                }
                // Masks join the photo's own, as in Lightroom.
                if let Some(incoming) = filtered.remove(amb_presets::masks::MASK_TAG) {
                    masks::merge(next, &incoming, amount / 100.0);
                }
                let unscaled =
                    apply_amount(next, &filtered, amount / 100.0, clamped, &mut rejected);
                if !rejected.is_empty() {
                    notes.push(format!(
                        "skipped {} invalid preset value(s): {}",
                        rejected.len(),
                        rejected.join("; ")
                    ));
                }
                if !unscaled.is_empty() {
                    notes.push(format!(
                        "copied {} unscaled at {amount}%: no current value or fixed default to \
                         scale from (white balance depends on the image)",
                        unscaled.join(", ")
                    ));
                }
            }
            Command::AddMask {
                kind,
                mask,
                mode,
                values,
                adjust,
            } => {
                masks::add(next, kind, *mask, mode.as_deref(), values.as_ref(), clamped)?;
                if let Some(adjust) = adjust {
                    let Some(new) = mask.is_none().then(|| masks::count(next) - 1) else {
                        return Err(Error::InvalidValue {
                            name: "adjust".into(),
                            reason: "is for a new mask; change an existing one with set_mask"
                                .into(),
                        });
                    };
                    masks::set(next, new, None, adjust, clamped)?;
                }
            }
            Command::SetMask { mask, part, values } => {
                masks::set(next, *mask, *part, values, clamped)?;
            }
            Command::PaintMask {
                mask,
                part,
                mode,
                points,
                size,
                feather,
                flow,
                density,
                erase,
                dabs,
            } => {
                let stroke = masks::Stroke {
                    dabs: dabs.as_deref(),
                    points,
                    size: *size,
                    feather: *feather,
                    flow: *flow,
                    density: *density,
                    erase: *erase,
                };
                masks::paint(next, *mask, *part, mode.as_deref(), &stroke, clamped)?;
            }
            Command::RemoveMask { mask, part } => masks::remove(next, *mask, *part)?,
            Command::DuplicateMask { mask } => masks::duplicate(next, *mask)?,
            Command::MoveMask {
                mask,
                part: None,
                to,
            } => masks::move_to(next, *mask, *to)?,
            Command::MoveMask {
                mask,
                part: Some(part),
                to,
            } => masks::move_part(next, *mask, *part, *to)?,
            Command::AddSpot {
                at,
                radius,
                kind,
                source,
                source_auto,
                feather,
                opacity,
                points,
                separate,
            } => {
                let spot = spots::NewSpot {
                    points: points.as_deref(),
                    separate: *separate,
                    kind: kind.as_deref(),
                    at: *at,
                    radius: *radius,
                    source: *source,
                    source_auto: *source_auto,
                    feather: *feather,
                    opacity: *opacity,
                };
                spots::add(next, &spot, clamped)?;
            }
            Command::SetSpot { spot, values } => spots::set(next, *spot, values, clamped)?,
            Command::RemoveSpot { spot } => spots::remove(next, *spot)?,
            Command::Undo
            | Command::Redo
            | Command::SaveSnapshot { .. }
            | Command::RenameSnapshot { .. }
            | Command::DeleteSnapshot { .. } => unreachable!("handled by execute"),
        }
        Ok(())
    }
}

/// Validate a (normalised) preset value for a registry tag the way set_param
/// validates a command value: scalars (numbers, switches, enums) go through
/// [`convert::value_from_command`], so wrong kinds and out-of-set enum values
/// are refused and numbers are clamped to the range (and reported). Curves,
/// sequences, structs and text are taken as they are, as before.
fn preset_value(info: &TagInfo, value: Value, clamped: &mut Vec<Clamp>) -> Result<Value, Error> {
    if !matches!(info.kind.as_str(), "number" | "bool" | "enum") {
        return Ok(value);
    }
    // A non-finite number serialises as null and is refused like any wrong kind.
    let json = serde_json::to_value(&value).unwrap_or_default();
    convert::value_from_command(info, &json, clamped)
}

/// Apply validated preset values on top of `next` at `fraction` (1.0 = 100%).
///
/// Numeric settings move linearly from the current value (or the registry
/// default) towards the preset's value, may extrapolate past it above 100%,
/// and are clamped to their range with the clamping reported. Numeric settings
/// with neither a current value nor a fixed default (Temperature, Tint) are
/// copied unscaled; their names are returned. Everything else is copied from
/// the preset whenever `fraction > 0`.
fn apply_amount(
    next: &mut BTreeMap<String, Value>,
    preset: &BTreeMap<String, Value>,
    fraction: f64,
    clamped: &mut Vec<Clamp>,
    rejected: &mut Vec<String>,
) -> Vec<String> {
    let mut unscaled = Vec::new();
    if fraction <= 0.0 {
        return unscaled;
    }
    let registry = Registry::builtin();
    for (name, target) in preset {
        let info = registry.lookup(name).filter(|i| i.kind == "number");
        let (Some(info), Some(to)) = (info, convert::finite(target)) else {
            next.insert(name.clone(), target.clone());
            continue;
        };
        let from = next
            .get(name)
            .and_then(convert::finite)
            .or_else(|| registry.numeric_default(name));
        let Some(from) = from else {
            if fraction != 1.0 {
                unscaled.push(name.clone());
            }
            next.insert(name.clone(), target.clone());
            continue;
        };
        let raw = from + (to - from) * fraction;
        if !raw.is_finite() {
            rejected.push(format!("{name}: scaling to {fraction}x overflows"));
            continue;
        }
        let mut v = convert::clamp_number(&info, raw, clamped);
        // Interpolation leaves float noise (1.0 + (0.2 - 1.0) * 1.0 != 0.2);
        // snap to the preset's own value when that is meant.
        if (v - to).abs() < 1e-9 {
            v = to;
        }
        next.insert(name.clone(), Value::Number(v));
    }
    unscaled
}

/// Side effects Lightroom applies when a slider moves, plus guidance notes.
///
/// Moving Temperature or Tint under As Shot/Auto switches WhiteBalance to
/// Custom. Lightroom keeps the as-shot value of the other slider, but that
/// value lives in the raw file, not in the settings, so this layer cannot fill
/// it in: a Custom white balance without Temperature is not rendered at all,
/// and one without Tint renders with tint 0. Tint alone is therefore refused,
/// and Temperature alone carries a warning.
fn after_set(
    name: &str,
    next: &mut BTreeMap<String, Value>,
    notes: &mut Vec<String>,
) -> Result<(), Error> {
    // Relative white balance (rendered images) has a neutral 0, so moving
    // either slider simply switches to Custom.
    if matches!(name, "IncrementalTemperature" | "IncrementalTint")
        && matches!(
            next.get("WhiteBalance").and_then(Value::as_str),
            None | Some("As Shot") | Some("Auto")
        )
    {
        next.insert("WhiteBalance".into(), Value::Text("Custom".into()));
    }
    if matches!(name, "Temperature" | "Tint") {
        let mode = next.get("WhiteBalance").and_then(Value::as_str);
        if matches!(mode, None | Some("As Shot") | Some("Auto")) {
            let has = |n: &str| next.get(n).and_then(convert::finite).is_some();
            let (has_temperature, has_tint) = (has("Temperature"), has("Tint"));
            if !has_temperature {
                return Err(Error::InvalidValue {
                    name: name.to_string(),
                    reason: format!(
                        "white balance is {}, so Tint needs a Temperature to switch to Custom; \
                         set Temperature first (in the same batch, before Tint), e.g. the as-shot \
                         temperature that `amb render` prints",
                        mode.unwrap_or("As Shot")
                    ),
                });
            }
            next.insert("WhiteBalance".into(), Value::Text("Custom".into()));
            notes.push(format!(
                "WhiteBalance set to \"Custom\" so that {name} takes effect (as Lightroom does when the slider moves)"
            ));
            if !has_tint {
                notes.push(
                    "Tint is not set, so the photo keeps its as-shot tint; set Tint to change it \
                     (`amb render` prints the as-shot values, e.g. `as shot: 5150 K, tint +4`)"
                        .into(),
                );
            }
        }
    }
    if group_of(name).as_deref() == Some("tone_legacy") {
        let modern = match name {
            "Exposure" => "Exposure2012",
            "Contrast" => "Contrast2012",
            "Shadows" => "Blacks2012 (legacy Shadows is the old Blacks slider)",
            "HighlightRecovery" => "Highlights2012",
            "FillLight" => "Shadows2012",
            "Clarity" => "Clarity2012",
            _ => "Exposure2012 and the tone curve",
        };
        notes.push(format!(
            "{name} is a legacy PV2010/PV2003 control; current process versions use {modern}"
        ));
    }
    Ok(())
}

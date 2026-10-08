//! Auto tone: Exposure, Whites and Blacks from the histogram.
//!
//! The statistics are of the photo as it is edited now, so a second run
//! starts from the first one's result and only fine-tunes. It moves the
//! midtones part of the way to a target and keeps the brightest tones from
//! clipping, so a dark, low-key photo stays low-key.

use amb_plugin_sdk::{Input, Output};

/// sRGB level (0–1) to linear light.
fn linear(v: f64) -> f64 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to an sRGB level (0–1).
fn encode(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn signed(v: f64, decimals: usize) -> String {
    let text = format!("{:+.*}", decimals, v);
    text.replace('-', "−")
}

pub fn run(input: Input) -> Result<Output, String> {
    let stats = input
        .stats
        .as_ref()
        .ok_or("needs to see the photo's histogram (the photo-stats permission)")?;
    let strength = input.number("strength") / 100.0;
    let target = match input.choice("brightness") {
        "Darker" => 0.38,
        "Brighter" => 0.54,
        _ => 0.46,
    };
    let median = stats.percentile(0.5).clamp(0.01, 0.98);
    let high = stats.percentile(0.995).max(0.01);
    let low = stats.percentile(0.005);
    let mut out = Output::new();
    let mut said = Vec::new();

    // Midtones: part of the way to the target (a dark, low-key photo stays
    // fairly dark), and never so far that the brightest 0.5% clip.
    let mut ev = 0.6 * (linear(target) / linear(median)).log2();
    if ev > 0.0 {
        let headroom = (linear(0.98) / linear(high)).log2().max(0.0);
        // Highlights roll off, so they approach white slowly: stop once they
        // are close rather than creeping up on every run.
        if headroom < ev {
            ev = if headroom < 0.35 { 0.0 } else { headroom };
        }
    }
    let ev = ((ev * strength).clamp(-2.0, 2.0) * 100.0).round() / 100.0;
    if ev.abs() >= 0.05 {
        out = out.adjust("Exposure2012", ev);
        said.push(format!("Exposure {} EV", signed(ev, 2)));
    }

    if input.flag("range") {
        // Where the ends land after that exposure change.
        let gain = 2f64.powf(ev);
        let (high, low) = (encode(linear(high) * gain), encode(linear(low) * gain));
        // Whites: reach near white without clipping much.
        let clipped = if ev <= 0.0 {
            stats.share_above(254.0 / 255.0)
        } else {
            0.0
        };
        let whites = if clipped > 0.01 {
            -(clipped * 400.0).min(40.0)
        } else if high < 0.88 {
            // (Not when they're nearly there: Whites moves them only slowly.)
            ((0.94 - high) * 150.0).min(40.0)
        } else {
            0.0
        };
        let whites = (whites * strength).round();
        if whites.abs() >= 2.0 {
            out = out.adjust("Whites2012", whites);
            said.push(format!("Whites {}", signed(whites, 0)));
        }
        // Blacks: reach near black.
        let blacks = if low > 0.03 {
            -((low - 0.03) * 200.0).min(40.0)
        } else {
            0.0
        };
        let blacks = (blacks * strength).round();
        if blacks.abs() >= 2.0 {
            out = out.adjust("Blacks2012", blacks);
            said.push(format!("Blacks {}", signed(blacks, 0)));
        }
    }

    Ok(if said.is_empty() {
        out.message("The tones are already balanced.")
    } else {
        out.message(said.join(", "))
    })
}

amb_plugin_sdk::export_plugin!(run);

#[cfg(test)]
mod tests {
    use super::*;
    use amb_plugin_sdk::Stats;

    fn input_with(level: u8, spread: u8) -> Input {
        let mut rgb = Vec::new();
        for i in 0..=spread {
            let v = level.saturating_sub(spread / 2).saturating_add(i);
            rgb.extend([v, v, v]);
        }
        let mut input = Input::for_test(
            serde_json::json!({ "strength": 100, "brightness": "Natural", "range": true }),
        );
        input.stats = Some(Stats::from_rgb8(&rgb, rgb.len() as u32 / 3, 1));
        input
    }

    #[test]
    fn a_dark_flat_photo_gets_brighter_and_fuller() {
        let out = run(input_with(40, 40)).unwrap();
        let names: Vec<_> = out
            .commands
            .iter()
            .map(|c| c["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["Exposure2012", "Whites2012", "Blacks2012"]);
        assert!(out.commands[0]["delta"].as_f64().unwrap() > 1.0);
        assert!(out.message.unwrap().starts_with("Exposure +"));
    }

    #[test]
    fn a_balanced_photo_is_left_alone() {
        // Median near 0.46, from black to white.
        let mut input = input_with(117, 0);
        let mut rgb: Vec<u8> = (0..=255u8).flat_map(|v| [v, v, v]).collect();
        rgb.extend((0..60).flat_map(|_| [117, 117, 117]));
        input.stats = Some(Stats::from_rgb8(&rgb, rgb.len() as u32 / 3, 1));
        let out = run(input).unwrap();
        assert!(out.commands.is_empty(), "{:?}", out.commands);
    }

    #[test]
    fn without_statistics_it_says_why() {
        let input = Input::for_test(serde_json::json!({ "strength": 100 }));
        assert!(run(input).unwrap_err().contains("photo-stats"));
    }
}

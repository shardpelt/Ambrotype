// SPDX-License-Identifier: GPL-3.0-or-later
//! Fuzzy lens-name matching, in the spirit of lensfun's `lfFuzzyStrCmp`.
//!
//! Names are normalised into tokens (lowercase, `f/2.8` → `f2.8`, punctuation
//! other than `.`, `-` and `|` dropped, glued prefixes such as Canon's
//! `EF24-70mm` split, `24.0-70.0 mm` → `24-70mm`), maker words are dropped,
//! and a candidate must share the focal-length token. The score is mostly the
//! fraction of the profile's tokens found in the shot's lens name, plus a
//! little of the converse to prefer the most specific profile.

/// Minimum score for a name to count as a match.
pub(crate) const MIN_SCORE: f64 = 0.6;

/// Normalised tokens of a lens (or maker) name.
pub(crate) fn tokens(name: &str) -> Vec<String> {
    let lower = name.to_lowercase().replace("f/", "f");
    let cleaned: String = lower
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '|') {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut out: Vec<String> = Vec::new();
    for word in cleaned.split_whitespace() {
        let word = if word == "|" {
            word
        } else {
            word.trim_matches(|c| c == '.' || c == '-')
        };
        if word.is_empty() {
            continue;
        }
        for piece in split_glued(word) {
            let piece = strip_zero_fractions(&piece);
            // "24 mm" → "24mm", "f 2.8" → "f2.8".
            if let Some(prev) = out.last_mut() {
                let numeric = |s: &str| {
                    s.starts_with(|c: char| c.is_ascii_digit())
                        && s.chars()
                            .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
                };
                if piece == "mm" && numeric(prev) {
                    prev.push_str("mm");
                    continue;
                }
                if prev == "f" && numeric(&piece) {
                    prev.push_str(&piece);
                    continue;
                }
            }
            out.push(piece);
        }
    }
    out
}

/// Split a word where a letter run is glued to a number (`ef-s18-55mm` →
/// `ef-s`, `18-55mm`), except an aperture's `f` (`f2.8`), and split an
/// aperture glued after a focal length (`35mmf1.4` → `35mm`, `f1.4`).
fn split_glued(word: &str) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut pieces = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
        let split = match prev {
            Some(p) if c.is_ascii_digit() && p.is_alphabetic() => cur != "f",
            Some(_) if c == 'f' && next_digit => cur.ends_with("mm") && cur.len() > 2,
            _ => false,
        };
        if split && !cur.is_empty() {
            pieces.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    pieces
}

/// `24.0-70.0mm` → `24-70mm`, `f4.0` → `f4`; other fractions are kept.
fn strip_zero_fractions(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let mut out = String::with_capacity(token.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' && i > 0 && chars[i - 1].is_ascii_digit() {
            let mut j = i + 1;
            while j < chars.len() && chars[j] == '0' {
                j += 1;
            }
            let more_digits = j < chars.len() && chars[j].is_ascii_digit();
            if !more_digits {
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A focal-length token such as `35mm` or `150-600mm`.
pub(crate) fn is_focal(token: &str) -> bool {
    token.strip_suffix("mm").is_some_and(|n| {
        n.starts_with(|c: char| c.is_ascii_digit())
            && n.chars()
                .all(|c| c.is_ascii_digit() || c == '.' || c == '-')
    })
}

/// The numeric part of an aperture token: `f2.8g` → `2.8`, `f5-6.3` → `5-6.3`.
/// Model codes such as Tamron's `f012` are not apertures.
pub(crate) fn aperture_key(token: &str) -> Option<&str> {
    let rest = token.strip_prefix('f')?;
    if !rest.starts_with(|c: char| ('1'..='9').contains(&c)) {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Score how well a profile's name tokens match the shot's lens-name tokens,
/// ignoring maker words. `None` when the focal-length tokens disagree or the
/// profile name is empty after dropping maker words.
pub(crate) fn score(profile: &[String], shot: &[String], makers: &[String]) -> Option<f64> {
    let keep = |t: &&String| !makers.contains(t);
    let mut p: Vec<&String> = profile.iter().filter(keep).collect();
    let mut s: Vec<&String> = shot.iter().filter(keep).collect();
    p.sort();
    p.dedup();
    s.sort();
    s.dedup();
    if p.is_empty() || s.is_empty() {
        return None;
    }
    let p_focal: Vec<&&String> = p.iter().filter(|t| is_focal(t)).collect();
    if !p_focal.is_empty() && !p_focal.iter().all(|t| s.contains(t)) {
        return None;
    }
    let matched = p.iter().filter(|t| s.contains(t)).count() as f64;
    let mut score = 0.75 * matched / p.len() as f64 + 0.25 * matched / s.len() as f64;
    let p_ap: Vec<&str> = p.iter().filter_map(|t| aperture_key(t)).collect();
    let s_ap: Vec<&str> = s.iter().filter_map(|t| aperture_key(t)).collect();
    if !p_ap.is_empty() && !s_ap.is_empty() && !p_ap.iter().any(|a| s_ap.contains(a)) {
        score *= 0.85;
    }
    Some(score)
}

/// Lowercased maker words to ignore when comparing names.
pub(crate) fn maker_words<'a>(makers: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = makers.into_iter().flat_map(tokens).collect();
    out.sort();
    out.dedup();
    out
}

/// Case-insensitive comparison of the first word of two maker names
/// (`NIKON CORPORATION` vs `Nikon`).
pub(crate) fn same_maker(a: &str, b: &str) -> bool {
    let first = |s: &str| s.split_whitespace().next().map(str::to_lowercase);
    match (first(a), first(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Vec<String> {
        tokens(s)
    }

    #[test]
    fn normalises_like_lensfun() {
        assert_eq!(
            t("SP 35mm f/1.8 Di VC USD (F012)"),
            ["sp", "35mm", "f1.8", "di", "vc", "usd", "f012"]
        );
        assert_eq!(
            t("Sigma 150-600mm f/5-6.3 DG OS HSM | C"),
            ["sigma", "150-600mm", "f5-6.3", "dg", "os", "hsm", "|", "c"]
        );
        assert_eq!(t("EF-S18-55mm f/3.5-5.6 IS STM")[..2], ["ef-s", "18-55mm"]);
        assert_eq!(t("24.0-70.0 mm F 2.8"), ["24-70mm", "f2.8"]);
        assert_eq!(t("XF35mmF1.4 R"), ["xf", "35mm", "f1.4", "r"]);
        assert_eq!(t("Sigma 24-105mm f/4.0"), ["sigma", "24-105mm", "f4"]);
    }

    #[test]
    fn focal_and_aperture_tokens() {
        assert!(is_focal("35mm") && is_focal("150-600mm") && !is_focal("mm"));
        assert_eq!(aperture_key("f2.8g"), Some("2.8"));
        assert_eq!(aperture_key("f5-6.3"), Some("5-6.3"));
        assert_eq!(aperture_key("f012"), None);
    }

    #[test]
    fn scores_require_focal_and_prefer_specific() {
        let makers = maker_words(["Nikon"]);
        let shot = t("AF-S Nikkor 24-70mm f/2.8G ED VR");
        let exact = score(&t("Nikon AF-S Nikkor 24-70mm f/2.8G ED VR"), &shot, &makers).unwrap();
        let less = score(&t("Nikon AF-S Nikkor 24-70mm f/2.8G ED"), &shot, &makers).unwrap();
        assert!(exact > less && (exact - 1.0).abs() < 1e-12);
        assert_eq!(
            score(&t("Nikon AF-S Nikkor 24-85mm f/2.8G ED"), &shot, &makers),
            None
        );
        assert!(same_maker("NIKON CORPORATION", "Nikon"));
    }
}

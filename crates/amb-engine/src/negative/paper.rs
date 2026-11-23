// SPDX-License-Identifier: GPL-3.0-or-later
//! The paper curve (negative model v2, ours): the converted negative is
//! printed on a modelled photographic paper instead of mapped back to scene
//! light, so the result is a print's reflectance (paper white = 1).
//!
//! A paper's characteristic curve gives print density D from log exposure x
//! as a logistic: a toe, a straight line and a shoulder,
//! D(x) = Dmin + (Dmax − Dmin) / (1 + e^(−g·x)). The slope g is solved so the
//! curve's ISO R (log-exposure range ×100 between Dmin + 0.04 and 90 % of the
//! way to Dmax) is the grade's. Split grade prints with a grade 0 and a
//! grade 5 emulsion; a preflash adds a share of the paper's threshold
//! exposure everywhere. docs/design/film-negatives.md has the details.

/// Log exposure (decades) that the negative's levels (0..1) span on the
/// paper at contrast 1 (ours: a normal negative then fits grade 2, ISO R 100).
pub(crate) const PAPER_NEGATIVE_RANGE: f64 = 1.0;

/// ISO R of grades 0 to 5 (ours, from the ranges paper datasheets commonly
/// print); fractional grades are interpolated, as multigrade filters come in
/// half grades.
const ISO_R: [f64; 6] = [140.0, 120.0, 100.0, 80.0, 60.0, 40.0];

/// ISO R's lower end: the density above Dmin where the paper starts to
/// respond (its threshold), and its upper end as a share of Dmax − Dmin.
const THRESHOLD_DENSITY: f64 = 0.04;
const RANGE_TOP: f64 = 0.9;

/// Split grade: the grades of the soft and hard emulsions.
const SOFT_GRADE: f64 = 0.0;
const HARD_GRADE: f64 = 5.0;

/// Paper log exposure is kept inside this, so e^(−g·x) stays finite.
const X_LIMIT: f64 = 30.0;

/// A paper (`amb:Paper`). Our own, named generically: no claim to match a product.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Densest black and paper white's own density.
    pub d_max: f64,
    pub d_min: f64,
    /// Image tone on black-and-white prints: extra density per channel at
    /// Dmax, growing with density from none at paper white.
    pub tone: [f64; 3],
}

/// The built-in papers; the first is the default and stands in for unknown ids.
pub const PAPERS: &[Paper] = &[
    Paper {
        id: "glossy-neutral",
        name: "Glossy Neutral",
        description: "Glossy paper with deep blacks and a neutral image tone.",
        d_max: 2.10,
        d_min: 0.05,
        tone: [0.0; 3],
    },
    Paper {
        id: "matte-neutral",
        name: "Matte Neutral",
        description: "Matte paper: softer blacks and a gentle, neutral print.",
        d_max: 1.60,
        d_min: 0.07,
        tone: [0.0; 3],
    },
    Paper {
        id: "warm-fibre",
        name: "Warm Fibre",
        description: "Fibre-based paper with a warm image tone on black-and-white prints.",
        d_max: 2.00,
        d_min: 0.06,
        tone: [-0.02, 0.01, 0.06],
    },
];

/// The paper with `id`, or the default one.
pub fn paper(id: &str) -> &'static Paper {
    PAPERS.iter().find(|p| p.id == id).unwrap_or(&PAPERS[0])
}

/// The paper print settings (`amb:PaperCurve`, `amb:Paper*`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaperPrint {
    pub enabled: bool,
    /// Paper id (see [`PAPERS`]).
    pub paper: String,
    /// 0..5, fractional allowed; ignored with split grade.
    pub grade: f64,
    pub split_grade: bool,
    /// Split grade: soft and hard exposure in EV (more = darker).
    pub soft_exposure: f64,
    pub hard_exposure: f64,
    /// Preflash, 0..100 % of the paper's threshold exposure.
    pub preflash: f64,
}

impl Default for PaperPrint {
    fn default() -> Self {
        PaperPrint {
            enabled: false,
            paper: PAPERS[0].id.into(),
            grade: 2.0,
            split_grade: false,
            soft_exposure: 0.0,
            hard_exposure: 0.0,
            preflash: 0.0,
        }
    }
}

/// ISO R of a (fractional) grade.
pub(crate) fn iso_range(grade: f64) -> f64 {
    let g = grade.clamp(0.0, 5.0);
    let i = (g.floor() as usize).min(4);
    ISO_R[i] + (ISO_R[i + 1] - ISO_R[i]) * (g - i as f64)
}

fn logit(f: f64) -> f64 {
    (f / (1.0 - f)).ln()
}

/// One emulsion's characteristic curve, centred at x = 0.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Curve {
    d_min: f64,
    span: f64,
    slope: f64,
}

impl Curve {
    pub(crate) fn new(paper: &Paper, grade: f64) -> Curve {
        let span = paper.d_max - paper.d_min;
        let r = iso_range(grade) / 100.0;
        let slope = (logit(RANGE_TOP) - logit(THRESHOLD_DENSITY / span)) / r;
        Curve {
            d_min: paper.d_min,
            span,
            slope,
        }
    }

    /// Print density at paper log exposure `x`.
    pub(crate) fn density(&self, x: f64) -> f64 {
        let x = x.clamp(-X_LIMIT, X_LIMIT);
        self.d_min + self.span / (1.0 + (-self.slope * x).exp())
    }

    /// The paper log exposure where density reaches Dmin + the threshold.
    fn threshold(&self) -> f64 {
        logit(THRESHOLD_DENSITY / self.span) / self.slope
    }
}

/// One emulsion as printed: its curve, its exposure offset (decades) and the
/// preflash added to its exposure (linear, relative to x = 0).
#[derive(Debug, Clone, Copy)]
struct Emulsion {
    curve: Curve,
    offset: f64,
    flash: f64,
}

/// A prepared paper print.
#[derive(Debug, Clone)]
pub(crate) struct PaperCurve {
    emulsions: Vec<Emulsion>,
    span: f64,
    tone: [f64; 3],
    /// Paper log exposure of the midtone: where the plain print (no offsets,
    /// no preflash) has reflectance `midtone_grey`.
    anchor: f64,
}

impl PaperCurve {
    /// `black_and_white`: whether the paper's image tone shows.
    pub(crate) fn new(p: &PaperPrint, midtone_grey: f64, black_and_white: bool) -> PaperCurve {
        let paper = paper(&p.paper);
        let ev = std::f64::consts::LOG10_2;
        let flash = (p.preflash / 100.0).clamp(0.0, 1.0);
        let emulsion = |grade: f64, exposure: f64| {
            let curve = Curve::new(paper, grade);
            Emulsion {
                curve,
                offset: exposure * ev,
                flash: flash * 10f64.powf(curve.threshold()),
            }
        };
        let emulsions = if p.split_grade {
            vec![
                emulsion(SOFT_GRADE, p.soft_exposure),
                emulsion(HARD_GRADE, p.hard_exposure),
            ]
        } else {
            vec![emulsion(p.grade, 0.0)]
        };
        let mut c = PaperCurve {
            emulsions,
            span: paper.d_max - paper.d_min,
            tone: if black_and_white {
                paper.tone
            } else {
                [0.0; 3]
            },
            anchor: 0.0,
        };
        c.anchor = c.solve_anchor(-(midtone_grey.clamp(1e-4, 1.0)).log10());
        c
    }

    /// Density above Dmin at paper log exposure `x`: the emulsions' mean.
    pub(crate) fn density(&self, x: f64) -> f64 {
        self.densities(x, true)
    }

    fn densities(&self, x: f64, printed: bool) -> f64 {
        let sum: f64 = self
            .emulsions
            .iter()
            .map(|e| {
                let x = if printed {
                    let x = (x + e.offset).clamp(-X_LIMIT, X_LIMIT);
                    (10f64.powf(x) + e.flash).log10()
                } else {
                    x
                };
                e.curve.density(x) - e.curve.d_min
            })
            .sum();
        (sum / self.emulsions.len() as f64).clamp(0.0, self.span)
    }

    /// The x where the plain print has density `d` above Dmin (bisection;
    /// the curve rises).
    fn solve_anchor(&self, d: f64) -> f64 {
        let (mut lo, mut hi) = (-X_LIMIT, X_LIMIT);
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if self.densities(mid, false) < d {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Print reflectance of channel `c` against paper white, for scene log
    /// exposure `e` relative to the midtone (decades; higher = brighter).
    pub(crate) fn reflectance(&self, e: f64, c: usize) -> f64 {
        let d = self.density(self.anchor - e);
        10f64.powf(-(d + self.tone[c] * d / self.span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: f64 = 0.18;

    fn print(p: PaperPrint) -> PaperCurve {
        PaperCurve::new(&p, GREY, true)
    }

    fn graded(paper: &str, grade: f64) -> PaperCurve {
        print(PaperPrint {
            enabled: true,
            paper: paper.into(),
            grade,
            ..PaperPrint::default()
        })
    }

    /// The x where `c`'s density above Dmin reaches `d` (bisection).
    fn x_at(c: &PaperCurve, d: f64) -> f64 {
        let (mut lo, mut hi) = (-X_LIMIT, X_LIMIT);
        for _ in 0..100 {
            let mid = 0.5 * (lo + hi);
            if c.density(mid) < d {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// The ISO R a print shows, measured on its density curve.
    fn measured_range(c: &PaperCurve) -> f64 {
        100.0 * (x_at(c, RANGE_TOP * c.span) - x_at(c, THRESHOLD_DENSITY))
    }

    #[test]
    fn each_grade_has_its_iso_range() {
        for paper in PAPERS {
            for (grade, r) in ISO_R.iter().enumerate() {
                let got = measured_range(&graded(paper.id, grade as f64));
                assert!((got - r).abs() <= 2.0, "{} grade {grade}: {got}", paper.id);
            }
            let half = measured_range(&graded(paper.id, 2.5));
            assert!((half - 90.0).abs() <= 2.0, "{} grade 2.5: {half}", paper.id);
        }
    }

    #[test]
    fn curve_rises_between_dmin_and_dmax() {
        for paper in PAPERS {
            for grade in [0.0, 2.0, 5.0] {
                let c = Curve::new(paper, grade);
                let mut last = c.density(-5.0);
                assert!(last >= paper.d_min);
                for i in -49..=50 {
                    let d = c.density(f64::from(i) / 10.0);
                    assert!(d >= last && d <= paper.d_max, "{} {grade}", paper.id);
                    last = d;
                }
                assert!((c.density(10.0) - paper.d_max).abs() < 1e-3);
                assert!((c.density(-10.0) - paper.d_min).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn paper_white_is_one_and_the_midtone_is_grey() {
        for paper in PAPERS {
            let c = print(PaperPrint {
                enabled: true,
                paper: paper.id.into(),
                ..PaperPrint::default()
            });
            for ch in 0..3 {
                assert!((c.reflectance(10.0, ch) - 1.0).abs() < 1e-3);
            }
            let mid = PaperCurve::new(
                &PaperPrint {
                    paper: paper.id.into(),
                    ..PaperPrint::default()
                },
                GREY,
                false,
            );
            assert!(
                (mid.reflectance(0.0, 1) - GREY).abs() < 1e-6,
                "{}",
                paper.id
            );
            let black = 10f64.powf(-(paper.d_max - paper.d_min));
            assert!((mid.reflectance(-10.0, 1) - black).abs() < 1e-4);
        }
    }

    #[test]
    fn preflash_darkens_only_the_lightest_tones() {
        let plain = graded("glossy-neutral", 3.0);
        let flashed = print(PaperPrint {
            enabled: true,
            grade: 3.0,
            preflash: 80.0,
            ..PaperPrint::default()
        });
        // A bright highlight (paper barely exposed) and a deep shadow.
        let light = |c: &PaperCurve| c.density(c.anchor - 0.5);
        let dark = |c: &PaperCurve| c.density(c.anchor + 1.0);
        assert!(light(&flashed) - light(&plain) > 0.05);
        assert!((dark(&flashed) - dark(&plain)).abs() < 0.002);
    }

    #[test]
    fn split_grade_lies_between_grades_0_and_5() {
        let split = print(PaperPrint {
            enabled: true,
            split_grade: true,
            ..PaperPrint::default()
        });
        let r = measured_range(&split);
        let soft = measured_range(&graded("glossy-neutral", 0.0));
        let hard = measured_range(&graded("glossy-neutral", 5.0));
        assert!(hard < r && r < soft, "{hard} < {r} < {soft}");
        assert!((split.reflectance(0.0, 1) - GREY).abs() < 1e-6);
        // More hard exposure: darker shadows, and more contrast.
        let harder = print(PaperPrint {
            enabled: true,
            split_grade: true,
            hard_exposure: 1.0,
            ..PaperPrint::default()
        });
        assert!(harder.reflectance(-0.3, 1) < split.reflectance(-0.3, 1));
    }

    #[test]
    fn warm_tone_only_on_black_and_white() {
        let warm = PaperPrint {
            enabled: true,
            paper: "warm-fibre".into(),
            ..PaperPrint::default()
        };
        let bw = PaperCurve::new(&warm, GREY, true);
        assert!(bw.reflectance(0.0, 0) > bw.reflectance(0.0, 2));
        let colour = PaperCurve::new(&warm, GREY, false);
        assert_eq!(colour.reflectance(0.0, 0), colour.reflectance(0.0, 2));
        assert_eq!(paper("nope").id, "glossy-neutral");
    }
}

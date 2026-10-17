//! Primaries and curves node (aba Cor marco 3; node 01 "global" of the KODA powergrade): the
//! DaVinci Primaries panel — Lift / Gamma / Gain (Y, R, G, B) and Offset (R, G, B) in bars or
//! wheels, Temp, Tint, Contrast, Pivot, Mid Detail, Color Boost, Shadows, Highlights, Saturation,
//! Hue and Lum Mix — and the custom curves Y / R / G / B, with DaVinci's defaults and ranges.
//!
//! Blackmagic does not publish the formulas. The ones here are the usual grading-tool math
//! (lift / gain / power, pivoted contrast, Rec.709 luma and chroma), with Temp and Tint **fitted
//! to DaVinci's output** (black-box: frames 00 → 01 of the owner's powergrade, whose node 01 values
//! are visible in the reference screenshot). Every choice not checked against DaVinci is listed
//! in `docs/koda/DIARIO.md` as a "decisão para revisar".
//!
//! Order inside the node (as DaVinci processes a corrector node: detail, balance, primaries,
//! then curves): Mid Detail (spatial, [`MidDetail`]) → Temp / Tint → Lift, Gain, Gamma per channel
//! then the Y (master) controls mixed by Lum Mix → Offset → Contrast around Pivot → Shadows /
//! Highlights → Color Boost → Saturation and Hue → curve Y (master, per channel) → curves R, G, B.
//!
//! The CPU ([`Ready::apply`]) and the GPU (`grade.wgsl`, `prim`) read the same flat parameter
//! block and the same baked curve table ([`Ready::aux`]), so they agree to float rounding.

use serde::{Deserialize, Serialize};

/// Neutral values and ranges, as DaVinci shows them.
pub mod range {
    pub const LIFT: (f32, f32) = (-1.0, 1.0);
    pub const GAMMA: (f32, f32) = (-1.0, 1.0);
    pub const GAIN: (f32, f32) = (0.0, 16.0);
    /// Offset is shown around 25 (DaVinci's neutral); one unit is 1 % of the signal.
    pub const OFFSET: (f32, f32) = (-75.0, 125.0);
    pub const OFFSET_NEUTRAL: f32 = 25.0;
    pub const TEMP: (f32, f32) = (-4000.0, 4000.0);
    pub const TINT: (f32, f32) = (-100.0, 100.0);
    pub const CONTRAST: (f32, f32) = (0.0, 2.0);
    pub const PIVOT: (f32, f32) = (0.0, 1.0);
    pub const PIVOT_DEFAULT: f32 = 0.435;
    pub const MID_DETAIL: (f32, f32) = (-100.0, 100.0);
    pub const COLOR_BOOST: (f32, f32) = (-100.0, 100.0);
    pub const SHADOWS: (f32, f32) = (-100.0, 100.0);
    pub const HIGHLIGHTS: (f32, f32) = (-100.0, 100.0);
    /// Saturation and Hue: 50 is neutral.
    pub const SAT: (f32, f32) = (0.0, 100.0);
    pub const HUE: (f32, f32) = (0.0, 100.0);
    pub const LUM_MIX: (f32, f32) = (0.0, 100.0);
    /// Most points a curve keeps.
    pub const CURVE_POINTS: usize = 32;
}

/// Rec.709 luma weights (DaVinci's YRGB luminance).
pub const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// Temp / Tint as channel gains, fitted to DaVinci (Temp −69.9 and Tint −5.78 give R ×0.975 and
/// B ×1.0225 against G in the signal; `examples/calibra_primarias.rs`): R = e^(TEMP_K·temp),
/// B = e^(−TEMP_K·temp), G = e^(−TINT_K·tint), then scaled to keep luma. Exponential so the far
/// ends of the range never give a negative gain (one calibration point: a decision to review).
pub const TEMP_K: f32 = 3.402e-4;
pub const TINT_K: f32 = 2.16e-4;

/// Mid Detail: Gaussian sigma in clip pixels and strength at 100.
pub const MID_DETAIL_SIGMA: f32 = 12.0;
pub const MID_DETAIL_GAIN: f32 = 1.5;

/// Shadows / Highlights: largest luma change at ±100.
pub const SHADOWS_GAIN: f32 = 0.25;

/// Samples of a baked curve over [0, 1].
pub const CURVE_SAMPLES: usize = 1024;

/// How the panel shows Lift / Gamma / Gain / Offset (the numbers are the same either way).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    #[default]
    Bars,
    Wheels,
}

/// One custom curve: control points (x, y) in [0, 1], sorted by x.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CurvePoints(pub Vec<[f32; 2]>);

impl Default for CurvePoints {
    fn default() -> Self {
        CurvePoints(vec![[0.0, 0.0], [1.0, 1.0]])
    }
}

impl CurvePoints {
    /// Points safe to evaluate: finite, x in [0, 1] (y in [−0.5, 1.5]), sorted, one per x, at
    /// most [`range::CURVE_POINTS`], end points at x = 0 and x = 1 (added flat when missing).
    pub fn sanitized(&self) -> Self {
        let mut v: Vec<[f32; 2]> = self
            .0
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| [p[0].clamp(0.0, 1.0), p[1].clamp(-0.5, 1.5)])
            .take(range::CURVE_POINTS * 4)
            .collect();
        v.sort_by(|a, b| a[0].total_cmp(&b[0]));
        v.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-4);
        v.truncate(range::CURVE_POINTS);
        if v.is_empty() {
            return CurvePoints::default();
        }
        if let Some(&first) = v.first().filter(|p| p[0] > 0.0) {
            v.insert(0, [0.0, first[1]]);
        }
        if let Some(&last) = v.last().filter(|p| p[0] < 1.0) {
            v.push([1.0, last[1]]);
        }
        if v.len() > range::CURVE_POINTS {
            // the ends were added to a full list: drop interior points next to them
            v.remove(1);
            if v.len() > range::CURVE_POINTS {
                let n = v.len();
                v.remove(n - 2);
            }
        }
        CurvePoints(v)
    }

    pub fn is_identity(&self) -> bool {
        self.0.iter().all(|p| (p[0] - p[1]).abs() < 1e-6)
    }

    /// The curve at `x` in [0, 1]: monotone cubic (Fritsch–Carlson) through the points, so it
    /// never overshoots between them. Expects sanitized points.
    pub fn eval(&self, x: f32) -> f32 {
        let p = &self.0;
        let n = p.len();
        let (Some(first), Some(last)) = (p.first(), p.last()) else { return x };
        if n < 2 {
            return first[1];
        }
        if x <= first[0] {
            return first[1];
        }
        if x >= last[0] {
            return last[1];
        }
        let k = p.iter().position(|q| q[0] > x).unwrap_or(n - 1).clamp(1, n - 1) - 1;
        let slopes = self.tangents();
        let (Some(a), Some(b), Some(&ma), Some(&mb)) = (p.get(k), p.get(k + 1), slopes.get(k), slopes.get(k + 1)) else { return x };
        let h = b[0] - a[0];
        if h <= 0.0 {
            return a[1];
        }
        let t = (x - a[0]) / h;
        let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * a[1] + (t3 - 2.0 * t2 + t) * h * ma + (-2.0 * t3 + 3.0 * t2) * b[1] + (t3 - t2) * h * mb
    }

    /// Fritsch–Carlson tangents at the points.
    fn tangents(&self) -> Vec<f32> {
        let p = &self.0;
        let n = p.len();
        let d: Vec<f32> = p.windows(2).map(|w| (w[1][1] - w[0][1]) / (w[1][0] - w[0][0]).max(1e-6)).collect();
        let mut m = vec![0f32; n];
        for i in 0..n {
            m[i] = match (i.checked_sub(1).and_then(|j| d.get(j)), d.get(i)) {
                (None, Some(&r)) => r,
                (Some(&l), None) => l,
                (Some(&l), Some(&r)) if l * r > 0.0 => (l + r) / 2.0,
                _ => 0.0,
            };
        }
        for (i, &dk) in d.iter().enumerate() {
            if dk == 0.0 {
                m[i] = 0.0;
                if let Some(x) = m.get_mut(i + 1) {
                    *x = 0.0;
                }
                continue;
            }
            let (a, b) = (m[i] / dk, m.get(i + 1).copied().unwrap_or(0.0) / dk);
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                m[i] = t * a * dk;
                if let Some(x) = m.get_mut(i + 1) {
                    *x = t * b * dk;
                }
            }
        }
        m
    }
}

/// The four custom curves. `linked`: editing R, G or B edits all three (DaVinci's join).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Curves {
    pub y: CurvePoints,
    pub r: CurvePoints,
    pub g: CurvePoints,
    pub b: CurvePoints,
    pub linked: bool,
}

impl Default for Curves {
    fn default() -> Self {
        Curves { y: CurvePoints::default(), r: CurvePoints::default(), g: CurvePoints::default(), b: CurvePoints::default(), linked: true }
    }
}

impl Curves {
    pub fn is_identity(&self) -> bool {
        self.y.is_identity() && self.r.is_identity() && self.g.is_identity() && self.b.is_identity()
    }

    /// The curve of a channel name ("y", "r", "g", "b").
    pub fn channel_mut(&mut self, ch: &str) -> Option<&mut CurvePoints> {
        match ch {
            "y" => Some(&mut self.y),
            "r" => Some(&mut self.r),
            "g" => Some(&mut self.g),
            "b" => Some(&mut self.b),
            _ => None,
        }
    }

    /// Set a channel's points (sanitized); linked R / G / B move together.
    pub fn set(&mut self, ch: &str, pts: CurvePoints) -> Result<(), String> {
        let pts = pts.sanitized();
        if self.linked && matches!(ch, "r" | "g" | "b") {
            self.r = pts.clone();
            self.g = pts.clone();
            self.b = pts;
            return Ok(());
        }
        *self.channel_mut(ch).ok_or_else(|| format!("channel: expected \"y\", \"r\", \"g\" or \"b\", not \"{ch}\""))? = pts;
        Ok(())
    }

    fn sanitized(&self) -> Self {
        Curves { y: self.y.sanitized(), r: self.r.sanitized(), g: self.g.sanitized(), b: self.b.sanitized(), linked: self.linked }
    }
}

/// The node's parameters. Lift / Gamma / Gain are `[Y, R, G, B]`, Offset `[R, G, B]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Primaries {
    pub mode: Mode,
    pub lift: [f32; 4],
    pub gamma: [f32; 4],
    pub gain: [f32; 4],
    pub offset: [f32; 3],
    pub temp: f32,
    pub tint: f32,
    pub contrast: f32,
    pub pivot: f32,
    pub mid_detail: f32,
    pub color_boost: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub sat: f32,
    pub hue: f32,
    pub lum_mix: f32,
    pub curves: Curves,
}

impl Default for Primaries {
    fn default() -> Self {
        Primaries {
            mode: Mode::Bars,
            lift: [0.0; 4],
            gamma: [0.0; 4],
            gain: [1.0; 4],
            offset: [range::OFFSET_NEUTRAL; 3],
            temp: 0.0,
            tint: 0.0,
            contrast: 1.0,
            pivot: range::PIVOT_DEFAULT,
            mid_detail: 0.0,
            color_boost: 0.0,
            shadows: 0.0,
            highlights: 0.0,
            sat: 50.0,
            hue: 50.0,
            lum_mix: 100.0,
            curves: Curves::default(),
        }
    }
}

fn cl(x: f32, r: (f32, f32), default: f32) -> f32 {
    if x.is_finite() { x.clamp(r.0, r.1) } else { default }
}

impl Primaries {
    /// Every number in range (NaN → the default), curves sanitized.
    pub fn sanitized(&self) -> Self {
        let d = Primaries::default();
        Primaries {
            mode: self.mode,
            lift: self.lift.map(|v| cl(v, range::LIFT, 0.0)),
            gamma: self.gamma.map(|v| cl(v, range::GAMMA, 0.0)),
            gain: self.gain.map(|v| cl(v, range::GAIN, 1.0)),
            offset: self.offset.map(|v| cl(v, range::OFFSET, range::OFFSET_NEUTRAL)),
            temp: cl(self.temp, range::TEMP, d.temp),
            tint: cl(self.tint, range::TINT, d.tint),
            contrast: cl(self.contrast, range::CONTRAST, d.contrast),
            pivot: cl(self.pivot, range::PIVOT, d.pivot),
            mid_detail: cl(self.mid_detail, range::MID_DETAIL, 0.0),
            color_boost: cl(self.color_boost, range::COLOR_BOOST, 0.0),
            shadows: cl(self.shadows, range::SHADOWS, 0.0),
            highlights: cl(self.highlights, range::HIGHLIGHTS, 0.0),
            sat: cl(self.sat, range::SAT, d.sat),
            hue: cl(self.hue, range::HUE, d.hue),
            lum_mix: cl(self.lum_mix, range::LUM_MIX, d.lum_mix),
            curves: self.curves.sanitized(),
        }
    }

    /// Whether the per-pixel part changes nothing (Mid Detail aside).
    pub fn pixel_identity(&self) -> bool {
        let d = Primaries::default();
        self.lift == d.lift
            && self.gamma == d.gamma
            && self.gain == d.gain
            && self.offset == d.offset
            && self.temp == 0.0
            && self.tint == 0.0
            && self.contrast == 1.0
            && self.color_boost == 0.0
            && self.shadows == 0.0
            && self.highlights == 0.0
            && self.sat == d.sat
            && self.hue == d.hue
            && self.curves.is_identity()
    }

    pub fn is_identity(&self) -> bool {
        self.pixel_identity() && self.mid_detail == 0.0
    }

    /// Set a parameter by its JSON name: `lift` / `gamma` / `gain` take `[y, r, g, b]` (offset
    /// `[r, g, b]`) or one channel as `lift.y`, `offset.r`…; `mode` is "bars" or "wheels";
    /// `curvesLinked` a bool; the rest a number. Values are clamped to DaVinci's ranges.
    pub fn set_param(&mut self, name: &str, v: &serde_json::Value) -> Result<(), String> {
        let num = || v.as_f64().filter(|x| x.is_finite()).map(|x| x as f32).ok_or_else(|| format!("{name}: expected a finite number"));
        let arr = |n: usize| -> Result<Vec<f32>, String> {
            let a = v.as_array().filter(|a| a.len() == n).ok_or_else(|| format!("{name}: expected {n} numbers"))?;
            a.iter().map(|x| x.as_f64().filter(|x| x.is_finite()).map(|x| x as f32).ok_or_else(|| format!("{name}: expected {n} finite numbers"))).collect()
        };
        let (group, ch) = match name.split_once('.') {
            Some((g, c)) => (g, Some(c)),
            None => (name, None),
        };
        let idx = |c: &str, with_y: bool| -> Result<usize, String> {
            let i = match c {
                "y" if with_y => 0,
                "r" => 0,
                "g" => 1,
                "b" => 2,
                _ => return Err(format!("{name}: channel is y, r, g or b")),
            };
            Ok(if with_y && c != "y" { i + 1 } else { i })
        };
        match (group, ch) {
            ("lift" | "gamma" | "gain", None) => {
                let a = arr(4)?;
                let t = self.ywheel_mut(group);
                for (d, s) in t.iter_mut().zip(a) {
                    *d = s;
                }
            }
            ("lift" | "gamma" | "gain", Some(c)) => {
                let i = idx(c, true)?;
                let x = num()?;
                if let Some(d) = self.ywheel_mut(group).get_mut(i) {
                    *d = x;
                }
            }
            ("offset", None) => {
                let a = arr(3)?;
                for (d, s) in self.offset.iter_mut().zip(a) {
                    *d = s;
                }
            }
            ("offset", Some(c)) => {
                let i = idx(c, false)?;
                let x = num()?;
                if let Some(d) = self.offset.get_mut(i) {
                    *d = x;
                }
            }
            ("mode", None) => {
                self.mode = match v.as_str() {
                    Some("bars") => Mode::Bars,
                    Some("wheels") => Mode::Wheels,
                    _ => return Err("mode: \"bars\" or \"wheels\"".into()),
                }
            }
            ("curvesLinked", None) => self.curves.linked = v.as_bool().ok_or("curvesLinked: expected true or false")?,
            ("temp", None) => self.temp = num()?,
            ("tint", None) => self.tint = num()?,
            ("contrast", None) => self.contrast = num()?,
            ("pivot", None) => self.pivot = num()?,
            ("midDetail", None) => self.mid_detail = num()?,
            ("colorBoost", None) => self.color_boost = num()?,
            ("shadows", None) => self.shadows = num()?,
            ("highlights", None) => self.highlights = num()?,
            ("sat", None) => self.sat = num()?,
            ("hue", None) => self.hue = num()?,
            ("lumMix", None) => self.lum_mix = num()?,
            _ => {
                return Err(format!(
                    "primaries node has no parameter \"{name}\" (lift, gamma, gain, offset — or lift.y, offset.r… — temp, tint, contrast, pivot, midDetail, colorBoost, shadows, highlights, sat, hue, lumMix, mode, curvesLinked)"
                ));
            }
        }
        *self = self.sanitized();
        Ok(())
    }

    fn ywheel_mut(&mut self, g: &str) -> &mut [f32; 4] {
        match g {
            "lift" => &mut self.lift,
            "gamma" => &mut self.gamma,
            _ => &mut self.gain,
        }
    }

    /// Reset a group of controls to DaVinci's defaults: "all", "lift", "gamma", "gain",
    /// "offset", "curves", or a curve channel "curve.y" / "curve.r"…
    pub fn reset(&mut self, what: &str) -> Result<(), String> {
        let d = Primaries::default();
        match what {
            "all" => *self = Primaries { mode: self.mode, ..d },
            "lift" => self.lift = d.lift,
            "gamma" => self.gamma = d.gamma,
            "gain" => self.gain = d.gain,
            "offset" => self.offset = d.offset,
            "curves" => self.curves = Curves { linked: self.curves.linked, ..Curves::default() },
            c if c.starts_with("curve.") => {
                let ch = c.trim_start_matches("curve.");
                *self.curves.channel_mut(ch).ok_or_else(|| format!("reset: no curve \"{ch}\""))? = CurvePoints::default();
            }
            other => return Err(format!("reset: \"{other}\" is not all, lift, gamma, gain, offset, curves or curve.<y|r|g|b>")),
        }
        Ok(())
    }

    /// The node ready to run on pixels (flat parameters and the baked curves).
    pub fn ready(&self) -> Ready {
        let mut p = [0f32; PARAMS];
        // white balance gains, luma kept
        let mut wb = [(TEMP_K * self.temp).exp(), (-TINT_K * self.tint).exp(), (-TEMP_K * self.temp).exp()];
        let y = LUMA[0] * wb[0] + LUMA[1] * wb[1] + LUMA[2] * wb[2];
        if y > 1e-6 {
            wb = wb.map(|v| v / y);
        }
        p[0..3].copy_from_slice(&wb);
        p[3] = self.lum_mix / 100.0;
        p[4..8].copy_from_slice(&self.lift);
        p[8..12].copy_from_slice(&self.gain);
        // gamma as the power's exponent: 2^(−gamma)
        for i in 0..4 {
            p[12 + i] = (-self.gamma[i]).exp2();
        }
        for i in 0..3 {
            p[16 + i] = (self.offset[i] - range::OFFSET_NEUTRAL) / 100.0;
        }
        p[19] = self.contrast;
        p[20] = self.pivot;
        p[21] = self.shadows / 100.0 * SHADOWS_GAIN;
        p[22] = self.highlights / 100.0 * SHADOWS_GAIN;
        p[23] = self.color_boost / 100.0;
        p[24] = self.sat / 50.0;
        let th = (self.hue - 50.0) / 50.0 * std::f32::consts::PI;
        p[25] = th.cos();
        p[26] = th.sin();
        let curves = !self.curves.is_identity();
        p[27] = f32::from(u8::from(curves));
        // curve table: texel i = (y, r, g, b) at x = i / (N − 1); end slopes after
        let n = CURVE_SAMPLES;
        let table: Vec<[f32; 4]> = (0..n)
            .map(|i| {
                let x = i as f32 / (n - 1) as f32;
                [self.curves.y.eval(x), self.curves.r.eval(x), self.curves.g.eval(x), self.curves.b.eval(x)]
            })
            .collect();
        let slope = |a: usize, b: usize, c: usize| table.get(b).zip(table.get(a)).map_or(1.0, |(tb, ta)| (tb[c] - ta[c]) * (n - 1) as f32);
        for c in 0..4 {
            p[28 + c] = slope(0, 1, c);
            p[32 + c] = slope(n - 2, n - 1, c);
        }
        Ready { p, table }
    }
}

/// Node 01 "global" of the KODA powergrade: the values DaVinci's panel shows (Temp −69.9, Tint
/// −5.78, Shadows 17.5, the rest at the defaults) and a Y curve fitted to DaVinci's output, since
/// its points are not visible (`examples/calibra_primarias.rs --curve`; the points below 0.3 and
/// above 0.75 are not constrained by the reference frame).
pub fn koda_global() -> Primaries {
    let mut p = Primaries { temp: -69.9, tint: -5.78, shadows: 17.5, ..Default::default() };
    p.curves.y = CurvePoints(vec![
        [0.0, 0.125],
        [0.2, 0.0825],
        [0.35, 0.2125],
        [0.45, 0.3287],
        [0.55, 0.4387],
        [0.65, 0.545],
        [0.75, 0.6563],
        [0.9, 0.94],
        [1.0, 0.995],
    ])
    .sanitized();
    p
}

/// Floats in the flat parameter block (row 0 of the GPU aux image, 4 per texel).
pub const PARAMS: usize = 36;

/// A primaries node ready to run.
#[derive(Clone, Debug, PartialEq)]
pub struct Ready {
    pub p: [f32; PARAMS],
    pub table: Vec<[f32; 4]>,
}

#[inline]
fn spow(x: f32, e: f32) -> f32 {
    x.abs().powf(e).copysign(x)
}

#[inline]
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    LUMA[0] * c[0] + LUMA[1] * c[1] + LUMA[2] * c[2]
}

impl Ready {
    /// Curve `c` (0 = Y, 1–3 = R, G, B) at `x`: the table, linear in between, extended past
    /// [0, 1] with the end slopes (no clipping).
    #[inline]
    fn curve(&self, c: usize, x: f32) -> f32 {
        let n = self.table.len();
        let at = |i: usize| self.table.get(i).map_or(0.0, |t| t[c]);
        if x <= 0.0 {
            return at(0) + self.p[28 + c] * x;
        }
        if x >= 1.0 {
            return at(n - 1) + self.p[32 + c] * (x - 1.0);
        }
        let f = x * (n - 1) as f32;
        let i = (f as usize).min(n - 2);
        let t = f - i as f32;
        at(i) + (at(i + 1) - at(i)) * t
    }

    /// The node on one pixel's signal.
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let p = &self.p;
        // Temp / Tint
        let mut c = [c[0] * p[0], c[1] * p[1], c[2] * p[2]];
        // Lift, Gain, Gamma per channel
        let lgg = |x: f32, k: usize| spow((x + p[4 + k] * (1.0 - x)) * p[8 + k], p[12 + k]);
        for (i, v) in c.iter_mut().enumerate() {
            *v = lgg(*v, i + 1);
        }
        // the Y (master) controls: per channel at Lum Mix 0, on luma only at 100
        let y = luma(c);
        let dy = lgg(y, 0) - y;
        let m = p[3];
        c = c.map(|v| (lgg(v, 0)) * (1.0 - m) + (v + dy) * m);
        // Offset
        for i in 0..3 {
            c[i] += p[16 + i];
        }
        // Contrast around Pivot
        c = c.map(|v| (v - p[20]) * p[19] + p[20]);
        // Shadows / Highlights: luma moved where it is dark / bright, colour kept
        let y = luma(c);
        let d = p[21] * (1.0 - smooth(0.0, 0.6, y)) + p[22] * smooth(0.4, 1.0, y);
        c = c.map(|v| v + d);
        // Color Boost (more on low saturation), Saturation, Hue
        let y = luma(c);
        let cb = (c[2] - y) / 1.8556;
        let cr = (c[0] - y) / 1.5748;
        let chroma = (cb * cb + cr * cr).sqrt();
        let boost = 1.0 + p[23] * (1.0 - (chroma * 4.0).min(1.0));
        let k = p[24] * boost;
        let (cb, cr) = ((cb * p[25] - cr * p[26]) * k, (cb * p[26] + cr * p[25]) * k);
        let r = y + 1.5748 * cr;
        let b = y + 1.8556 * cb;
        let g = (y - LUMA[0] * r - LUMA[2] * b) / LUMA[1];
        c = [r, g, b];
        // curves: Y as a master on every channel, then R, G, B
        if p[27] > 0.5 {
            for (i, v) in c.iter_mut().enumerate() {
                *v = self.curve(i + 1, self.curve(0, *v));
            }
        }
        c
    }

    /// The GPU aux image: row 0 the parameters (4 per texel), row 1 the curve table.
    pub fn aux(&self) -> filmcraft_render::external::AuxImage {
        let w = CURVE_SAMPLES;
        let mut rgba = vec![0f32; w * 2 * 4];
        for (d, s) in rgba.iter_mut().zip(self.p.iter()) {
            *d = *s;
        }
        for (i, t) in self.table.iter().enumerate() {
            if let Some(d) = rgba.get_mut((w + i) * 4..(w + i) * 4 + 4) {
                d.copy_from_slice(t);
            }
        }
        filmcraft_render::external::AuxImage { w: w as u32, h: 2, rgba }
    }
}

/// Mid Detail (a spatial part of the node): local contrast against a Gaussian of
/// [`MID_DETAIL_SIGMA`] clip pixels, weighted to the midtones. `amount` is the panel value / 100.
#[inline]
pub fn mid_detail(orig: [f32; 3], blur: [f32; 3], amount: f32) -> [f32; 3] {
    let (yo, yb) = (luma(orig), luma(blur));
    let w = (4.0 * yo * (1.0 - yo)).clamp(0.0, 1.0);
    let d = (yo - yb) * amount * MID_DETAIL_GAIN * w;
    orig.map(|v| v + d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 3], b: [f32; 3], tol: f32) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol)
    }

    #[test]
    fn defaults_are_davinci_and_identity() {
        let p = Primaries::default();
        assert_eq!((p.lift, p.gamma, p.gain, p.offset), ([0.0; 4], [0.0; 4], [1.0; 4], [25.0; 3]));
        assert_eq!((p.contrast, p.pivot, p.sat, p.hue, p.lum_mix), (1.0, 0.435, 50.0, 50.0, 100.0));
        assert!(p.is_identity());
        let r = p.ready();
        for c in [[0.0, 0.0, 0.0], [0.18, 0.4, 0.9], [-0.2, 1.4, 0.5], [2.0, 2.0, 2.0]] {
            assert!(close(r.apply(c), c, 1e-5), "{c:?} -> {:?}", r.apply(c));
        }
    }

    #[test]
    fn controls_move_the_picture_the_right_way() {
        let grey = [0.4f32; 3];
        let with = |f: &dyn Fn(&mut Primaries)| {
            let mut p = Primaries::default();
            f(&mut p);
            p.sanitized().ready().apply(grey)
        };
        assert!(with(&|p| p.gain[0] = 1.5)[1] > 0.55);
        assert!(with(&|p| p.lift[0] = 0.2)[1] > 0.45);
        assert!(with(&|p| p.gamma[0] = 0.5)[1] > 0.4, "positive gamma brightens");
        assert!((with(&|p| p.offset = [35.0; 3])[1] - 0.5).abs() < 1e-5, "offset +10 = +0.1");
        assert!(with(&|p| p.contrast = 1.5)[1] < 0.4, "below the pivot darkens");
        let cool = with(&|p| p.temp = -500.0);
        assert!(cool[2] > cool[0], "negative temp is cooler: {cool:?}");
        let green = with(&|p| p.tint = -50.0);
        assert!(green[1] > green[0] && green[1] > green[2], "negative tint is greener: {green:?}");
        // gain on the red channel only
        let r = with(&|p| p.gain[1] = 2.0);
        assert!(r[0] > 0.7 && (r[1] - 0.4).abs() < 1e-5);
        // saturation 0 is grey, 100 doubles the chroma
        let mut p = Primaries { sat: 0.0, ..Default::default() };
        let c = p.ready().apply([0.6, 0.4, 0.2]);
        assert!((c[0] - c[1]).abs() < 1e-5 && (c[1] - c[2]).abs() < 1e-5, "{c:?}");
        p.sat = 100.0;
        let c = p.ready().apply([0.6, 0.4, 0.2]);
        assert!(c[0] - c[2] > 0.75, "{c:?}");
        // Lum Mix 100: a Y gain keeps the channel differences (luma only)
        let c = Primaries { gain: [1.5, 1.0, 1.0, 1.0], ..Default::default() }.ready().apply([0.6, 0.4, 0.2]);
        assert!(((c[0] - c[2]) - 0.4).abs() < 1e-4, "{c:?}");
        let c = Primaries { gain: [1.5, 1.0, 1.0, 1.0], lum_mix: 0.0, ..Default::default() }.ready().apply([0.6, 0.4, 0.2]);
        assert!(((c[0] - c[2]) - 0.6).abs() < 1e-4, "{c:?}");
    }

    #[test]
    fn curves_are_monotone_and_clean() {
        let c = CurvePoints(vec![[0.0, 0.0], [0.25, 0.1], [0.5, 0.6], [0.55, 0.62], [1.0, 1.0]]).sanitized();
        let mut last = f32::MIN;
        for i in 0..=1000 {
            let v = c.eval(i as f32 / 1000.0);
            assert!(v >= last - 1e-6, "monotone at {i}: {v} < {last}");
            last = v;
        }
        // hostile points
        let h = CurvePoints(vec![[f32::NAN, 0.0], [2.0, 9.0], [0.5, -9.0], [0.5, 0.3], [-1.0, 0.0]]).sanitized();
        assert!(h.0.iter().all(|p| (0.0..=1.0).contains(&p[0]) && (-0.5..=1.5).contains(&p[1])));
        assert_eq!(h.0.first().map(|p| p[0]), Some(0.0));
        assert_eq!(h.0.last().map(|p| p[0]), Some(1.0));
        let many = CurvePoints((0..500).map(|i| [i as f32 / 499.0, 0.5]).collect()).sanitized();
        assert!(many.0.len() <= range::CURVE_POINTS);
        assert_eq!(CurvePoints(vec![]).sanitized(), CurvePoints::default());
        // linked R/G/B move together; Y stays
        let mut cv = Curves::default();
        cv.set("g", CurvePoints(vec![[0.0, 0.1], [1.0, 0.9]])).unwrap();
        assert_eq!(cv.r, cv.g);
        assert_eq!(cv.b, cv.g);
        assert!(cv.y.is_identity());
        assert!(cv.set("q", CurvePoints::default()).is_err());
        // a curve on the node, beyond [0, 1] it continues with the end slope
        let p = Primaries { curves: cv, ..Default::default() };
        let r = p.ready();
        let lo = r.apply([0.0; 3]);
        assert!((lo[0] - 0.1).abs() < 1e-3, "{lo:?}");
        let hi = r.apply([1.2; 3]);
        assert!(hi[0] > 0.9 && hi[0] < 1.2, "{hi:?}");
    }

    #[test]
    fn params_are_checked_and_clamped() {
        let mut p = Primaries::default();
        p.set_param("lift", &serde_json::json!([0.1, 0.2, 0.3, 9.0])).unwrap();
        assert_eq!(p.lift, [0.1, 0.2, 0.3, 1.0]);
        p.set_param("gain.r", &serde_json::json!(2.0)).unwrap();
        assert_eq!(p.gain[1], 2.0);
        p.set_param("offset.b", &serde_json::json!(30.0)).unwrap();
        assert_eq!(p.offset, [25.0, 25.0, 30.0]);
        p.set_param("temp", &serde_json::json!(-69.9)).unwrap();
        p.set_param("sat", &serde_json::json!(1000)).unwrap();
        assert_eq!(p.sat, 100.0);
        p.set_param("mode", &serde_json::json!("wheels")).unwrap();
        assert_eq!(p.mode, Mode::Wheels);
        assert!(p.set_param("offset.y", &serde_json::json!(1)).is_err());
        assert!(p.set_param("lift", &serde_json::json!([1, 2])).is_err());
        assert!(p.set_param("exposure", &serde_json::json!(1)).is_err());
        assert!(p.set_param("hue", &serde_json::json!("x")).is_err());
        p.reset("lift").unwrap();
        assert_eq!(p.lift, [0.0; 4]);
        p.reset("all").unwrap();
        assert_eq!(p, Primaries { mode: Mode::Wheels, ..Default::default() });
        assert!(p.reset("nope").is_err());
    }

    #[test]
    fn mid_detail_adds_local_contrast_in_the_midtones() {
        let o = mid_detail([0.5; 3], [0.4; 3], 0.5);
        assert!(o[0] > 0.5);
        assert_eq!(mid_detail([0.5; 3], [0.4; 3], 0.0), [0.5; 3]);
        // black and white barely move
        assert!((mid_detail([0.0; 3], [0.2; 3], 1.0)[0]).abs() < 1e-6);
    }
}

//! Color Space Transform node math (aba Cor, node 02 of the KODA powergrade): camera signal →
//! scene linear (input curve) → output gamut (3×3 matrix) → tone mapping → gamut mapping →
//! output signal (output curve). Pure functions on one pixel's RGB signal; the CPU reference
//! ([`crate::eval`]) and the GPU passes (`grade.wgsl`, parameters from [`CstParams::gpu`]) run the
//! same formulas.
//!
//! Curves and primaries come from `filmcraft-color` (Sony's published S-Log3 and S-Gamut3.Cine).
//! DaVinci Resolve's tone and gamut mapping algorithms are not published: the filmic tone curve
//! here was **fitted to DaVinci's output** (black-box, frames 01 → 02 of the owner's powergrade,
//! `docs/koda/DIARIO.md`), not derived from Blackmagic code.

use serde::{Deserialize, Serialize};

/// Colour gamuts a CST node converts between.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Gamut {
    SGamut3Cine,
    SGamut3,
    Rec709,
    Rec2020,
    P3D65,
}

impl Gamut {
    fn color(self) -> filmcraft_color::Gamut {
        match self {
            Gamut::SGamut3Cine => filmcraft_color::Gamut::SGamut3Cine,
            Gamut::SGamut3 => filmcraft_color::Gamut::SGamut3,
            Gamut::Rec709 => filmcraft_color::Gamut::Bt709,
            Gamut::Rec2020 => filmcraft_color::Gamut::Bt2020,
            Gamut::P3D65 => filmcraft_color::Gamut::P3D65,
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }
}

/// Transfer curves: how a signal encodes linear light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Curve {
    /// Sony S-Log3 (full-range code value / 1023 as the signal).
    SLog3,
    /// Pure power 2.4 (DaVinci's "Gamma 2.4", the Rec.709 display encoding).
    Gamma24,
    /// Pure power 2.2.
    Gamma22,
    /// sRGB.
    Srgb,
    /// No curve.
    Linear,
}

impl Curve {
    pub fn from_name(s: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(s.to_string())).ok()
    }

    /// Code for the GPU (`u.i1`).
    fn code(self) -> u32 {
        match self {
            Curve::SLog3 => 1,
            Curve::Gamma24 => 2,
            Curve::Gamma22 => 3,
            Curve::Srgb => 4,
            Curve::Linear => 5,
        }
    }

    /// Signal → linear. Odd-symmetric for the power curves (no clipping of negatives).
    pub fn decode(self, v: f32) -> f32 {
        match self {
            Curve::SLog3 => filmcraft_color::log::slog3_decode(v as f64) as f32,
            Curve::Gamma24 => v.abs().powf(2.4).copysign(v),
            Curve::Gamma22 => v.abs().powf(2.2).copysign(v),
            Curve::Srgb => crate::eval::to_linear(v),
            Curve::Linear => v,
        }
    }

    /// Linear → signal (inverse of [`Curve::decode`]).
    pub fn encode(self, l: f32) -> f32 {
        match self {
            Curve::SLog3 => filmcraft_color::log::slog3_encode(l as f64) as f32,
            Curve::Gamma24 => l.abs().powf(1.0 / 2.4).copysign(l),
            Curve::Gamma22 => l.abs().powf(1.0 / 2.2).copysign(l),
            Curve::Srgb => crate::eval::to_signal(l),
            Curve::Linear => l,
        }
    }
}

/// How scene light is brought into the display range.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum ToneMapping {
    /// No tone mapping: linear light goes straight to the output curve.
    None,
    /// The KODA filmic curve, per channel: display code (2.4) =
    /// `white / (1 + exp(-(log2(v) - mid) / contrast))`, i.e. a logistic in stops. The defaults
    /// fit DaVinci's S-Log3 → Rec.709 Gamma 2.4 CST, which tone maps each channel (measured:
    /// per channel ΔE 0.71 mean against 1.40 on luminance or max(RGB)).
    Filmic {
        #[serde(default = "f_white")]
        white: f32,
        #[serde(default = "f_mid")]
        mid: f32,
        #[serde(default = "f_contrast")]
        contrast: f32,
    },
}

fn f_white() -> f32 {
    FILMIC_DEFAULT.0
}
fn f_mid() -> f32 {
    FILMIC_DEFAULT.1
}
fn f_contrast() -> f32 {
    FILMIC_DEFAULT.2
}

/// Filmic curve defaults (white, mid, contrast), fitted to DaVinci (see module docs).
pub const FILMIC_DEFAULT: (f32, f32, f32) = (1.034, -1.55, 1.86);

/// How out-of-gamut colours are handled after the matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GamutMapping {
    None,
    /// Compress colours beyond 80 % of the gamut boundary towards the achromatic axis (the
    /// ACES Reference Gamut Compression shape `filmcraft-color` uses on input).
    Compress,
}

/// A CST node's parameters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cst {
    pub input_gamut: Gamut,
    pub input_curve: Curve,
    pub output_gamut: Gamut,
    pub output_curve: Curve,
    pub tone_mapping: ToneMapping,
    pub gamut_mapping: GamutMapping,
}

impl Default for Cst {
    /// The KODA powergrade's CST: S-Gamut3.Cine / S-Log3 → Rec.709 / Gamma 2.4, tone mapped.
    fn default() -> Self {
        let (white, mid, contrast) = FILMIC_DEFAULT;
        Cst {
            input_gamut: Gamut::SGamut3Cine,
            input_curve: Curve::SLog3,
            output_gamut: Gamut::Rec709,
            output_curve: Curve::Gamma24,
            tone_mapping: ToneMapping::Filmic { white, mid, contrast },
            // DaVinci's frame matched slightly better without our compression (ΔE 0.706 vs 0.713)
            gamut_mapping: GamutMapping::None,
        }
    }
}

/// The filmic curve: scene luminance → display-linear luminance.
#[inline]
pub fn filmic(y: f32, white: f32, mid: f32, contrast: f32) -> f32 {
    if y <= 0.0 {
        return 0.0;
    }
    let c = contrast.max(0.05);
    let code = white / (1.0 + (-(y.log2() - mid) / c).exp());
    code.max(0.0).powf(2.4)
}

/// Gamut compression (per channel distance from the achromatic axis), as in `filmcraft-color`.
#[inline]
pub fn compress(c: [f32; 3]) -> [f32; 3] {
    const TH: f32 = 0.8;
    const LIM: f32 = 1.25;
    const P: f32 = 1.2;
    let ach = c[0].max(c[1]).max(c[2]);
    if ach <= 1e-9 {
        return c;
    }
    let s = (LIM - TH) / (((LIM - TH) / (1.0 - TH)).powf(P) - 1.0).powf(1.0 / P);
    c.map(|v| {
        let d = (ach - v) / ach;
        if d < TH {
            return v;
        }
        let x = (d - TH) / s;
        let dc = TH + s * x / (1.0 + x.powf(P)).powf(1.0 / P);
        ach - dc * ach
    })
}

impl Cst {
    /// The 3×3 matrix input gamut → output gamut (row-major).
    pub fn matrix(&self) -> [[f32; 3]; 3] {
        let m = filmcraft_color::spaces::gamut_matrix(self.input_gamut.color(), self.output_gamut.color());
        m.map(|r| r.map(|v| v as f32))
    }

    /// One pixel's signal through the transform.
    pub fn apply(&self, c: [f32; 3], m: &[[f32; 3]; 3]) -> [f32; 3] {
        let l = c.map(|v| self.input_curve.decode(v));
        let mut o = [
            m[0][0] * l[0] + m[0][1] * l[1] + m[0][2] * l[2],
            m[1][0] * l[0] + m[1][1] * l[1] + m[1][2] * l[2],
            m[2][0] * l[0] + m[2][1] * l[1] + m[2][2] * l[2],
        ];
        if let ToneMapping::Filmic { white, mid, contrast } = self.tone_mapping {
            o = o.map(|v| filmic(v, white, mid, contrast));
        }
        if self.gamut_mapping == GamutMapping::Compress {
            o = compress(o);
        }
        o.map(|v| self.output_curve.encode(v))
    }

    /// The GPU parameters: `i1` = (node type, input curve, output curve, flags) with flags bit 0 =
    /// tone mapping, bit 1 = gamut compression; `p` = matrix (9), unused (3), white, mid, contrast.
    pub fn gpu(&self) -> ([u32; 4], [f32; 24]) {
        let m = self.matrix();
        let (tone, white, mid, contrast) = match self.tone_mapping {
            ToneMapping::None => (0, 0.0, 0.0, 1.0),
            ToneMapping::Filmic { white, mid, contrast } => (1, white, mid, contrast.max(0.05)),
        };
        let flags = tone | if self.gamut_mapping == GamutMapping::Compress { 2 } else { 0 };
        let mut p = [0.0; 24];
        for (i, v) in m.iter().flatten().enumerate() {
            p[i] = *v;
        }
        p[12] = white;
        p[13] = mid;
        p[14] = contrast;
        ([crate::eval::NODE_CST, self.input_curve.code(), self.output_curve.code(), flags], p)
    }

    /// Clamp the tone curve parameters to sane ranges (hostile projects / commands).
    pub fn sanitized(&self) -> Self {
        let mut c = *self;
        if let ToneMapping::Filmic { white, mid, contrast } = c.tone_mapping {
            let fin = |v: f32, d: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
            c.tone_mapping = ToneMapping::Filmic {
                white: fin(white, FILMIC_DEFAULT.0, 0.1, 4.0),
                mid: fin(mid, FILMIC_DEFAULT.1, -10.0, 10.0),
                contrast: fin(contrast, FILMIC_DEFAULT.2, 0.05, 10.0),
            };
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slog3_mid_grey_and_round_trips() {
        // Sony: 18 % grey = code 420 / 1023
        assert!((Curve::SLog3.decode(420.0 / 1023.0) - 0.18).abs() < 1e-4);
        for c in [Curve::SLog3, Curve::Gamma24, Curve::Gamma22, Curve::Srgb, Curve::Linear] {
            for v in [0.0f32, 0.1, 0.4, 0.8, 1.0] {
                assert!((c.encode(c.decode(v)) - v).abs() < 1e-4, "{c:?} {v}");
            }
        }
    }

    #[test]
    fn identity_cst_is_identity() {
        let c = Cst {
            input_gamut: Gamut::Rec709,
            input_curve: Curve::Gamma24,
            output_gamut: Gamut::Rec709,
            output_curve: Curve::Gamma24,
            tone_mapping: ToneMapping::None,
            gamut_mapping: GamutMapping::None,
        };
        let m = c.matrix();
        for px in [[0.2f32, 0.5, 0.9], [1.2, -0.1, 0.0]] {
            let o = c.apply(px, &m);
            for i in 0..3 {
                assert!((o[i] - px[i]).abs() < 1e-5, "{px:?} -> {o:?}");
            }
        }
    }

    #[test]
    fn default_cst_puts_mid_grey_where_davinci_does() {
        // S-Log3 18 % grey (code 420) comes out around display code 0.40 (fitted to DaVinci)
        let c = Cst::default();
        let o = c.apply([420.0 / 1023.0; 3], &c.matrix());
        assert!((o[0] - 0.40).abs() < 0.02 && (o[0] - o[1]).abs() < 1e-3 && (o[1] - o[2]).abs() < 1e-3, "{o:?}");
        // monotonic in exposure, bounded by white
        let mut last = 0.0;
        for k in 0..60 {
            let v = 95.0 / 1023.0 + k as f32 * 0.012;
            let y = c.apply([v; 3], &c.matrix())[1];
            assert!(y >= last - 1e-6 && y.is_finite() && y <= 1.05, "{v} -> {y}");
            last = y;
        }
    }

    #[test]
    fn hostile_tone_parameters_are_clamped() {
        let c = Cst { tone_mapping: ToneMapping::Filmic { white: f32::NAN, mid: 1e9, contrast: 0.0 }, ..Cst::default() }.sanitized();
        let ToneMapping::Filmic { white, mid, contrast } = c.tone_mapping else { panic!() };
        assert!(white.is_finite() && mid <= 10.0 && contrast >= 0.05);
        let o = c.apply([0.5; 3], &c.matrix());
        assert!(o.iter().all(|v| v.is_finite()));
    }
}

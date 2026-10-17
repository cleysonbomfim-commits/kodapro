//! Spatial nodes (aba Cor marco 4): sharpening (node 03 "nitidez" of the KODA powergrade) and
//! glow (node 04). Both read neighbouring pixels, so each runs as a few GPU passes (see
//! [`crate::eval`]) and as the same steps on the CPU.
//!
//! The defaults are the KODA powergrade's own nodes, **fitted to DaVinci's output** (black-box,
//! frames 02 → 03 → 04; `examples/calibra_marco4.rs`). DaVinci's sharpening and its Glow are not
//! published; the models here are an unsharp mask on the signal and a thresholded, blurred copy
//! of the picture composited back (Soft Light, Screen, Add or Normal), with a highlight recovery
//! that rolls values above white back under it.

use serde::{Deserialize, Serialize};

/// Largest Gaussian radius (taps each side) a sharpening pass reads.
pub const MAX_SHARPEN_TAPS: u32 = 32;
/// Largest Gaussian radius (taps each side) of the glow's blur, on its reduced copy.
pub const MAX_GLOW_TAPS: u32 = 96;

/// Unsharp mask on the signal: `x + amount · (x − Gaussian_radius(x))`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sharpen {
    /// Strength (0 = off).
    pub amount: f32,
    /// Gaussian sigma in clip pixels.
    pub radius: f32,
}

impl Default for Sharpen {
    /// Node 03 of the KODA powergrade (fitted: ΔE 0.38 → 0.14 on a 540 × 960 crop).
    fn default() -> Self {
        Sharpen { amount: 0.9, radius: 0.8 }
    }
}

pub mod range {
    pub const SHARPEN_AMOUNT: (f32, f32) = (0.0, 5.0);
    pub const SHARPEN_RADIUS: (f32, f32) = (0.2, 10.0);
    pub const THRESHOLD: (f32, f32) = (0.0, 1.0);
    pub const SPREAD: (f32, f32) = (1.0, 1000.0);
    pub const GAIN: (f32, f32) = (0.0, 4.0);
    pub const SATURATION: (f32, f32) = (0.0, 4.0);
    pub const GAMMA: (f32, f32) = (0.2, 5.0);
    pub const UNIT: (f32, f32) = (0.0, 1.0);
}

fn cl(x: f32, r: (f32, f32), d: f32) -> f32 {
    if x.is_nan() { d } else { x.clamp(r.0, r.1) }
}

fn num(name: &str, v: &serde_json::Value) -> Result<f32, String> {
    v.as_f64().filter(|x| x.is_finite()).map(|x| x as f32).ok_or_else(|| format!("{name}: expected a finite number"))
}

impl Sharpen {
    pub fn sanitized(&self) -> Self {
        let d = Sharpen::default();
        Sharpen { amount: cl(self.amount, range::SHARPEN_AMOUNT, d.amount), radius: cl(self.radius, range::SHARPEN_RADIUS, d.radius) }
    }

    pub fn is_identity(&self) -> bool {
        self.amount <= 0.0
    }

    pub fn set_param(&mut self, name: &str, v: &serde_json::Value) -> Result<(), String> {
        match name {
            "amount" => self.amount = num(name, v)?,
            "radius" => self.radius = num(name, v)?,
            other => return Err(format!("sharpen node has no parameter \"{other}\" (amount, radius)")),
        }
        *self = self.sanitized();
        Ok(())
    }

    /// The Gaussian at `px_scale` working pixels per clip pixel: (sigma, taps each side).
    pub fn kernel(&self, px_scale: f32) -> (f32, u32) {
        let sigma = (self.radius * px_scale).max(0.05);
        let taps = (3.0 * sigma).ceil().clamp(1.0, MAX_SHARPEN_TAPS as f32) as u32;
        (sigma, taps)
    }
}

/// Normalized Gaussian weights for taps −n…n (the CPU and the shader use the same formula).
pub fn gauss_weights(sigma: f32, n: u32) -> Vec<f32> {
    let n = n.min(MAX_GLOW_TAPS) as i32;
    let w: Vec<f32> = (-n..=n).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = w.iter().sum();
    w.into_iter().map(|x| x / s.max(1e-12)).collect()
}

/// How the glow goes back over the picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlowStyle {
    #[default]
    SoftLight,
    Screen,
    Add,
    Normal,
}

impl GlowStyle {
    pub fn code(self) -> u32 {
        match self {
            GlowStyle::SoftLight => 0,
            GlowStyle::Screen => 1,
            GlowStyle::Add => 2,
            GlowStyle::Normal => 3,
        }
    }
}

/// Glow: the picture above `threshold` (signal luma, soft edge), blurred by `spread`, mixed with
/// a `core` share of the unblurred source, saturated, raised to `gamma`, times `gain`, then
/// composited with `style` at `opacity`; `highlight_recovery` rolls the result back under white.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Glow {
    pub style: GlowStyle,
    pub threshold: f32,
    /// Gaussian sigma in clip pixels.
    pub spread: f32,
    pub gain: f32,
    pub saturation: f32,
    pub gamma: f32,
    /// Share of the glow taken from the unblurred source (0–1).
    pub core: f32,
    pub opacity: f32,
    pub highlight_recovery: f32,
}

impl Default for Glow {
    /// Node 04 of the KODA powergrade (fitted at 1/4 resolution: ΔE 3.77 → 0.81).
    fn default() -> Self {
        Glow {
            style: GlowStyle::SoftLight,
            threshold: 0.238,
            spread: 115.0,
            gain: 0.9,
            saturation: 1.45,
            gamma: 1.063,
            core: 0.25,
            opacity: 0.426,
            highlight_recovery: 0.0,
        }
    }
}

impl Glow {
    pub fn sanitized(&self) -> Self {
        let d = Glow::default();
        Glow {
            style: self.style,
            threshold: cl(self.threshold, range::THRESHOLD, d.threshold),
            spread: cl(self.spread, range::SPREAD, d.spread),
            gain: cl(self.gain, range::GAIN, d.gain),
            saturation: cl(self.saturation, range::SATURATION, d.saturation),
            gamma: cl(self.gamma, range::GAMMA, d.gamma),
            core: cl(self.core, range::UNIT, d.core),
            opacity: cl(self.opacity, range::UNIT, d.opacity),
            highlight_recovery: cl(self.highlight_recovery, range::UNIT, 0.0),
        }
    }

    pub fn is_identity(&self) -> bool {
        self.opacity <= 0.0 || (self.gain <= 0.0 && self.style != GlowStyle::SoftLight && self.style != GlowStyle::Normal)
    }

    pub fn set_param(&mut self, name: &str, v: &serde_json::Value) -> Result<(), String> {
        match name {
            "style" => {
                self.style = v
                    .as_str()
                    .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
                    .ok_or("style: \"softLight\", \"screen\", \"add\" or \"normal\"")?
            }
            "threshold" => self.threshold = num(name, v)?,
            "spread" => self.spread = num(name, v)?,
            "gain" => self.gain = num(name, v)?,
            "saturation" => self.saturation = num(name, v)?,
            "gamma" => self.gamma = num(name, v)?,
            "core" => self.core = num(name, v)?,
            "opacity" => self.opacity = num(name, v)?,
            "highlightRecovery" => self.highlight_recovery = num(name, v)?,
            other => {
                return Err(format!(
                    "glow node has no parameter \"{other}\" (style, threshold, spread, gain, saturation, gamma, core, opacity, highlightRecovery)"
                ));
            }
        }
        *self = self.sanitized();
        Ok(())
    }

    /// How the blur runs at `px_scale`: (reduction factor k, sigma on the 1/k copy, taps each
    /// side). A wide glow is blurred on a copy reduced k times (k × k block averages), then
    /// enlarged bilinearly: the same look for a fraction of the work (marco 7).
    pub fn blur_plan(&self, px_scale: f32) -> (u32, f32, u32) {
        let sigma = (self.spread * px_scale).max(0.3);
        let k: u32 = if sigma < 8.0 {
            1
        } else if sigma < 24.0 {
            2
        } else if sigma < 64.0 {
            4
        } else {
            8
        };
        let s = sigma / k as f32;
        let taps = (3.0 * s).ceil().clamp(1.0, MAX_GLOW_TAPS as f32) as u32;
        (k, s, taps)
    }

    /// The source mask of a pixel's signal: 1 above the threshold (soft over ±0.15 of luma).
    #[inline]
    pub fn mask(&self, c: [f32; 3]) -> f32 {
        if self.threshold <= 0.0 {
            return 1.0;
        }
        smoothstep(self.threshold - 0.15, self.threshold + 0.15, luma(c))
    }

    /// The glow over one pixel: `x` the picture's signal, `blur` the blurred source, `src` the
    /// unblurred source (both unpremultiplied signals).
    #[inline]
    pub fn composite(&self, x: [f32; 3], blur: [f32; 3], src: [f32; 3]) -> [f32; 3] {
        let k = self.core;
        let g = [0, 1, 2].map(|i| blur[i] * (1.0 - k) + src[i] * k);
        let y = luma(g);
        let g = g.map(|v| (y + self.saturation * (v - y)).max(0.0).powf(self.gamma) * self.gain);
        let out = [0, 1, 2].map(|i| {
            let (b, t) = (x[i], g[i]);
            let c = match self.style {
                // the picture as soft light over the glow (fits DaVinci's Soft Light best)
                GlowStyle::SoftLight => soft_light(t, b),
                GlowStyle::Screen => 1.0 - (1.0 - b) * (1.0 - t),
                GlowStyle::Add => b + t,
                GlowStyle::Normal => t,
            };
            b + (c - b) * self.opacity
        });
        out.map(|v| recover(v, self.highlight_recovery))
    }
}

/// W3C soft light of `source` over `backdrop`.
#[inline]
pub fn soft_light(backdrop: f32, source: f32) -> f32 {
    let (b, s) = (backdrop, source);
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.max(0.0).sqrt() };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

/// Highlight recovery: values above 0.8 roll off towards 1 (amount 0–1 mixes it in).
#[inline]
pub fn recover(v: f32, amount: f32) -> f32 {
    const KNEE: f32 = 0.8;
    if amount <= 0.0 || v <= KNEE {
        return v;
    }
    let r = KNEE + (1.0 - KNEE) * (1.0 - (-(v - KNEE) / (1.0 - KNEE)).exp());
    v + (r - v) * amount
}

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    crate::primaries::LUMA[0] * c[0] + crate::primaries::LUMA[1] * c[1] + crate::primaries::LUMA[2] * c[2]
}

#[inline]
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_params() {
        let mut s = Sharpen::default();
        assert!(!s.is_identity());
        s.set_param("amount", &serde_json::json!(99)).unwrap();
        assert_eq!(s.amount, 5.0);
        assert!(s.set_param("radius", &serde_json::json!("x")).is_err());
        assert!(s.set_param("nope", &serde_json::json!(1)).is_err());
        let (sigma, taps) = Sharpen { amount: 1.0, radius: 10.0 }.kernel(4.0);
        assert!(sigma > 39.0 && taps == MAX_SHARPEN_TAPS);
        let w = gauss_weights(0.8, 3);
        assert_eq!(w.len(), 7);
        assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5);

        let mut g = Glow::default();
        g.set_param("style", &serde_json::json!("screen")).unwrap();
        assert_eq!(g.style, GlowStyle::Screen);
        assert!(g.set_param("style", &serde_json::json!("multiply")).is_err());
        g.set_param("spread", &serde_json::json!(-5)).unwrap();
        assert_eq!(g.spread, 1.0);
        g.set_param("opacity", &serde_json::json!(f64::MAX)).unwrap();
        assert_eq!(g.opacity, 1.0);
        assert!(g.set_param("gain", &serde_json::json!(null)).is_err());
    }

    #[test]
    fn styles_and_recovery() {
        let x = [0.5f32, 0.4, 0.3];
        let base = Glow { opacity: 1.0, core: 0.0, saturation: 1.0, gamma: 1.0, gain: 1.0, ..Default::default() };
        let add = Glow { style: GlowStyle::Add, ..base }.composite(x, [0.2; 3], [0.0; 3]);
        assert!((add[0] - 0.7).abs() < 1e-5);
        let screen = Glow { style: GlowStyle::Screen, ..base }.composite(x, [0.2; 3], [0.0; 3]);
        assert!(screen[0] > 0.5 && screen[0] < 0.7);
        let normal = Glow { style: GlowStyle::Normal, ..base }.composite(x, [0.2; 3], [0.0; 3]);
        assert!((normal[1] - 0.2).abs() < 1e-5);
        // soft light with a mid-grey glow leaves the picture alone
        let sl = Glow { style: GlowStyle::SoftLight, ..base }.composite(x, [0.5; 3], [0.0; 3]);
        assert!(sl.iter().all(|v| v.is_finite()));
        // recovery keeps values under ~1 and leaves the rest alone
        assert_eq!(recover(0.5, 1.0), 0.5);
        assert!(recover(3.0, 1.0) < 1.0 && recover(3.0, 1.0) > 0.95);
        assert_eq!(recover(3.0, 0.0), 3.0);
        // the mask
        let g = Glow::default();
        assert_eq!(g.mask([0.0; 3]), 0.0);
        assert_eq!(g.mask([1.0; 3]), 1.0);
        assert_eq!(Glow { threshold: 0.0, ..g }.mask([0.0; 3]), 1.0);
    }
}

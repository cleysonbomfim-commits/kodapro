//! Colour difference for the "same picture as DaVinci" criterion (ESPEC_COR_V1: mean ΔE 2000
//! below 1, maximum below 3). Pixels are Rec.709 / Gamma 2.4 display signals (DaVinci's output
//! space): decoded with the 2.4 power, converted to XYZ with the BT.709 / D65 matrix, then to
//! CIE L*a*b* (D65), and compared with CIEDE2000 (CIE 142-2001; implementation notes and test
//! data: G. Sharma, W. Wu, E. N. Dalal, 2005).

/// Rec.709 / Gamma 2.4 signal → CIE L*a*b* (D65).
pub fn lab_from_rec709_g24(c: [f32; 3]) -> [f64; 3] {
    let l = c.map(|v| (v.clamp(0.0, 1.0) as f64).powf(2.4));
    // BT.709 primaries, D65 white
    let x = 0.412_390_8 * l[0] + 0.357_584_3 * l[1] + 0.180_480_8 * l[2];
    let y = 0.212_639 * l[0] + 0.715_168_7 * l[1] + 0.072_192_3 * l[2];
    let z = 0.019_330_8 * l[0] + 0.119_194_8 * l[1] + 0.950_532_2 * l[2];
    lab_from_xyz([x, y, z])
}

fn lab_from_xyz(xyz: [f64; 3]) -> [f64; 3] {
    const WHITE: [f64; 3] = [0.950_47, 1.0, 1.088_83];
    let f = |t: f64| if t > (6.0f64 / 29.0).powi(3) { t.cbrt() } else { t / (3.0 * (6.0f64 / 29.0).powi(2)) + 4.0 / 29.0 };
    let (fx, fy, fz) = (f(xyz[0] / WHITE[0]), f(xyz[1] / WHITE[1]), f(xyz[2] / WHITE[2]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference between two L*a*b* colours (kL = kC = kH = 1).
pub fn ciede2000(a: [f64; 3], b: [f64; 3]) -> f64 {
    use std::f64::consts::PI;
    let deg = |r: f64| r * 180.0 / PI;
    let rad = |d: f64| d * PI / 180.0;
    let (l1, a1, b1) = (a[0], a[1], a[2]);
    let (l2, a2, b2) = (b[0], b[1], b[2]);
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let cm = (c1 + c2) / 2.0;
    let g = 0.5 * (1.0 - (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = ((a1p * a1p + b1 * b1).sqrt(), (a2p * a2p + b2 * b2).sqrt());
    let hue = |bb: f64, ap: f64| {
        if bb == 0.0 && ap == 0.0 {
            0.0
        } else {
            let h = deg(bb.atan2(ap));
            if h < 0.0 { h + 360.0 } else { h }
        }
    };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dhh = 2.0 * (c1p * c2p).sqrt() * rad(dh / 2.0).sin();
    let lm = (l1 + l2) / 2.0;
    let cmp = (c1p + c2p) / 2.0;
    let hm = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * rad(hm - 30.0).cos() + 0.24 * rad(2.0 * hm).cos() + 0.32 * rad(3.0 * hm + 6.0).cos() - 0.20 * rad(4.0 * hm - 63.0).cos();
    let dtheta = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cmp.powi(7) / (cmp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cmp;
    let sh = 1.0 + 0.015 * cmp * t;
    let rt = -rad(2.0 * dtheta).sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dhh / sh).powi(2) + rt * (dc / sc) * (dhh / sh)).sqrt()
}

/// ΔE 2000 over two images of the same size (Rec.709 / Gamma 2.4 signals).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    pub mean: f64,
    pub p99: f64,
    pub max: f64,
    pub pixels: usize,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ΔE2000 média {:.3}, p99 {:.3}, máxima {:.3} ({} pixels)", self.mean, self.p99, self.max, self.pixels)
    }
}

/// Per-pixel ΔE 2000 between `a` and `b`.
pub fn per_pixel(a: &[[f32; 3]], b: &[[f32; 3]]) -> Vec<f64> {
    a.iter().zip(b).map(|(x, y)| ciede2000(lab_from_rec709_g24(*x), lab_from_rec709_g24(*y))).collect()
}

/// Mean, 99th percentile and maximum ΔE 2000 between `a` and `b`.
pub fn compare(a: &[[f32; 3]], b: &[[f32; 3]]) -> Stats {
    let mut d = per_pixel(a, b);
    let n = d.len();
    if n == 0 {
        return Stats { mean: 0.0, p99: 0.0, max: 0.0, pixels: 0 };
    }
    let mean = d.iter().sum::<f64>() / n as f64;
    d.sort_unstable_by(f64::total_cmp);
    Stats { mean, p99: d.get(n * 99 / 100).copied().unwrap_or(0.0), max: d.last().copied().unwrap_or(0.0), pixels: n }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test pairs from Sharma, Wu and Dalal (2005), table 1.
    #[test]
    fn sharma_reference_pairs() {
        let pairs = [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 3.1571, -77.2803], [50.0, 0.0, -82.7485], 2.8615),
            ([50.0, 2.8361, -74.0200], [50.0, 0.0, -82.7485], 3.4412),
            ([50.0, -1.3802, -84.2814], [50.0, 0.0, -82.7485], 1.0000),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
            ([60.2574, -34.0099, 36.2677], [60.4626, -34.1751, 39.4387], 1.2644),
            ([22.7233, 20.0904, -46.6940], [23.0331, 14.9730, -42.5619], 2.0373),
            ([90.8027, -2.0831, 1.4410], [91.1528, -1.6435, 0.0447], 1.4441),
            ([2.0776, 0.0795, -1.1350], [0.9033, -0.0636, -0.5514], 0.9082),
        ];
        for (a, b, want) in pairs {
            let got = ciede2000(a, b);
            assert!((got - want).abs() < 1e-4, "{a:?} {b:?}: {got} vs {want}");
        }
    }

    #[test]
    fn identical_images_have_zero_difference() {
        let a = vec![[0.2f32, 0.5, 0.9], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
        let s = compare(&a, &a);
        assert_eq!((s.mean, s.max), (0.0, 0.0));
        // white is L* 100, black 0
        assert!((lab_from_rec709_g24([1.0; 3])[0] - 100.0).abs() < 1e-3);
    }
}

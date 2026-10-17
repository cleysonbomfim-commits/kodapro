//! Evaluating a grade: the CPU reference ([`apply_cpu`]) and the GPU passes ([`gpu_passes`],
//! run by `filmcraft-gpu` with [`WGSL`]). Both work on the compositor's working image (linear
//! premultiplied RGBA f32) and must agree: the CPU is the oracle, the GPU the fast path.
//!
//! **Signal.** Nodes grade the picture as a colourist sees it, not linear light: each pixel is
//! un-premultiplied, encoded to the grading signal ([`to_signal`]), passed through the nodes,
//! then decoded and premultiplied again. Values are never clipped between nodes (32-bit float,
//! negatives and values above 1 survive). The signal is the inverse of FilmCraft's decode of
//! Rec.709-tagged media (the sRGB curve), so a camera clip left at its default interpretation
//! reaches the nodes as its own code values: S-Log3 from a Sony FX30, as in DaVinci. Whatever the
//! last node outputs is what the monitor shows (it encodes with the same curve).

use std::collections::HashMap;
use std::sync::Arc;

use filmcraft_color::Lut3d;
use filmcraft_render::Image;
use filmcraft_render::external::{AuxImage, BoxPass, GpuPass};

use crate::grade::{Grade, Interpolation, NodeKind};

/// Linear light → grading signal: the sRGB transfer function, extended to negatives by odd
/// symmetry and above 1 by the same power curve (no clipping).
#[inline]
pub fn to_signal(x: f32) -> f32 {
    let a = x.abs();
    let s = if a <= 0.003_130_8 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 };
    s.copysign(x)
}

/// Grading signal → linear light, the inverse of [`to_signal`].
#[inline]
pub fn to_linear(x: f32) -> f32 {
    let a = x.abs();
    let l = if a <= 0.040_45 { a / 12.92 } else { ((a + 0.055) / 1.055).powf(2.4) };
    l.copysign(x)
}

/// A LUT ready to evaluate: the cube (CPU) and the same cube unfolded into a 2D image for the
/// GPU (`size² × size` texels, x = r + g·size, y = b: the cube's own order).
#[derive(Debug)]
pub struct LoadedLut {
    pub cube: Lut3d,
    pub aux: Arc<AuxImage>,
}

impl LoadedLut {
    /// `None` for a cube that cannot be sampled safely (fewer than 2 points a side, a short
    /// table, a degenerate or non-finite domain).
    pub fn new(cube: Lut3d) -> Option<Self> {
        let n = cube.size;
        let ok_domain = (0..3).all(|c| cube.domain_min[c].is_finite() && cube.domain_max[c].is_finite() && cube.domain_max[c] > cube.domain_min[c]);
        if !(2..=129).contains(&n) || cube.data.len() != n.checked_pow(3)? || !ok_domain {
            return None;
        }
        let rgba = cube.data.iter().flat_map(|c| [c[0], c[1], c[2], 1.0]).collect();
        let aux = Arc::new(AuxImage { w: (n * n) as u32, h: n as u32, rgba });
        Some(LoadedLut { cube, aux })
    }
}

/// LUTs by reference (`lib:<id>`), resolved from the project before a frame is graded.
pub type Luts = HashMap<String, Arc<LoadedLut>>;

/// A node ready to run on pixels.
enum Ready {
    Gain([f32; 3]),
    Cst(crate::cst::Cst, [[f32; 3]; 3]),
    Lut(Arc<LoadedLut>, f32, Interpolation),
    Prim(Arc<crate::primaries::Ready>, Arc<AuxImage>),
}

/// A step of the grade: per-pixel nodes (fused on the CPU, one GPU pass each), or a spatial
/// part of a node that reads neighbouring pixels.
enum Stage {
    Pixel(Vec<Ready>),
    /// Mid Detail of a primaries node: the image against its Gaussian (box radii per axis).
    MidDetail {
        amount: f32,
        radii: Vec<u32>,
    },
    /// Unsharp mask on the signal, an explicit Gaussian of `taps` each side.
    Sharpen {
        amount: f32,
        sigma: f32,
        taps: u32,
    },
    /// Glow: the masked source reduced `k` times, blurred (Gaussian `sigma`, `taps` each side),
    /// enlarged and composited back.
    Glow {
        glow: crate::spatial::Glow,
        k: u32,
        sigma: f32,
        taps: u32,
    },
}

/// Box radii of a Gaussian of `sigma` working-image pixels (the same on the CPU and the GPU).
fn gauss_radii(sigma: f32) -> Vec<u32> {
    filmcraft_render::effects::gaussian_boxes(1 << 16, 1 << 16, sigma, sigma).0
}

/// The active nodes of `grade` as stages, at `px_scale` working pixels per clip pixel. A LUT
/// node whose LUT is not loaded changes nothing (on the CPU and the GPU alike).
fn stages(grade: &Grade, luts: &Luts, px_scale: f32) -> Vec<Stage> {
    let mut out: Vec<Stage> = Vec::new();
    if grade.is_identity() {
        return out;
    }
    let push = |out: &mut Vec<Stage>, r: Ready| match out.last_mut() {
        Some(Stage::Pixel(v)) => v.push(r),
        _ => out.push(Stage::Pixel(vec![r])),
    };
    let scale = if px_scale.is_finite() && px_scale > 0.0 { px_scale.min(16.0) } else { 1.0 };
    for n in grade.active() {
        match &n.kind {
            NodeKind::Gain { master, rgb } => push(&mut out, Ready::Gain(rgb.map(|v| v * master))),
            NodeKind::Cst(c) => push(&mut out, Ready::Cst(*c, c.matrix())),
            NodeKind::Lut { lut, intensity, interpolation } => {
                if *intensity > 0.0
                    && let Some(l) = luts.get(lut)
                {
                    push(&mut out, Ready::Lut(l.clone(), *intensity, *interpolation));
                }
            }
            NodeKind::Primaries(p) => {
                if p.mid_detail != 0.0 {
                    let radii = gauss_radii(crate::primaries::MID_DETAIL_SIGMA * scale);
                    if !radii.is_empty() {
                        out.push(Stage::MidDetail { amount: p.mid_detail / 100.0, radii });
                    }
                }
                if !p.pixel_identity() {
                    let r = p.ready();
                    let aux = Arc::new(r.aux());
                    push(&mut out, Ready::Prim(Arc::new(r), aux));
                }
            }
            NodeKind::Sharpen(x) => {
                let (sigma, taps) = x.kernel(scale);
                out.push(Stage::Sharpen { amount: x.amount, sigma, taps });
            }
            NodeKind::Glow(x) => {
                let (k, sigma, taps) = x.blur_plan(scale);
                out.push(Stage::Glow { glow: *x, k, sigma, taps });
            }
        }
    }
    out
}

#[inline]
fn run(r: &Ready, c: [f32; 3]) -> [f32; 3] {
    match r {
        Ready::Gain(g) => [c[0] * g[0], c[1] * g[1], c[2] * g[2]],
        Ready::Cst(cst, m) => cst.apply(c, m),
        Ready::Lut(l, k, interp) => {
            let v = match interp {
                Interpolation::Trilinear => l.cube.apply_trilinear(c),
                Interpolation::Tetrahedral => l.cube.apply(c),
            };
            [c[0] + (v[0] - c[0]) * k, c[1] + (v[1] - c[1]) * k, c[2] + (v[2] - c[2]) * k]
        }
        Ready::Prim(p, _) => p.apply(c),
    }
}

/// Apply the per-pixel nodes of `grade` to one pixel's signal (no premultiplication), in order.
/// Spatial parts (Mid Detail) need the whole picture and are left out here.
pub fn apply_signal(grade: &Grade, luts: &Luts, c: [f32; 3]) -> [f32; 3] {
    stages(grade, luts, 1.0).iter().fold(c, |c, s| match s {
        Stage::Pixel(v) => v.iter().fold(c, |c, r| run(r, c)),
        Stage::MidDetail { .. } | Stage::Sharpen { .. } | Stage::Glow { .. } => c,
    })
}

#[inline]
fn unpremul_signal(p: &[f32]) -> Option<[f32; 3]> {
    let a = *p.get(3)?;
    (a > 0.0).then(|| [to_signal(p[0] / a), to_signal(p[1] / a), to_signal(p[2] / a)])
}

/// Separable Gaussian (explicit taps, edges clamped) of a premultiplied image, all 4 channels:
/// the CPU twin of the shader's `gauss_pass`.
fn gauss_cpu(img: &Image, sigma: f32, taps: u32) -> Image {
    use rayon::prelude::*;
    let k = crate::spatial::gauss_weights(sigma, taps);
    let n = taps.min(crate::spatial::MAX_GLOW_TAPS) as isize;
    let (w, h) = (img.w, img.h);
    let pass = |src: &Image, vertical: bool| -> Image {
        let mut out = Image { w, h, px: vec![0.0; src.px.len()] };
        out.px.par_chunks_mut(w * 4).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let mut acc = [0f32; 4];
                for (j, kk) in k.iter().enumerate() {
                    let d = j as isize - n;
                    let (sx, sy) = if vertical {
                        (x, (y as isize + d).clamp(0, h as isize - 1) as usize)
                    } else {
                        ((x as isize + d).clamp(0, w as isize - 1) as usize, y)
                    };
                    if let Some(p) = src.px.get((sy * w + sx) * 4..(sy * w + sx) * 4 + 4) {
                        for c in 0..4 {
                            acc[c] += p[c] * kk;
                        }
                    }
                }
                if let Some(o) = row.get_mut(x * 4..x * 4 + 4) {
                    o.copy_from_slice(&acc);
                }
            }
        });
        out
    };
    if w == 0 || h == 0 {
        return img.clone();
    }
    pass(&pass(img, false), true)
}

/// The picture as premultiplied signal (what the sharpening blurs).
fn signal_image(img: &Image) -> Image {
    let mut out = img.clone();
    for p in out.px.as_chunks_mut::<4>().0 {
        let a = p[3];
        if a > 0.0 {
            for i in 0..3 {
                p[i] = to_signal(p[i] / a) * a;
            }
        } else {
            *p = [0.0; 4];
        }
    }
    out
}

/// Unpremultiplied colour of a premultiplied pixel (black when transparent).
#[inline]
fn unpremul(p: &[f32]) -> [f32; 3] {
    match p.get(3) {
        Some(&a) if a > 0.0 => [p[0] / a, p[1] / a, p[2] / a],
        _ => [0.0; 3],
    }
}

/// Apply `grade` to `img` (linear premultiplied RGBA f32) on the CPU, at `px_scale` working
/// pixels per clip pixel (spatial parts scale with it).
pub fn apply_cpu(grade: &Grade, luts: &Luts, img: &mut Image, px_scale: f32) {
    use rayon::prelude::*;
    for stage in stages(grade, luts, px_scale) {
        match stage {
            Stage::Pixel(nodes) => img.px.par_chunks_mut(4 * 1024).for_each(|chunk| {
                for p in chunk.as_chunks_mut::<4>().0 {
                    let a = p[3];
                    if a <= 0.0 {
                        continue;
                    }
                    let mut c = [to_signal(p[0] / a), to_signal(p[1] / a), to_signal(p[2] / a)];
                    for r in &nodes {
                        c = run(r, c);
                    }
                    for i in 0..3 {
                        p[i] = to_linear(c[i]) * a;
                    }
                }
            }),
            Stage::MidDetail { amount, radii } => {
                let orig = img.clone();
                filmcraft_render::effects::box_blur(img, &radii, &radii, true);
                img.px.par_chunks_mut(4).zip(orig.px.par_chunks(4)).for_each(|(p, o)| {
                    let a = o.get(3).copied().unwrap_or(0.0);
                    let (Some(oc), Some(bc)) = (unpremul_signal(o), unpremul_signal(p)) else {
                        p.copy_from_slice(o);
                        return;
                    };
                    let c = crate::primaries::mid_detail(oc, bc, amount);
                    for i in 0..3 {
                        p[i] = to_linear(c[i]) * a;
                    }
                    p[3] = a;
                });
            }
            Stage::Sharpen { amount, sigma, taps } => {
                let sig = signal_image(img);
                let blur = gauss_cpu(&sig, sigma, taps);
                img.px.par_chunks_mut(4).zip(sig.px.par_chunks(4)).zip(blur.px.par_chunks(4)).for_each(|((p, s), b)| {
                    let a = s.get(3).copied().unwrap_or(0.0);
                    if a <= 0.0 {
                        return;
                    }
                    let (x, g) = (unpremul(s), unpremul(b));
                    for i in 0..3 {
                        p[i] = to_linear(x[i] + amount * (x[i] - g[i])) * a;
                    }
                });
            }
            Stage::Glow { glow, k, sigma, taps } => {
                let blurred = gauss_cpu(&glow_down(img, &glow, k as usize), sigma, taps);
                let k = k as usize;
                img.px.par_chunks_mut(img.w.max(1) * 4).enumerate().for_each(|(y, row)| {
                    for (x, p) in row.chunks_mut(4).enumerate() {
                        let Some(xs) = unpremul_signal(p) else { continue };
                        let a = p[3];
                        let m = glow.mask(xs);
                        let g = unpremul(&bilinear(&blurred, (x as f32 + 0.5) / k as f32 - 0.5, (y as f32 + 0.5) / k as f32 - 0.5));
                        let c = glow.composite(xs, g, xs.map(|v| v * m));
                        for i in 0..3 {
                            p[i] = to_linear(c[i]) * a;
                        }
                    }
                });
            }
        }
    }
}

/// The glow's masked source (premultiplied signal) reduced `k` times: each pixel the mean of a
/// k × k block (the pixels of the block inside the picture). The CPU twin of `glow_down`.
fn glow_down(img: &Image, glow: &crate::spatial::Glow, k: usize) -> Image {
    use rayon::prelude::*;
    let k = k.max(1);
    let (w, h) = (img.w, img.h);
    let (rw, rh) = (w.div_ceil(k), h.div_ceil(k));
    let mut small = Image { w: rw, h: rh, px: vec![0.0; rw * rh * 4] };
    small.px.par_chunks_mut(rw.max(1) * 4).enumerate().for_each(|(y, row)| {
        for x in 0..rw {
            let mut acc = [0f32; 4];
            let mut n = 0f32;
            for yy in y * k..((y + 1) * k).min(h) {
                for xx in x * k..((x + 1) * k).min(w) {
                    let Some(p) = img.px.get((yy * w + xx) * 4..(yy * w + xx) * 4 + 4) else { continue };
                    n += 1.0;
                    if let Some(s) = unpremul_signal(p) {
                        let m = glow.mask(s);
                        for i in 0..3 {
                            acc[i] += s[i] * m * p[3];
                        }
                        acc[3] += p[3];
                    }
                }
            }
            if let Some(o) = row.get_mut(x * 4..x * 4 + 4) {
                for i in 0..4 {
                    o[i] = acc[i] / n.max(1.0);
                }
            }
        }
    });
    small
}

/// Bilinear sample of a premultiplied image at pixel-centre coordinates (edges clamped).
fn bilinear(img: &Image, fx: f32, fy: f32) -> [f32; 4] {
    let (w, h) = (img.w.max(1), img.h.max(1));
    let fx = fx.clamp(0.0, (w - 1) as f32);
    let fy = fy.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |x: usize, y: usize| -> [f32; 4] {
        let i = (y * w + x) * 4;
        img.px.get(i..i + 4).map_or([0.0; 4], |p| [p[0], p[1], p[2], p[3]])
    };
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    [0, 1, 2, 3].map(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

/// The compute shader the GPU passes run. It declares the bindings of `filmcraft-gpu`'s effect
/// stage (`fx.wgsl`: uniform `u`, `src`, `dst`, `aux`) and the entry point `ext_px`; `u.i1.x` is
/// the node type and `u.p0`… its parameters. Each pass un-premultiplies, encodes, applies one
/// node, decodes and premultiplies, like [`apply_cpu`] per node (encode and decode are inverses,
/// so a chain of passes equals the CPU's single encode, nodes, decode).
pub const WGSL: &str = include_str!("grade.wgsl");

/// Node type codes in [`WGSL`] (`u.i1.x`).
pub const NODE_GAIN: u32 = 1;
pub const NODE_CST: u32 = 2;
pub const NODE_LUT: u32 = 3;
pub const NODE_PRIMARIES: u32 = 4;
pub const NODE_MID_DETAIL: u32 = 5;
pub const NODE_TO_SIGNAL: u32 = 6;
pub const NODE_GAUSS: u32 = 7;
pub const NODE_SHARPEN: u32 = 8;
pub const NODE_GLOW_DOWN: u32 = 9;
pub const NODE_GLOW_BLUR: u32 = 11;
pub const NODE_GLOW: u32 = 10;

/// The GPU passes for `grade` at `px_scale` (none when it changes nothing).
pub fn gpu_passes(grade: &Grade, luts: &Luts, px_scale: f32) -> Vec<GpuPass> {
    let mut out = Vec::new();
    for stage in stages(grade, luts, px_scale) {
        match stage {
            Stage::Pixel(nodes) => out.extend(nodes.into_iter().map(pixel_pass)),
            Stage::MidDetail { amount, radii } => {
                blur_passes(&radii, &mut out);
                let mut p = [0.0; 24];
                p[0] = amount;
                out.push(GpuPass { i1: [NODE_MID_DETAIL, 0, 0, 0], p, combine: true, ..Default::default() });
            }
            Stage::Sharpen { amount, sigma, taps } => {
                let mut p = [0.0; 24];
                p[0] = sigma;
                out.push(GpuPass { i1: [NODE_TO_SIGNAL, 0, 0, 0], ..Default::default() });
                out.push(GpuPass { i1: [NODE_GAUSS, taps, 0, 0], p, save: true, ..Default::default() });
                out.push(GpuPass { i1: [NODE_GAUSS, taps, 1, 0], p, ..Default::default() });
                p[0] = amount;
                out.push(GpuPass { i1: [NODE_SHARPEN, 0, 0, 0], p, combine: true, ..Default::default() });
            }
            Stage::Glow { glow, k, sigma, taps } => {
                let mut p = [0.0; 24];
                p[..8].copy_from_slice(&[glow.threshold, glow.gain, glow.saturation, glow.gamma, glow.core, glow.opacity, glow.highlight_recovery, 0.0]);
                p[8] = sigma;
                out.push(GpuPass { i1: [NODE_GLOW_DOWN, 0, k, 0], p, save: true, ..Default::default() });
                out.push(GpuPass { i1: [NODE_GLOW_BLUR, taps, k, 0], p, ..Default::default() });
                out.push(GpuPass { i1: [NODE_GLOW_BLUR, taps, k, 1], p, ..Default::default() });
                out.push(GpuPass { i1: [NODE_GLOW, glow.style.code(), k, 0], p, combine: true, ..Default::default() });
            }
        }
    }
    out
}

/// Box passes along rows then columns; the first keeps the image it reads (for the combine).
fn blur_passes(radii: &[u32], out: &mut Vec<GpuPass>) {
    let first = out.len();
    for vertical in [false, true] {
        for &radius in radii.iter().filter(|r| **r > 0) {
            out.push(GpuPass { blur: Some(BoxPass { radius, vertical }), ..Default::default() });
        }
    }
    if let Some(p) = out.get_mut(first) {
        p.save = true;
    }
}

fn pixel_pass(r: Ready) -> GpuPass {
    match r {
        Ready::Gain(g) => {
            let mut p = [0.0; 24];
            p[..3].copy_from_slice(&g);
            GpuPass { i1: [NODE_GAIN, 0, 0, 0], p, ..Default::default() }
        }
        Ready::Cst(c, _) => {
            let (i1, p) = c.gpu();
            GpuPass { i1, p, ..Default::default() }
        }
        Ready::Lut(l, k, interp) => {
            let mut p = [0.0; 24];
            p[0] = k;
            p[4..7].copy_from_slice(&l.cube.domain_min);
            p[8..11].copy_from_slice(&l.cube.domain_max);
            let tet = u32::from(interp == Interpolation::Tetrahedral);
            GpuPass { i1: [NODE_LUT, tet, l.cube.size as u32, 0], p, aux: Some(l.aux.clone()), ..Default::default() }
        }
        Ready::Prim(r, aux) => GpuPass { i1: [NODE_PRIMARIES, r.table.len() as u32, 0, 0], p: [0.0; 24], aux: Some(aux), ..Default::default() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_is_invertible_without_clipping() {
        for x in [-2.0f32, -0.5, -0.001, 0.0, 0.002, 0.18, 0.5, 1.0, 1.7, 8.0] {
            let back = to_linear(to_signal(x));
            assert!((back - x).abs() <= 1e-5 * x.abs().max(1.0), "{x} -> {back}");
        }
        // mid grey 0.18 is ~0.46 in the signal; 1.0 stays 1.0
        assert!((to_signal(0.18) - 0.4613).abs() < 1e-3);
        assert!((to_signal(1.0) - 1.0).abs() < 1e-6);
        assert!(to_signal(4.0) > 1.0 && to_signal(-0.5) < 0.0);
    }

    #[test]
    fn gain_multiplies_the_signal_and_keeps_alpha() {
        let mut g = Grade::default();
        g.add(NodeKind::Gain { master: 2.0, rgb: [1.0, 0.5, 1.0] }, None, None).unwrap();
        // half-transparent mid grey, premultiplied; an over-white pixel; a negative one
        let mut img = Image { w: 3, h: 1, px: vec![0.09, 0.09, 0.09, 0.5, 2.0, 2.0, 2.0, 1.0, -0.1, 0.2, 0.0, 1.0] };
        apply_cpu(&g, &Luts::new(), &mut img, 1.0);
        let px = &img.px;
        let s = to_signal(0.18);
        let want = [to_linear(s * 2.0) * 0.5, to_linear(s) * 0.5, to_linear(s * 2.0) * 0.5, 0.5];
        for (a, b) in px[..4].iter().zip(want) {
            assert!((a - b).abs() < 1e-6, "{px:?}");
        }
        assert!(px[4] > 2.0, "over-white values are not clipped: {px:?}");
        assert!(px[8] < -0.1, "negative values are not clipped: {px:?}");
        // a disabled node, a bypassed grade: unchanged
        let before = img.px.clone();
        let id = g.nodes[0].id;
        g.node_mut(id).unwrap().enabled = false;
        apply_cpu(&g, &Luts::new(), &mut img, 1.0);
        assert_eq!(img.px, before);
        assert!(gpu_passes(&g, &Luts::new(), 1.0).is_empty());
    }

    #[test]
    fn transparent_pixels_stay_untouched() {
        let mut g = Grade::default();
        g.add(NodeKind::Gain { master: 3.0, rgb: [1.0; 3] }, None, None).unwrap();
        let mut img = Image { w: 1, h: 1, px: vec![0.0, 0.0, 0.0, 0.0] };
        apply_cpu(&g, &Luts::new(), &mut img, 1.0);
        assert_eq!(img.px, [0.0; 4]);
        assert_eq!(gpu_passes(&g, &Luts::new(), 1.0).len(), 1);
    }

    fn lut(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> Arc<LoadedLut> {
        Arc::new(LoadedLut::new(Lut3d::from_fn(size, f)).expect("valid cube"))
    }

    #[test]
    fn lut_nodes_mix_by_intensity_and_skip_when_missing() {
        let invert = lut(17, |c| c.map(|v| 1.0 - v));
        let mut luts = Luts::new();
        luts.insert("lib:inv".into(), invert);
        let mut g = Grade::default();
        let id = g.add(NodeKind::from_type("lut").unwrap(), None, None).unwrap();
        g.node_mut(id).unwrap().kind.set_param("lut", &serde_json::json!("lib:inv")).unwrap();
        let c = [0.2f32, 0.5, 0.9];
        let full = apply_signal(&g, &luts, c);
        for i in 0..3 {
            assert!((full[i] - (1.0 - c[i])).abs() < 1e-5, "{full:?}");
        }
        g.node_mut(id).unwrap().kind.set_param("intensity", &serde_json::json!(0.25)).unwrap();
        let quarter = apply_signal(&g, &luts, c);
        for i in 0..3 {
            assert!((quarter[i] - (c[i] + (1.0 - 2.0 * c[i]) * 0.25)).abs() < 1e-5, "{quarter:?}");
        }
        // a reference the project does not have: the node changes nothing, on both paths
        assert_eq!(apply_signal(&g, &Luts::new(), c), c);
        assert!(gpu_passes(&g, &Luts::new(), 1.0).is_empty());
        let passes = gpu_passes(&g, &luts, 1.0);
        assert_eq!(passes.len(), 1);
        let aux = passes[0].aux.as_ref().expect("the LUT image");
        assert_eq!((aux.w, aux.h, aux.rgba.len()), (17 * 17, 17, 17 * 17 * 17 * 4));
    }

    #[test]
    fn unusable_cubes_are_refused() {
        let mut c = Lut3d::identity(4);
        c.data.pop();
        assert!(LoadedLut::new(c).is_none(), "short table");
        let mut c = Lut3d::identity(4);
        c.domain_max = [0.0; 3];
        assert!(LoadedLut::new(c).is_none(), "empty domain");
        let mut c = Lut3d::identity(4);
        c.size = 1;
        c.data.truncate(1);
        assert!(LoadedLut::new(c).is_none(), "one point a side");
    }
}

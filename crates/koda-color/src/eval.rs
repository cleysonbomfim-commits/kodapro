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
use filmcraft_render::external::AuxImage;

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
}

/// The active nodes of `grade`, ready to run. A LUT node whose LUT is not loaded changes
/// nothing (on the CPU and the GPU alike).
fn ready(grade: &Grade, luts: &Luts) -> Vec<Ready> {
    if grade.is_identity() {
        return Vec::new();
    }
    grade
        .active()
        .filter_map(|n| match &n.kind {
            NodeKind::Gain { master, rgb } => Some(Ready::Gain(rgb.map(|v| v * master))),
            NodeKind::Cst(c) => Some(Ready::Cst(*c, c.matrix())),
            NodeKind::Lut { lut, intensity, interpolation } => {
                (*intensity > 0.0).then_some(())?;
                luts.get(lut).map(|l| Ready::Lut(l.clone(), *intensity, *interpolation))
            }
        })
        .collect()
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
    }
}

/// Apply `grade` to one pixel's signal (no premultiplication): the nodes in order.
pub fn apply_signal(grade: &Grade, luts: &Luts, c: [f32; 3]) -> [f32; 3] {
    ready(grade, luts).iter().fold(c, |c, r| run(r, c))
}

/// Apply `grade` to `px` (linear premultiplied RGBA f32, 4 floats a pixel) on the CPU.
pub fn apply_cpu(grade: &Grade, luts: &Luts, px: &mut [f32]) {
    let nodes = ready(grade, luts);
    if nodes.is_empty() {
        return;
    }
    use rayon::prelude::*;
    px.par_chunks_mut(4 * 1024).for_each(|chunk| {
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
    });
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

/// One GPU pass: integer parameters (`u.i1`), up to 24 floats (`u.p0`…`u.p5`) and an optional
/// auxiliary image (a LUT).
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub i1: [u32; 4],
    pub p: [f32; 24],
    pub aux: Option<Arc<AuxImage>>,
}

/// The GPU passes for `grade`: one per active node (none when it changes nothing).
pub fn gpu_passes(grade: &Grade, luts: &Luts) -> Vec<Pass> {
    ready(grade, luts)
        .into_iter()
        .map(|r| match r {
            Ready::Gain(g) => {
                let mut p = [0.0; 24];
                p[..3].copy_from_slice(&g);
                Pass { i1: [NODE_GAIN, 0, 0, 0], p, aux: None }
            }
            Ready::Cst(c, _) => {
                let (i1, p) = c.gpu();
                Pass { i1, p, aux: None }
            }
            Ready::Lut(l, k, interp) => {
                let mut p = [0.0; 24];
                p[0] = k;
                p[4..7].copy_from_slice(&l.cube.domain_min);
                p[8..11].copy_from_slice(&l.cube.domain_max);
                let tet = u32::from(interp == Interpolation::Tetrahedral);
                Pass { i1: [NODE_LUT, tet, l.cube.size as u32, 0], p, aux: Some(l.aux.clone()) }
            }
        })
        .collect()
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
        let mut px = vec![0.09, 0.09, 0.09, 0.5, 2.0, 2.0, 2.0, 1.0, -0.1, 0.2, 0.0, 1.0];
        apply_cpu(&g, &Luts::new(), &mut px);
        let s = to_signal(0.18);
        let want = [to_linear(s * 2.0) * 0.5, to_linear(s) * 0.5, to_linear(s * 2.0) * 0.5, 0.5];
        for (a, b) in px[..4].iter().zip(want) {
            assert!((a - b).abs() < 1e-6, "{px:?}");
        }
        assert!(px[4] > 2.0, "over-white values are not clipped: {px:?}");
        assert!(px[8] < -0.1, "negative values are not clipped: {px:?}");
        // a disabled node, a bypassed grade: unchanged
        let before = px.clone();
        let id = g.nodes[0].id;
        g.node_mut(id).unwrap().enabled = false;
        apply_cpu(&g, &Luts::new(), &mut px);
        assert_eq!(px, before);
        assert!(gpu_passes(&g, &Luts::new()).is_empty());
    }

    #[test]
    fn transparent_pixels_stay_untouched() {
        let mut g = Grade::default();
        g.add(NodeKind::Gain { master: 3.0, rgb: [1.0; 3] }, None, None).unwrap();
        let mut px = vec![0.0, 0.0, 0.0, 0.0];
        apply_cpu(&g, &Luts::new(), &mut px);
        assert_eq!(px, [0.0; 4]);
        assert_eq!(gpu_passes(&g, &Luts::new()).len(), 1);
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
        assert!(gpu_passes(&g, &Luts::new()).is_empty());
        let passes = gpu_passes(&g, &luts);
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

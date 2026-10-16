//! Evaluating a grade: the CPU reference ([`apply_cpu`]) and the GPU passes ([`gpu_passes`],
//! run by `filmcraft-gpu` with [`WGSL`]). Both work on the compositor's working image (linear
//! premultiplied RGBA f32) and must agree: the CPU is the oracle, the GPU the fast path.
//!
//! **Signal.** Nodes grade the picture as a colourist sees it, not linear light: each pixel is
//! un-premultiplied, encoded to the grading signal ([`to_signal`]), passed through the nodes,
//! then decoded and premultiplied again. Values are never clipped between nodes (32-bit float,
//! negatives and values above 1 survive). Marco 1 grades in an extended sRGB/Rec.709 display
//! signal; marco 2 replaces this with the clip's own input signal (S-Log3 before the CST).

use crate::grade::{Grade, NodeKind};

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

/// One node on one pixel's signal (RGB).
#[inline]
fn node(kind: &NodeKind, c: [f32; 3]) -> [f32; 3] {
    match kind {
        NodeKind::Gain { master, rgb } => [c[0] * master * rgb[0], c[1] * master * rgb[1], c[2] * master * rgb[2]],
    }
}

/// Apply `grade` to `px` (linear premultiplied RGBA f32, 4 floats a pixel) on the CPU.
pub fn apply_cpu(grade: &Grade, px: &mut [f32]) {
    if grade.is_identity() {
        return;
    }
    let nodes: Vec<&NodeKind> = grade.active().map(|n| &n.kind).collect();
    use rayon::prelude::*;
    px.par_chunks_mut(4 * 1024).for_each(|chunk| {
        for p in chunk.as_chunks_mut::<4>().0 {
            let a = p[3];
            if a <= 0.0 {
                continue;
            }
            let mut c = [to_signal(p[0] / a), to_signal(p[1] / a), to_signal(p[2] / a)];
            for k in &nodes {
                c = node(k, c);
            }
            for i in 0..3 {
                p[i] = to_linear(c[i]) * a;
            }
        }
    });
}

/// The compute shader the GPU passes run. It declares the bindings of `filmcraft-gpu`'s effect
/// stage (`fx.wgsl`: uniform `u`, `src`, `dst`) and the entry point `ext_px`; `u.i1.x` is the node
/// type and `u.p0`… its parameters. Each pass un-premultiplies, encodes, applies one node, decodes
/// and premultiplies, exactly like [`apply_cpu`] does per node (encode / decode are inverses, so
/// a chain of passes equals the CPU's single encode, nodes, decode).
pub const WGSL: &str = include_str!("grade.wgsl");

/// Node type codes in [`WGSL`] (`u.i1.x`).
pub const NODE_GAIN: u32 = 1;

/// One GPU pass: integer parameters (`u.i1`) and up to 24 floats (`u.p0`…`u.p5`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pass {
    pub i1: [u32; 4],
    pub p: [f32; 24],
}

/// The GPU passes for `grade`: one per active node (none when it changes nothing).
pub fn gpu_passes(grade: &Grade) -> Vec<Pass> {
    if grade.is_identity() {
        return Vec::new();
    }
    grade
        .active()
        .map(|n| match &n.kind {
            NodeKind::Gain { master, rgb } => {
                let mut p = [0.0; 24];
                p[0] = master * rgb[0];
                p[1] = master * rgb[1];
                p[2] = master * rgb[2];
                Pass { i1: [NODE_GAIN, 0, 0, 0], p }
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
        apply_cpu(&g, &mut px);
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
        apply_cpu(&g, &mut px);
        assert_eq!(px, before);
        assert!(gpu_passes(&g).is_empty());
    }

    #[test]
    fn transparent_pixels_stay_untouched() {
        let mut g = Grade::default();
        g.add(NodeKind::Gain { master: 3.0, rgb: [1.0; 3] }, None, None).unwrap();
        let mut px = vec![0.0, 0.0, 0.0, 0.0];
        apply_cpu(&g, &mut px);
        assert_eq!(px, [0.0; 4]);
        assert_eq!(gpu_passes(&g).len(), 1);
    }
}

//! Imagens do marco 4 da aba Cor.
//!   cargo run --release -p koda-color --example imagens_marco4
//! - docs/koda/cor/referencia/comparacoes/{nitidez_02_vs_03,glow_03_vs_04,nitidez_glow_02_vs_04}.png
//!   (LOCAIS, fora do git: vídeo da cliente): KODA | DaVinci | mapa de ΔE 2000.
//! - docs/koda/cor/prints/marco4-carta.png (pública): cartela sintética pelo CST, depois com
//!   nitidez + glow do powergrade (Soft Light) e com glow Screen e Add.
#[path = "comum/carta.rs"]
mod carta;

use filmcraft_render::Image;
use koda_color::eval::{Luts, apply_cpu, to_linear, to_signal};
use koda_color::spatial::{Glow, GlowStyle, Sharpen};
use koda_color::{Grade, NodeKind, deltae};

fn grade(kinds: Vec<NodeKind>) -> Grade {
    let mut g = Grade::default();
    for k in kinds {
        let _ = g.add(k, None, None);
    }
    g
}

fn run(g: &Grade, v: &[[f32; 3]], w: usize) -> Vec<[f32; 3]> {
    let mut img = Image { w, h: v.len() / w, px: v.iter().flat_map(|c| [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), 1.0]).collect() };
    apply_cpu(g, &Luts::new(), &mut img, 1.0);
    img.px.chunks(4).map(|p| [to_signal(p[0]), to_signal(p[1]), to_signal(p[2])]).collect()
}

fn main() {
    let sharpen = || NodeKind::Sharpen(Sharpen::default());
    let glow = |style| NodeKind::Glow(Glow { style, ..Glow::default() });
    if let (Some(f2), Some(f3), Some(f4)) = (carta::frame("02_cst.tif"), carta::frame("03_nitidez.tif.tif"), carta::frame("04_glow.tif.tif")) {
        let dir = carta::root().join("referencia/comparacoes");
        for (out, g, src, want) in [
            ("nitidez_02_vs_03.png", grade(vec![sharpen()]), &f2, &f3),
            ("glow_03_vs_04.png", grade(vec![glow(GlowStyle::SoftLight)]), &f3, &f4),
            ("nitidez_glow_02_vs_04.png", grade(vec![sharpen(), glow(GlowStyle::SoftLight)]), &f2, &f4),
        ] {
            let ours = run(&g, src, 1080);
            let d = deltae::per_pixel(&ours, want);
            println!("{out}: {}", deltae::compare(&ours, want));
            carta::save(&dir.join(out), &[carta::half(&ours, 1080), carta::half(want, 1080), carta::half(&carta::heat(&d), 1080)]);
        }
    }
    let (w, raw) = carta::chart();
    let cst = || NodeKind::from_type("cst").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] });
    let panels: Vec<_> = [
        grade(vec![cst()]),
        grade(vec![cst(), sharpen(), glow(GlowStyle::SoftLight)]),
        grade(vec![cst(), NodeKind::Glow(Glow { style: GlowStyle::Screen, threshold: 0.6, spread: 20.0, opacity: 0.8, ..Glow::default() })]),
        grade(vec![
            cst(),
            NodeKind::Glow(Glow { style: GlowStyle::Add, threshold: 0.6, spread: 20.0, gain: 0.6, opacity: 1.0, highlight_recovery: 1.0, ..Glow::default() }),
        ]),
    ]
    .iter()
    .map(|g| carta::half(&run(g, &raw, w), w))
    .collect();
    carta::save(&carta::root().join("prints/marco4-carta.png"), &panels);
}

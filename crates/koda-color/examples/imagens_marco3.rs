//! Imagens do marco 3 da aba Cor.
//!   cargo run --release -p koda-color --example imagens_marco3
//! - docs/koda/cor/referencia/comparacoes/primarias_00_vs_01.png (LOCAL, fora do git: vídeo da
//!   cliente): KODA | DaVinci | mapa de ΔE 2000 do nó 01.
//! - docs/koda/cor/prints/marco3-carta.png (pública): cartela sintética em S-Log3 com o CST do
//!   powergrade depois de: nada | nó 01 (global) | primárias criativas | curvas.
#[path = "comum/carta.rs"]
mod carta;

use koda_color::eval::{Luts, apply_signal};
use koda_color::primaries::{CurvePoints, Primaries, koda_global};
use koda_color::{Grade, NodeKind, deltae};

fn with_cst(p: Option<Primaries>) -> Grade {
    let mut g = Grade::default();
    if let Some(p) = p {
        let _ = g.add(NodeKind::Primaries(Box::new(p)), None, None);
    }
    if let Some(k) = NodeKind::from_type("cst") {
        let _ = g.add(k, None, None);
    }
    g
}

fn run(g: &Grade, v: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let l = Luts::new();
    v.iter().map(|c| apply_signal(g, &l, *c)).collect()
}

fn main() {
    if let (Some(a), Some(b)) = (carta::frame("00_log.tif"), carta::frame("01_global.tif")) {
        let r = koda_global().ready();
        let ours: Vec<[f32; 3]> = a.iter().map(|c| r.apply(*c)).collect();
        let d = deltae::per_pixel(&ours, &b);
        println!("primárias 00 → 01: {}", deltae::compare(&ours, &b));
        carta::save(
            &carta::root().join("referencia/comparacoes/primarias_00_vs_01.png"),
            &[carta::half(&ours, 1080), carta::half(&b, 1080), carta::half(&carta::heat(&d), 1080)],
        );
    }
    let (w, raw) = carta::chart();
    let creative = Primaries { lift: [0.01, 0.0, 0.0, 0.015], gain: [1.03, 1.01, 1.0, 0.98], contrast: 1.1, sat: 56.0, temp: 90.0, ..Default::default() };
    let mut curves = Primaries::default();
    curves.curves.y = CurvePoints(vec![[0.0, 0.0], [0.25, 0.18], [0.75, 0.82], [1.0, 1.0]]).sanitized();
    curves.curves.linked = false;
    curves.curves.b = CurvePoints(vec![[0.0, 0.06], [0.5, 0.48], [1.0, 0.95]]).sanitized();
    let panels: Vec<_> = [None, Some(koda_global()), Some(creative), Some(curves)].into_iter().map(|p| carta::half(&run(&with_cst(p), &raw), w)).collect();
    carta::save(&carta::root().join("prints/marco3-carta.png"), &panels);
}

//! Imagens do marco 2 da aba Cor.
//!   cargo run --release -p koda-color --example imagens_marco2
//! - docs/koda/cor/referencia/comparacoes/*.png (LOCAIS, fora do git: têm o vídeo da cliente):
//!   KODA | DaVinci | mapa de ΔE 2000 (preto = 0, vermelho = 3, branco ≥ 6).
//! - docs/koda/cor/prints/marco2-carta.png (pública): cartela sintética em S-Log3 / S-Gamut3.Cine,
//!   sem imagem de cliente: bruto log | CST | CST + LUT do nó 05 (se a LUT existir).
#[path = "comum/carta.rs"]
mod carta;

use std::sync::Arc;

use carta::{chart, frame, half, heat, root, save};
use filmcraft_color::Lut3d;
use koda_color::eval::{LoadedLut, Luts, apply_signal};
use koda_color::{Grade, NodeKind, deltae};

fn grade(kind: NodeKind) -> Grade {
    let mut g = Grade::default();
    g.add(kind, None, None).unwrap_or_default();
    g
}

fn lut_grade(luts: &mut Luts) -> Option<Grade> {
    let path = koda_color::reference_file("Orson.cube")?;
    let cube = Lut3d::parse_cube(&std::fs::read_to_string(path).ok()?).ok()?;
    luts.insert("lib:orson".into(), Arc::new(LoadedLut::new(cube)?));
    let mut k = NodeKind::from_type("lut")?;
    k.set_param("lut", &serde_json::json!("lib:orson")).ok()?;
    Some(grade(k))
}

fn run(g: &Grade, luts: &Luts, v: &[[f32; 3]]) -> Vec<[f32; 3]> {
    v.iter().map(|c| apply_signal(g, luts, *c)).collect()
}

fn comparisons() {
    let dir = root().join("referencia/comparacoes");
    let cst = grade(NodeKind::from_type("cst").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] }));
    let none = Luts::new();
    let mut luts = Luts::new();
    let lut = lut_grade(&mut luts);
    let pairs: Vec<(&str, &str, &str, bool)> = vec![
        ("cst_01_vs_02.png", "01_global.tif", "02_cst.tif", false),
        ("cst_00_vs_02.png", "00_log.tif", "02_cst.tif", false),
        ("lut_04_vs_05.png", "04_glow.tif.tif", "05_lut.tif.tif", true),
        ("lut_02_vs_05.png", "02_cst.tif", "05_lut.tif.tif", true),
    ];
    for (out, src, want, is_lut) in pairs {
        let (Some(s), Some(w)) = (frame(src), frame(want)) else { continue };
        let ours = match (is_lut, &lut) {
            (false, _) => run(&cst, &none, &s),
            (true, Some(g)) => run(g, &luts, &s),
            (true, None) => continue,
        };
        let d = deltae::per_pixel(&ours, &w);
        println!("{out}: {}", deltae::compare(&ours, &w));
        save(&dir.join(out), &[half(&ours, 1080), half(&w, 1080), half(&heat(&d), 1080)]);
    }
}

fn chart_print() {
    let (w, raw) = chart();
    let cst = grade(NodeKind::from_type("cst").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] }));
    let after = run(&cst, &Luts::new(), &raw);
    let mut panels = vec![half(&raw, w), half(&after, w)];
    let mut luts = Luts::new();
    if let Some(mut g) = lut_grade(&mut luts) {
        let n = g.nodes.remove(0);
        let mut both = cst.clone();
        let _ = both.add(n.kind, Some(&n.name), None);
        panels.push(half(&run(&both, &luts, &raw), w));
        g.nodes.clear();
    }
    save(&root().join("prints/marco2-carta.png"), &panels);
}

fn main() {
    comparisons();
    chart_print();
}

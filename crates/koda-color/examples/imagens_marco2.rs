//! Imagens do marco 2 da aba Cor.
//!   cargo run --release -p koda-color --example imagens_marco2
//! - docs/koda/cor/referencia/comparacoes/*.png (LOCAIS, fora do git: têm o vídeo da cliente):
//!   KODA | DaVinci | mapa de ΔE 2000 (preto = 0, vermelho = 3, branco ≥ 6).
//! - docs/koda/cor/prints/marco2-carta.png (pública): cartela sintética em S-Log3 / S-Gamut3.Cine,
//!   sem imagem de cliente: bruto log | CST | CST + LUT do nó 05 (se a LUT existir).
use std::sync::Arc;

use filmcraft_color::Lut3d;
use koda_color::eval::{LoadedLut, Luts, apply_signal};
use koda_color::{Grade, NodeKind, deltae};

fn frame(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/koda/cor")
}

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

/// Half-size 8-bit copy (2 × 2 average).
fn half(v: &[[f32; 3]], w: usize) -> (usize, usize, Vec<[u8; 3]>) {
    let h = v.len() / w;
    let (hw, hh) = (w / 2, h / 2);
    let mut o = Vec::with_capacity(hw * hh);
    for y in 0..hh {
        for x in 0..hw {
            let mut s = [0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = v[(2 * y + dy) * w + 2 * x + dx];
                for i in 0..3 {
                    s[i] += p[i] / 4.0;
                }
            }
            o.push(s.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    (hw, hh, o)
}

fn heat(d: &[f64]) -> Vec<[f32; 3]> {
    d.iter()
        .map(|&e| {
            let e = e as f32;
            if e < 3.0 { [e / 3.0, 0.0, 0.0] } else { [1.0, ((e - 3.0) / 3.0).min(1.0), ((e - 3.0) / 3.0).min(1.0)] }
        })
        .collect()
}

/// Panels side by side, 12 px apart, saved as PNG.
fn save(path: &std::path::Path, panels: &[(usize, usize, Vec<[u8; 3]>)]) {
    let gap = 12;
    let h = panels.iter().map(|p| p.1).max().unwrap_or(0);
    let w: usize = panels.iter().map(|p| p.0).sum::<usize>() + gap * panels.len().saturating_sub(1);
    let mut img = image::RgbImage::from_pixel(w as u32, h as u32, image::Rgb([24, 24, 26]));
    let mut x0 = 0;
    for (pw, ph, px) in panels {
        for y in 0..*ph {
            for x in 0..*pw {
                img.put_pixel((x0 + x) as u32, y as u32, image::Rgb(px[y * pw + x]));
            }
        }
        x0 += pw + gap;
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match img.save(path) {
        Ok(()) => println!("gravado {}", path.display()),
        Err(e) => eprintln!("{}: {e}", path.display()),
    }
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

/// The synthetic chart: an S-Log3 / S-Gamut3.Cine signal of a grey ramp (−6 to +4 stops around
/// 18 % grey), saturated hues at three exposures, and a ColorChecker-like set of patches.
fn chart() -> (usize, Vec<[f32; 3]>) {
    let (w, h) = (720usize, 480usize);
    let to_sg3c = {
        // Rec.709 linear → S-Gamut3.Cine linear
        let m = filmcraft_color::spaces::gamut_matrix(filmcraft_color::Gamut::Bt709, filmcraft_color::Gamut::SGamut3Cine);
        move |c: [f64; 3]| filmcraft_color::spaces::mul_vec(&m, c)
    };
    let enc = |c: [f64; 3]| to_sg3c(c).map(|v| filmcraft_color::log::slog3_encode(v) as f32);
    let patches: [[f64; 3]; 18] = [
        [0.40, 0.18, 0.12],
        [0.80, 0.55, 0.45],
        [0.20, 0.30, 0.50],
        [0.20, 0.30, 0.10],
        [0.35, 0.33, 0.60],
        [0.25, 0.65, 0.55],
        [0.85, 0.40, 0.05],
        [0.12, 0.18, 0.55],
        [0.75, 0.20, 0.25],
        [0.18, 0.08, 0.25],
        [0.55, 0.75, 0.10],
        [0.90, 0.65, 0.05],
        [0.05, 0.08, 0.40],
        [0.12, 0.45, 0.12],
        [0.60, 0.05, 0.06],
        [0.95, 0.85, 0.05],
        [0.70, 0.12, 0.50],
        [0.02, 0.40, 0.60],
    ];
    let mut v = vec![[0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let c = if y < 80 {
                // grey ramp in stops
                let stops = -6.0 + 10.0 * x as f64 / (w - 1) as f64;
                let g = 0.18 * 2f64.powf(stops);
                [g; 3]
            } else if y < 240 {
                // hue sweep at three exposures
                let band = (y - 80) / 54;
                let hue = x as f64 / w as f64 * 6.0;
                let f = hue.fract();
                let rgb = match hue as u32 {
                    0 => [1.0, f, 0.0],
                    1 => [1.0 - f, 1.0, 0.0],
                    2 => [0.0, 1.0, f],
                    3 => [0.0, 1.0 - f, 1.0],
                    4 => [f, 0.0, 1.0],
                    _ => [1.0, 0.0, 1.0 - f],
                };
                let k = [0.12, 0.45, 1.6][band.min(2)];
                rgb.map(|c| 0.02 + c * k)
            } else {
                let (cx, cy) = (x / 120, (y - 240) / 80);
                patches[(cy * 6 + cx).min(17)]
            };
            v[y * w + x] = enc(c);
        }
    }
    (w, v)
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

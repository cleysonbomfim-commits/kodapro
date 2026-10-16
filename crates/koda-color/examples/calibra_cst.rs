//! Calibração do nó CST contra o DaVinci (frames locais em docs/koda/cor/referencia/).
//! cargo run --release -p koda-color --example calibra_cst
use koda_color::cst::{Cst, GamutMapping, ToneMapping, filmic};
use koda_color::deltae;

fn load(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

/// Tone mapping applied per channel / on max(RGB) / on luminance (as in `Cst::apply`).
#[derive(Clone, Copy, Debug)]
enum How {
    Luma,
    MaxRgb,
    PerChannel,
}

fn run(src: &[[f32; 3]], cst: &Cst, how: How) -> Vec<[f32; 3]> {
    let m = cst.matrix();
    src.iter()
        .map(|c| {
            let l = c.map(|v| cst.input_curve.decode(v));
            let mut o = [0f32; 3];
            for (i, row) in m.iter().enumerate() {
                o[i] = row[0] * l[0] + row[1] * l[1] + row[2] * l[2];
            }
            if let ToneMapping::Filmic { white, mid, contrast } = cst.tone_mapping {
                o = match how {
                    How::Luma => {
                        let y = 0.2126 * o[0] + 0.7152 * o[1] + 0.0722 * o[2];
                        let k = if y > 1e-9 { filmic(y, white, mid, contrast) / y } else { 0.0 };
                        o.map(|v| v * k)
                    }
                    How::MaxRgb => {
                        let y = o[0].max(o[1]).max(o[2]);
                        let k = if y > 1e-9 { filmic(y, white, mid, contrast) / y } else { 0.0 };
                        o.map(|v| v * k)
                    }
                    How::PerChannel => o.map(|v| if v > 0.0 { filmic(v, white, mid, contrast) } else { 0.0 }),
                };
            }
            if cst.gamut_mapping == GamutMapping::Compress {
                o = koda_color::cst::compress(o);
            }
            o.map(|v| cst.output_curve.encode(v))
        })
        .collect()
}

fn main() {
    if std::env::args().any(|a| a == "--smoothed") {
        smoothed();
        return;
    }
    if std::env::args().any(|a| a == "--outliers") {
        outliers();
        return;
    }
    if std::env::args().any(|a| a == "--refine") {
        refine();
        return;
    }
    let (Some(z), Some(a), Some(b)) = (load("00_log.tif"), load("01_global.tif"), load("02_cst.tif")) else {
        eprintln!("referências ausentes");
        return;
    };
    let base = Cst::default();
    let mut best: Option<(f64, String)> = None;
    for how in [How::Luma, How::MaxRgb, How::PerChannel] {
        for gm in [GamutMapping::None, GamutMapping::Compress] {
            let c = Cst { gamut_mapping: gm, ..base };
            let s = deltae::compare(&run(&a, &c, how), &b);
            let tag = format!("{how:?} + {gm:?}");
            println!("CST(01_global) vs 02_cst  [{tag}]  {s}");
            if best.as_ref().is_none_or(|b| s.mean < b.0) {
                best = Some((s.mean, tag));
            }
        }
    }
    let c = Cst { tone_mapping: ToneMapping::None, ..base };
    println!("sem tone mapping: {}", deltae::compare(&run(&a, &c, How::Luma), &b));
    println!("melhor: {:?}", best);
    println!("CST(00_log) vs 02_cst [Luma + Compress]: {}", deltae::compare(&run(&z, &base, How::Luma), &b));
}

/// Busca dos parâmetros da curva (modo por canal) minimizando o ΔE médio numa amostra.
#[allow(dead_code)]
pub fn refine() {
    let (Some(a), Some(b)) = (load("01_global.tif"), load("02_cst.tif")) else { return };
    let step = 37;
    let sa: Vec<[f32; 3]> = a.iter().step_by(step).copied().collect();
    let sb: Vec<[f32; 3]> = b.iter().step_by(step).copied().collect();
    let (mut w, mut mid, mut c) = koda_color::cst::FILMIC_DEFAULT;
    let mut best = f64::MAX;
    for round in 0..4 {
        let k = 0.5f32.powi(round);
        let mut improved = true;
        while improved {
            improved = false;
            for (dw, dm, dc) in [(0.01, 0.0, 0.0), (-0.01, 0.0, 0.0), (0.0, 0.05, 0.0), (0.0, -0.05, 0.0), (0.0, 0.0, 0.04), (0.0, 0.0, -0.04)] {
                let (tw, tm, tc) = (w + dw * k, mid + dm * k, c + dc * k);
                let cst = Cst { tone_mapping: ToneMapping::Filmic { white: tw, mid: tm, contrast: tc }, gamut_mapping: GamutMapping::None, ..Cst::default() };
                let s = deltae::compare(&run(&sa, &cst, How::PerChannel), &sb);
                if s.mean < best - 1e-5 {
                    best = s.mean;
                    (w, mid, c) = (tw, tm, tc);
                    improved = true;
                }
            }
        }
        println!("rodada {round}: white {w:.4} mid {mid:.4} contrast {c:.4}  média amostra {best:.4}");
    }
    let cst = Cst { tone_mapping: ToneMapping::Filmic { white: w, mid, contrast: c }, gamut_mapping: GamutMapping::None, ..Cst::default() };
    println!("imagem inteira: {}", deltae::compare(&run(&a, &cst, How::PerChannel), &b));
}

/// Quem são os pixels com ΔE > 3?
#[allow(dead_code)]
pub fn outliers() {
    let (Some(a), Some(b)) = (load("01_global.tif"), load("02_cst.tif")) else { return };
    let cst = Cst { gamut_mapping: GamutMapping::None, ..Cst::default() };
    let ours = run(&a, &cst, How::PerChannel);
    let d = deltae::per_pixel(&ours, &b);
    let mut bad: Vec<usize> = (0..d.len()).filter(|&i| d[i] > 3.0).collect();
    println!("pixels com ΔE > 3: {} de {} ({:.3}%)", bad.len(), d.len(), 100.0 * bad.len() as f64 / d.len() as f64);
    bad.sort_by(|x, y| d[*y].total_cmp(&d[*x]));
    for &i in bad.iter().step_by((bad.len() / 12).max(1)).take(12) {
        let (x, y) = (i % 1080, i / 1080);
        let (s, o, r) = (a[i], ours[i], b[i]);
        println!(
            "({x:4},{y:4}) ΔE {:.2}  entrada {:?}  nosso {:?}  davinci {:?}",
            d[i],
            s.map(|v| (v * 1023.0).round()),
            o.map(|v| (v * 1023.0).round()),
            r.map(|v| (v * 1023.0).round())
        );
    }
    // por faixa de brilho da entrada
    let mut by = [(0.0f64, 0usize); 10];
    for i in 0..d.len() {
        let k = ((a[i][0] + a[i][1] + a[i][2]) / 3.0 * 10.0).min(9.0) as usize;
        by[k].0 += d[i];
        by[k].1 += 1;
    }
    for (k, (s, n)) in by.iter().enumerate().filter(|x| x.1.1 > 0) {
        println!("entrada {:.1}-{:.1}: ΔE médio {:.3} ({} px)", k as f32 / 10.0, (k + 1) as f32 / 10.0, s / *n as f64, n);
    }
}

fn blur3(v: &[[f32; 3]], w: usize) -> Vec<[f32; 3]> {
    let h = v.len() / w;
    let mut o = vec![[0f32; 3]; v.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0f32; 3];
            let mut n = 0.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        let p = v[yy as usize * w + xx as usize];
                        for i in 0..3 {
                            s[i] += p[i];
                        }
                        n += 1.0;
                    }
                }
            }
            o[y * w + x] = s.map(|c| c / n);
        }
    }
    o
}

/// O mesmo ΔE com as duas imagens suavizadas 3×3 (tira o ruído de um pixel só).
#[allow(dead_code)]
pub fn smoothed() {
    let (Some(a), Some(b), Some(z)) = (load("01_global.tif"), load("02_cst.tif"), load("00_log.tif")) else { return };
    let cst = Cst { gamut_mapping: GamutMapping::None, ..Cst::default() };
    let ours = run(&a, &cst, How::PerChannel);
    println!("pixel a pixel: {}", deltae::compare(&ours, &b));
    println!("suavizado 3x3: {}", deltae::compare(&blur3(&ours, 1080), &blur3(&b, 1080)));
    // ruído entre exportações: 00 vs 01 nas regiões onde o nó 01 quase não mexe?
    let _ = z;
}

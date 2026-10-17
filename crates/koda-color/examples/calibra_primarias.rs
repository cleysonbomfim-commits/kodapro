//! Calibração das primárias contra o DaVinci: 00_log → 01_global (nó 01 com Temp −69,9,
//! Tint −5,78, Shadows 17,5; valores lidos do print davinci-color-referencia.png).
//! cargo run --release -p koda-color --example calibra_primarias
fn load(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

fn main() {
    if std::env::args().any(|a| a == "--curve") {
        fit_curve();
        return;
    }
    if std::env::args().any(|a| a == "--models") {
        fit_models();
        return;
    }
    if std::env::args().any(|a| a == "--gains") {
        fit_gains();
        return;
    }
    let (Some(a), Some(b)) = (load("00_log.tif"), load("01_global.tif")) else { return };
    // per channel: mean output and mean difference by input bin (only near-grey pixels, so the
    // other channels matter little)
    for c in 0..3 {
        let mut bins = vec![(0f64, 0f64, 0usize); 32];
        for (x, y) in a.iter().zip(&b) {
            let k = ((x[c] * 32.0) as usize).min(31);
            bins[k].0 += x[c] as f64;
            bins[k].1 += y[c] as f64;
            bins[k].2 += 1;
        }
        println!("canal {c}:");
        for (k, (sx, sy, n)) in bins.iter().enumerate() {
            if *n > 200 {
                let (mx, my) = (sx / *n as f64, sy / *n as f64);
                println!("  bin {k:2} n {n:7} in {:.4} out {:.4} diff {:+.5} ratio {:.4}", mx, my, my - mx, my / mx);
            }
        }
    }
    // the same against luma of input
    let mut bins = vec![([0f64; 3], [0f64; 3], 0usize); 32];
    for (x, y) in a.iter().zip(&b) {
        let l = (x[0] + x[1] + x[2]) / 3.0;
        let k = ((l * 32.0) as usize).min(31);
        for i in 0..3 {
            bins[k].0[i] += x[i] as f64;
            bins[k].1[i] += y[i] as f64;
        }
        bins[k].2 += 1;
    }
    println!("por brilho médio:");
    for (k, (sx, sy, n)) in bins.iter().enumerate() {
        if *n > 200 {
            let n = *n as f64;
            println!(
                "  bin {k:2} n {n:7} in [{:.4} {:.4} {:.4}] diff [{:+.5} {:+.5} {:+.5}]",
                sx[0] / n,
                sx[1] / n,
                sx[2] / n,
                (sy[0] - sx[0]) / n,
                (sy[1] - sx[1]) / n,
                (sy[2] - sx[2]) / n
            );
        }
    }
}

/// Fit: out_c = C(g_c · in_c), C a shared curve (binned), g_G = 1. Returns residual RMS.
pub fn shared_curve_fit(a: &[[f32; 3]], b: &[[f32; 3]], g: [f32; 3]) -> (f64, Vec<f64>) {
    const N: usize = 512;
    let mut s = vec![(0f64, 0usize); N];
    let key = |v: f32| ((v.clamp(0.0, 1.2) / 1.2 * (N - 1) as f32) as usize).min(N - 1);
    for (x, y) in a.iter().zip(b) {
        for c in 0..3 {
            let k = key(x[c] * g[c]);
            s[k].0 += y[c] as f64;
            s[k].1 += 1;
        }
    }
    let curve: Vec<f64> = s.iter().map(|(v, n)| if *n > 0 { v / *n as f64 } else { f64::NAN }).collect();
    let mut e = 0f64;
    let mut n = 0usize;
    for (x, y) in a.iter().zip(b) {
        for c in 0..3 {
            let v = curve[key(x[c] * g[c])];
            if v.is_finite() {
                e += (v - y[c] as f64).powi(2);
                n += 1;
            }
        }
    }
    ((e / n.max(1) as f64).sqrt(), curve)
}

#[allow(dead_code)]
pub fn fit_gains() {
    let (Some(a), Some(b)) = (load("00_log.tif"), load("01_global.tif")) else { return };
    let sa: Vec<[f32; 3]> = a.iter().step_by(7).copied().collect();
    let sb: Vec<[f32; 3]> = b.iter().step_by(7).copied().collect();
    let mut best = (f64::MAX, 0.0, 0.0);
    for i in -40..=40 {
        for j in -40..=40 {
            let g = [1.0 + i as f32 * 0.0025, 1.0, 1.0 + j as f32 * 0.0025];
            let (e, _) = shared_curve_fit(&sa, &sb, g);
            if e < best.0 {
                best = (e, g[0] as f64, g[2] as f64);
            }
        }
    }
    println!("ganhos (sinal): R {:.4} B {:.4} (G = 1)  rms {:.5}", best.1, best.2, best.0);
    let (e1, _) = shared_curve_fit(&sa, &sb, [1.0; 3]);
    println!("sem ganhos: rms {e1:.5}");
}

/// The same fit with the gains applied as offsets in the signal, or as gains in S-Log3 linear.
#[allow(dead_code)]
pub fn fit_models() {
    use koda_color::cst::Curve;
    let (Some(a), Some(b)) = (load("00_log.tif"), load("01_global.tif")) else { return };
    let sa: Vec<[f32; 3]> = a.iter().step_by(7).copied().collect();
    let sb: Vec<[f32; 3]> = b.iter().step_by(7).copied().collect();
    for model in ["offset", "lineargain"] {
        let mut best = (f64::MAX, 0.0, 0.0);
        for i in -40..=40 {
            for j in -40..=40 {
                let (p, q) = (i as f32 * 0.0025, j as f32 * 0.0025);
                let t: Vec<[f32; 3]> = sa
                    .iter()
                    .map(|c| match model {
                        "offset" => [c[0] + p * 0.2, c[1], c[2] + q * 0.2],
                        _ => {
                            let l = c.map(|v| Curve::SLog3.decode(v));
                            [Curve::SLog3.encode(l[0] * (1.0 + 4.0 * p)), c[1], Curve::SLog3.encode(l[2] * (1.0 + 4.0 * q))]
                        }
                    })
                    .collect();
                let (e, _) = shared_curve_fit(&t, &sb, [1.0; 3]);
                if e < best.0 {
                    best = (e, p as f64, q as f64);
                }
            }
        }
        let k = if model == "offset" { 0.2 } else { 4.0 };
        println!("{model}: R {:+.4} B {:+.4}  rms {:.5}", best.1 * k, best.2 * k, best.0);
    }
}

/// Node 01 as read in the screenshot (Temp −69.9, Tint −5.78, Shadows 17.5), plus a Y curve
/// fitted here (the curve's points are not visible): coordinate descent on the ΔE mean.
#[allow(dead_code)]
pub fn fit_curve() {
    use koda_color::primaries::{CurvePoints, Primaries};
    let (Some(a), Some(b)) = (load("00_log.tif"), load("01_global.tif")) else { return };
    let sa: Vec<[f32; 3]> = a.iter().step_by(23).copied().collect();
    let sb: Vec<[f32; 3]> = b.iter().step_by(23).copied().collect();
    let xs = [0.0f32, 0.2, 0.35, 0.45, 0.55, 0.65, 0.75, 0.9, 1.0];
    let mut ys: Vec<f32> = xs.to_vec();
    let base = Primaries { temp: -69.9, tint: -5.78, shadows: 17.5, ..Default::default() };
    let score = |ys: &[f32]| {
        let mut p = base.clone();
        p.curves.y = CurvePoints(xs.iter().zip(ys).map(|(x, y)| [*x, *y]).collect()).sanitized();
        let r = p.ready();
        let out: Vec<[f32; 3]> = sa.iter().map(|c| r.apply(*c)).collect();
        koda_color::deltae::compare(&out, &sb).mean
    };
    let mut best = score(&ys);
    println!("sem curva: {best:.4}");
    for round in 0..5 {
        let step = 0.02 * 0.5f32.powi(round);
        let mut improved = true;
        while improved {
            improved = false;
            for i in 0..ys.len() {
                for d in [step, -step] {
                    let mut t = ys.clone();
                    t[i] += d;
                    let s = score(&t);
                    if s < best - 1e-5 {
                        best = s;
                        ys = t;
                        improved = true;
                    }
                }
            }
        }
        println!("rodada {round}: média {best:.4} pontos {:?}", xs.iter().zip(&ys).map(|(x, y)| format!("[{x}, {y:.4}]")).collect::<Vec<_>>());
    }
    let mut p = base.clone();
    p.curves.y = CurvePoints(xs.iter().zip(&ys).map(|(x, y)| [*x, *y]).collect()).sanitized();
    let r = p.ready();
    let out: Vec<[f32; 3]> = a.iter().map(|c| r.apply(*c)).collect();
    println!("imagem inteira: {}", koda_color::deltae::compare(&out, &b));
    let p0 = Primaries { curves: p.curves.clone(), ..Default::default() };
    let r0 = p0.ready();
    let out0: Vec<[f32; 3]> = a.iter().map(|c| r0.apply(*c)).collect();
    println!("só a curva (sem Temp/Tint/Shadows): {}", koda_color::deltae::compare(&out0, &b));
}

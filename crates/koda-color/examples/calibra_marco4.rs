//! Calibração do marco 4 contra o DaVinci: nitidez (02_cst → 03_nitidez) e glow
//! (03_nitidez → 04_glow). Frames locais em docs/koda/cor/referencia/.
//! cargo run --release -p koda-color --example calibra_marco4 -- [--analise]
#[path = "comum/carta.rs"]
mod carta;

use koda_color::deltae;

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn analise(a: &[[f32; 3]], b: &[[f32; 3]], tag: &str) {
    println!("{tag}: {}", deltae::compare(a, b));
    let mut bins = vec![([0f64; 3], 0usize); 20];
    for (x, y) in a.iter().zip(b) {
        let k = ((luma(*x) * 20.0) as usize).min(19);
        for i in 0..3 {
            bins[k].0[i] += (y[i] - x[i]) as f64;
        }
        bins[k].1 += 1;
    }
    for (k, (s, n)) in bins.iter().enumerate().filter(|b| b.1.1 > 100) {
        let n = *n as f64;
        println!("  luma {:.2}-{:.2} n {:8}  diff [{:+.4} {:+.4} {:+.4}]", k as f32 / 20.0, (k + 1) as f32 / 20.0, n, s[0] / n, s[1] / n, s[2] / n);
    }
}

/// High-frequency energy: mean |x - mean of 3x3| (sharpening raises it).
fn hf(v: &[[f32; 3]], w: usize) -> f64 {
    let h = v.len() / w;
    let mut s = 0f64;
    let mut n = 0usize;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let c = luma(v[y * w + x]);
            let mut m = 0.0;
            for dy in 0..3 {
                for dx in 0..3 {
                    m += luma(v[(y + dy - 1) * w + x + dx - 1]);
                }
            }
            s += (c - m / 9.0).abs() as f64;
            n += 1;
        }
    }
    s / n as f64
}

fn main() {
    let (Some(f2), Some(f3), Some(f4)) = (carta::frame("02_cst.tif"), carta::frame("03_nitidez.tif.tif"), carta::frame("04_glow.tif.tif")) else {
        eprintln!("referências ausentes");
        return;
    };
    if std::env::args().any(|a| a == "--nitidez") {
        fit_sharpen(&f2, &f3);
    }
    if std::env::args().any(|a| a == "--glow") {
        fit_glow(&f3, &f4);
    }
    if std::env::args().any(|a| a == "--refina") {
        for mode in [5u32, 0] {
            println!("== modo {mode}");
            refine_glow(&f3, &f4, mode);
        }
    }
    if std::env::args().any(|a| a == "--no") {
        refine_node(&f3, &f4);
    }
    if std::env::args().any(|a| a == "--analise") {
        analise(&f2, &f3, "02 → 03 (nitidez)");
        analise(&f3, &f4, "03 → 04 (glow)");
        println!("alta frequência: 02 {:.5}  03 {:.5}  04 {:.5}", hf(&f2, 1080), hf(&f3, 1080), hf(&f4, 1080));
    }
}

/// Separable Gaussian with explicit taps (edge clamped), radius ceil(3σ).
pub fn gauss(v: &[[f32; 3]], w: usize, sigma: f32) -> Vec<[f32; 3]> {
    let h = v.len() / w;
    let r = (3.0 * sigma).ceil().max(1.0) as i32;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|x| x / ks).collect();
    let mut t = vec![[0f32; 3]; v.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0f32; 3];
            for (j, kk) in k.iter().enumerate() {
                let xx = (x as i32 + j as i32 - r).clamp(0, w as i32 - 1) as usize;
                let p = v[y * w + xx];
                for c in 0..3 {
                    s[c] += p[c] * kk;
                }
            }
            t[y * w + x] = s;
        }
    }
    let mut o = vec![[0f32; 3]; v.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = [0f32; 3];
            for (j, kk) in k.iter().enumerate() {
                let yy = (y as i32 + j as i32 - r).clamp(0, h as i32 - 1) as usize;
                let p = t[yy * w + x];
                for c in 0..3 {
                    s[c] += p[c] * kk;
                }
            }
            o[y * w + x] = s;
        }
    }
    o
}

fn crop(v: &[[f32; 3]], w: usize, x0: usize, y0: usize, cw: usize, ch: usize) -> Vec<[f32; 3]> {
    let mut o = Vec::with_capacity(cw * ch);
    for y in y0..y0 + ch {
        o.extend_from_slice(&v[y * w + x0..y * w + x0 + cw]);
    }
    o
}

#[allow(dead_code)]
pub fn fit_sharpen(f2: &[[f32; 3]], f3: &[[f32; 3]]) {
    let (cw, ch) = (540, 960);
    let a = crop(f2, 1080, 270, 480, cw, ch);
    let b = crop(f3, 1080, 270, 480, cw, ch);
    println!("crop sem nitidez: {}", deltae::compare(&a, &b));
    let mut best = (f64::MAX, 0.0, 0.0);
    for sigma in [0.4f32, 0.5, 0.6, 0.7, 0.8, 1.0, 1.2, 1.5, 2.0, 2.5, 3.0] {
        let bl = gauss(&a, cw, sigma);
        for k in 1..=60 {
            let amt = k as f32 * 0.05;
            let o: Vec<[f32; 3]> = a.iter().zip(&bl).map(|(x, g)| [0, 1, 2].map(|c| x[c] + amt * (x[c] - g[c]))).collect();
            let s = deltae::compare(&o, &b).mean;
            if s < best.0 {
                best = (s, sigma as f64, amt as f64);
            }
        }
        println!("  sigma {sigma}: melhor até agora média {:.4} (sigma {} quantidade {:.2})", best.0, best.1, best.2);
    }
}

fn down4(v: &[[f32; 3]], w: usize) -> (usize, Vec<[f32; 3]>) {
    let h = v.len() / w;
    let (dw, dh) = (w / 4, h / 4);
    let mut o = vec![[0f32; 3]; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let mut s = [0f32; 3];
            for yy in 0..4 {
                for xx in 0..4 {
                    let p = v[(y * 4 + yy) * w + x * 4 + xx];
                    for c in 0..3 {
                        s[c] += p[c] / 16.0;
                    }
                }
            }
            o[y * dw + x] = s;
        }
    }
    (dw, o)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn blend(mode: u32, x: f32, g: f32) -> f32 {
    match mode {
        // soft light (W3C)
        0 => {
            if g <= 0.5 {
                x - (1.0 - 2.0 * g) * x * (1.0 - x)
            } else {
                let d = if x <= 0.25 { ((16.0 * x - 12.0) * x + 4.0) * x } else { x.max(0.0).sqrt() };
                x + (2.0 * g - 1.0) * (d - x)
            }
        }
        // soft light (Pegtop)
        1 => (1.0 - 2.0 * g) * x * x + 2.0 * g * x,
        // screen
        2 => 1.0 - (1.0 - x) * (1.0 - g),
        // add
        3 => x + g,
        // soft light W3C with the roles swapped (the image over the glow)
        5 => blend(0, g, x),
        // overlay
        6 => {
            if x <= 0.5 {
                2.0 * x * g
            } else {
                1.0 - 2.0 * (1.0 - x) * (1.0 - g)
            }
        }
        // multiply
        7 => x * g,
        // normal
        _ => g,
    }
}

#[allow(dead_code)]
pub fn fit_glow(f3: &[[f32; 3]], f4: &[[f32; 3]]) {
    let (w, a) = down4(f3, 1080);
    let (_, b) = down4(f4, 1080);
    let step = 3;
    let bs: Vec<[f32; 3]> = b.iter().step_by(step).copied().collect();
    println!("sem glow (1/4): {}", deltae::compare(&a.iter().step_by(step).copied().collect::<Vec<_>>(), &bs));
    let names = ["soft light W3C", "soft light Pegtop", "screen", "add", "normal"];
    let mut best = (f64::MAX, String::new());
    for t in [0.0f32, 0.25, 0.45, 0.6, 0.75] {
        let src: Vec<[f32; 3]> = a
            .iter()
            .map(|c| {
                let m = smoothstep(t, t + 0.15, luma(*c));
                c.map(|v| v * if t == 0.0 { 1.0 } else { m })
            })
            .collect();
        for sigma_full in [8.0f32, 20.0, 50.0, 100.0, 200.0, 400.0] {
            let bl = gauss(&src, w, sigma_full / 4.0);
            let blsub: Vec<[f32; 3]> = bl.iter().step_by(step).copied().collect();
            let asub: Vec<[f32; 3]> = a.iter().step_by(step).copied().collect();
            for mode in 0..5u32 {
                for gi in 1..=24 {
                    let gain = gi as f32 * 0.1;
                    for mix in [0.25f32, 0.5, 0.75, 1.0] {
                        if mode == 4 && mix == 1.0 {
                            continue;
                        }
                        let o: Vec<[f32; 3]> =
                            asub.iter().zip(&blsub).map(|(x, g)| [0, 1, 2].map(|c| x[c] + (blend(mode, x[c], g[c] * gain) - x[c]) * mix)).collect();
                        let s = deltae::compare(&o, &bs).mean;
                        if s < best.0 {
                            best = (s, format!("{} limiar {t} sigma {sigma_full} ganho {gain:.1} mistura {mix}", names[mode as usize]));
                        }
                    }
                }
            }
        }
        println!("limiar {t}: melhor até agora {:.4} — {}", best.0, best.1);
    }
}

/// Refinement of the soft-light glow: sigma, gain, mix, glow saturation, glow gamma, threshold.
#[allow(dead_code)]
pub fn refine_glow(f3: &[[f32; 3]], f4: &[[f32; 3]], mode: u32) {
    let (w, a) = down4(f3, 1080);
    let (_, b) = down4(f4, 1080);
    let step = 3;
    let bs: Vec<[f32; 3]> = b.iter().step_by(step).copied().collect();
    let asub: Vec<[f32; 3]> = a.iter().step_by(step).copied().collect();
    let eval = |q: &[f32; 8]| -> (f64, Vec<[f32; 3]>) {
        let [sigma, gain, mix, sat, gamma, t, sigma2, alpha] = *q;
        let src: Vec<[f32; 3]> = a.iter().map(|c| c.map(|v| v * smoothstep(t - 0.15, t + 0.15, luma(*c)).max(if t <= 0.0 { 1.0 } else { 0.0 }))).collect();
        let b1 = gauss(&src, w, sigma / 4.0);
        let b2 = gauss(&src, w, sigma2 / 4.0);
        let bl: Vec<[f32; 3]> = b1.iter().zip(&b2).map(|(p, q)| [0, 1, 2].map(|c| p[c] * (1.0 - alpha) + q[c] * alpha)).collect();
        let blsub: Vec<[f32; 3]> = bl.iter().step_by(step).copied().collect();
        let o: Vec<[f32; 3]> = asub
            .iter()
            .zip(&blsub)
            .map(|(x, g)| {
                let y = luma(*g);
                let g = g.map(|v| (y + sat * (v - y)).max(0.0).powf(gamma) * gain);
                [0, 1, 2].map(|c| x[c] + (blend(mode, x[c], g[c]) - x[c]) * mix)
            })
            .collect();
        (deltae::compare(&o, &bs).mean, o)
    };
    let mut q = if mode == 5 { [95.0f32, 0.9, 0.413, 1.45, 1.063, 0.238, 20.0, 0.0] } else { [165.0f32, 0.8, 0.637, 1.275, 1.0, 0.238, 20.0, 0.0] };
    let mut best = eval(&q).0;
    println!("início {best:.4}");
    let steps = [40.0f32, 0.1, 0.1, 0.2, 0.1, 0.1, 10.0, 0.1];
    for round in 0..4 {
        let k = 0.5f32.powi(round);
        let mut improved = true;
        while improved {
            improved = false;
            for i in 0..8 {
                for d in [steps[i] * k, -steps[i] * k] {
                    let mut t = q;
                    t[i] = (t[i] + d).max(if i == 0 || i == 6 { 4.0 } else { 0.0 });
                    t[7] = t[7].min(1.0);
                    let s = eval(&t).0;
                    if s < best - 1e-4 {
                        best = s;
                        q = t;
                        improved = true;
                    }
                }
            }
        }
        println!(
            "rodada {round}: {best:.4}  sigma {:.1} ganho {:.3} mistura {:.3} saturação {:.3} gama {:.3} limiar {:.3} sigma2 {:.1} alfa {:.3}",
            q[0], q[1], q[2], q[3], q[4], q[5], q[6], q[7]
        );
    }
    // residual by luma
    let (_, o) = eval(&q);
    let mut bins = vec![([0f64; 3], 0usize); 10];
    for ((x, y), z) in asub.iter().zip(&bs).zip(&o) {
        let k = ((luma(*x) * 10.0) as usize).min(9);
        for c in 0..3 {
            bins[k].0[c] += (z[c] - y[c]) as f64;
        }
        bins[k].1 += 1;
    }
    for (k, (s, n)) in bins.iter().enumerate().filter(|b| b.1.1 > 50) {
        let n = *n as f64;
        println!("  luma {:.1}: erro [{:+.4} {:+.4} {:+.4}]", k as f32 / 10.0, s[0] / n, s[1] / n, s[2] / n);
    }
}

fn down2(v: &[[f32; 3]], w: usize) -> Vec<[f32; 3]> {
    let h = v.len() / w;
    let mut o = Vec::with_capacity(w * h / 4);
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let mut s = [0f32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = v[(2 * y + dy) * w + 2 * x + dx];
                for c in 0..3 {
                    s[c] += p[c] / 4.0;
                }
            }
            o.push(s);
        }
    }
    o
}

/// The real glow node at half resolution (px_scale 0.5), refined by coordinate descent.
#[allow(dead_code)]
pub fn refine_node(f3: &[[f32; 3]], f4: &[[f32; 3]]) {
    use koda_color::eval::{Luts, apply_cpu, to_linear, to_signal};
    use koda_color::spatial::Glow;
    use koda_color::{Grade, NodeKind};
    let a = down2(f3, 1080);
    let b = down2(f4, 1080);
    let w = 540;
    let base = filmcraft_render::Image { w, h: a.len() / w, px: a.iter().flat_map(|c| [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), 1.0]).collect() };
    let run = |g: &Glow| -> Vec<[f32; 3]> {
        let mut gr = Grade::default();
        let _ = gr.add(NodeKind::Glow(*g), None, None);
        let mut img = base.clone();
        apply_cpu(&gr, &Luts::new(), &mut img, 0.5);
        img.px.chunks(4).map(|p| [to_signal(p[0]), to_signal(p[1]), to_signal(p[2])]).collect()
    };
    let score = |g: &Glow| {
        let o = run(g);
        let s = deltae::compare(&o, &b);
        s.mean + 0.05 * s.p99
    };
    let mut g = Glow::default();
    let mut best = score(&g);
    println!("início: {}", deltae::compare(&run(&g), &b));
    let names = ["threshold", "spread", "gain", "saturation", "gamma", "core", "opacity"];
    let steps = [0.05f32, 20.0, 0.1, 0.1, 0.05, 0.05, 0.05];
    for round in 0..4 {
        let k = 0.5f32.powi(round);
        let mut improved = true;
        while improved {
            improved = false;
            for i in 0..7 {
                for d in [steps[i] * k, -steps[i] * k] {
                    let mut t = g;
                    let f = match i {
                        0 => &mut t.threshold,
                        1 => &mut t.spread,
                        2 => &mut t.gain,
                        3 => &mut t.saturation,
                        4 => &mut t.gamma,
                        5 => &mut t.core,
                        _ => &mut t.opacity,
                    };
                    *f += d;
                    let t = t.sanitized();
                    let s = score(&t);
                    if s < best - 1e-4 {
                        best = s;
                        g = t;
                        improved = true;
                    }
                }
            }
        }
        println!("rodada {round}: {best:.4} {g:?}");
    }
    let _ = names;
    println!("final (1/2): {}", deltae::compare(&run(&g), &b));
}

//! Comparação lado a lado da página Cor (método do LAYOUT_PAGINA_COR.md):
//!   cargo run --release -p koda-color --example compara_pagina -- <davinci.png> <koda.png> <saída.png>
//! Gera: DaVinci | KODA | os dois sobrepostos a 50 %, com as divisões medidas da referência
//! (linhas verdes) desenhadas por cima dos três. Com o print do DaVinci (vídeo da cliente), a
//! saída fica só em docs/koda/cor/referencia/ (fora do git).
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (Some(da), Some(ko), Some(out)) = (a.first(), a.get(1), a.get(2)) else {
        eprintln!("uso: compara_pagina <davinci.png> <koda.png> <saída.png>");
        return;
    };
    let (Ok(d), Ok(k)) = (image::open(da), image::open(ko)) else {
        eprintln!("não abriu as imagens");
        return;
    };
    let (w, h) = (d.width(), d.height());
    let k = k.resize_exact(w, h, image::imageops::FilterType::Triangle).into_rgba8();
    let d = d.into_rgba8();
    let gap = 16u32;
    let mut img = image::RgbaImage::from_pixel(w * 3 + gap * 2, h, image::Rgba([20, 20, 22, 255]));
    for y in 0..h {
        for x in 0..w {
            let (p, q) = (d.get_pixel(x, y), k.get_pixel(x, y));
            img.put_pixel(x, y, *p);
            img.put_pixel(w + gap + x, y, *q);
            let m =
                image::Rgba([((p[0] as u16 + q[0] as u16) / 2) as u8, ((p[1] as u16 + q[1] as u16) / 2) as u8, ((p[2] as u16 + q[2] as u16) / 2) as u8, 255]);
            img.put_pixel(2 * (w + gap) + x, y, m);
        }
    }
    // the measured divisions (% of the window), LAYOUT_PAGINA_COR.md
    let xs = [7.5f32, 26.5, 49.5, 71.0, 71.3, 35.5];
    let ys = [3.0f32, 5.5, 51.0, 52.0, 63.5, 64.5, 67.5, 68.0, 97.0];
    let green = image::Rgba([46, 213, 115, 255]);
    for panel in 0..3 {
        let x0 = panel * (w + gap);
        for px in xs {
            let x = x0 + (w as f32 * px / 100.0) as u32;
            for y in (0..h).step_by(3) {
                img.put_pixel(x.min(img.width() - 1), y, green);
            }
        }
        for py in ys {
            let y = ((h as f32 * py / 100.0) as u32).min(h - 1);
            for x in (x0..x0 + w).step_by(3) {
                img.put_pixel(x, y, green);
            }
        }
    }
    match img.save(out) {
        Ok(()) => println!("gravado {out}"),
        Err(e) => eprintln!("{out}: {e}"),
    }
}

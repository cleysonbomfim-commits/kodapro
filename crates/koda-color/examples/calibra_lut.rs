//! LUT do nó 05 contra o DaVinci. cargo run --release -p koda-color --example calibra_lut
use koda_color::deltae;

fn load(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

fn main() {
    let (Some(c2), Some(c4), Some(c5), Some(path)) =
        (load("02_cst.tif"), load("04_glow.tif.tif"), load("05_lut.tif.tif"), koda_color::reference_file("Orson.cube"))
    else {
        eprintln!("referências ausentes");
        return;
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lut = filmcraft_color::Lut3d::parse_cube(&text).expect("cube");
    for (name, src) in [("LUT(04_glow) vs 05_lut [isola a LUT]", &c4), ("LUT(02_cst) vs 05_lut [pedido; inclui nós 03 e 04]", &c2)] {
        let tet: Vec<[f32; 3]> = src.iter().map(|c| lut.apply(*c)).collect();
        let tri: Vec<[f32; 3]> = src.iter().map(|c| lut.apply_trilinear(*c)).collect();
        println!("{name}\n  tetraédrica: {}\n  trilinear:   {}", deltae::compare(&tet, &c5), deltae::compare(&tri, &c5));
    }
    println!("04_glow vs 02_cst (efeito dos nós 03+04): {}", deltae::compare(&c4, &c2));
}

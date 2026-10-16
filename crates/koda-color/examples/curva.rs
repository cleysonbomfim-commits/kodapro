//! Análise: curva neutra do CST do DaVinci (01_global S-Log3 → 02_cst Gamma 2.4).
//! cargo run --release -p koda-color --example curva
fn load(name: &str) -> (u32, u32, Vec<[f32; 3]>) {
    let p = koda_color::reference_file(name).expect("arquivo de referência");
    let img = image::open(&p).expect("tiff").into_rgb16();
    let (w, h) = img.dimensions();
    (w, h, img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}
fn main() {
    let (_, _, a) = load("01_global.tif");
    let (_, _, b) = load("02_cst.tif");
    let (_, _, z) = load("00_log.tif");
    // 00 vs 01: how much node 01 changed
    let d01: f64 = z.iter().zip(&a).map(|(x, y)| (0..3).map(|i| (x[i] - y[i]).abs() as f64).sum::<f64>()).sum::<f64>() / (z.len() * 3) as f64;
    println!("00_log vs 01_global: diferença média de código {:.5} ({:.1} em 10 bits)", d01, d01 * 1023.0);
    let mut bins = vec![(0.0f64, 0.0f64, 0usize); 64];
    for (x, y) in a.iter().zip(&b) {
        let mx = x[0].max(x[1]).max(x[2]);
        let mn = x[0].min(x[1]).min(x[2]);
        if mx - mn > 0.01 {
            continue;
        }
        let s = (x[0] + x[1] + x[2]) / 3.0;
        let k = ((s * 64.0) as usize).min(63);
        let o = (y[0] + y[1] + y[2]) / 3.0;
        bins[k].0 += s as f64;
        bins[k].1 += o as f64;
        bins[k].2 += 1;
    }
    println!("código S-Log3 (CV)  linear de cena  ->  saída (código)  saída linear (2.4)  n");
    for (s, o, n) in bins.iter().filter(|b| b.2 > 50) {
        let (s, o) = (s / *n as f64, o / *n as f64);
        let lin = filmcraft_color::log::slog3_decode(s);
        println!("{:7.1}  {:10.5}  ->  {:7.4}  {:9.5}  {}", s * 1023.0, lin, o, o.powf(2.4), n);
    }
    // chroma: overall stats
    let mean = |v: &Vec<[f32; 3]>| {
        let mut m = [0f64; 3];
        for p in v {
            for i in 0..3 {
                m[i] += p[i] as f64;
            }
        }
        m.map(|x| x / v.len() as f64)
    };
    println!("médias 01: {:?}  02: {:?}", mean(&a), mean(&b));
}

//! Projeto de teste de desempenho do marco 7 (local, usa o vídeo da cliente):
//!   cargo run --release -p koda-color --example projeto_desempenho -- <saída.fcproj> [--sem-grade]
//! O clipe da FX30 de docs/koda/cor/referencia/ três vezes seguidas (30 s) numa sequência do
//! tamanho do clipe, cada um com o powergrade KODA completo: global (primárias), CST, nitidez,
//! glow e a LUT do nó 05.
use filmcraft_engine::Session;
use koda_color::{Grade, NodeKind};
use serde_json::{Value, json};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first().cloned() else {
        eprintln!("uso: projeto_desempenho <saída.fcproj> [--sem-grade]");
        return;
    };
    let graded = !args.iter().any(|a| a == "--sem-grade");
    // --nos global,cst,nitidez,glow,LUT: só esses nós (para medir cada um)
    let only: Option<Vec<String>> = args.iter().position(|a| a == "--nos").and_then(|i| args.get(i + 1)).map(|v| v.split(',').map(str::to_string).collect());
    let (Some(clip), Some(lut)) = (koda_color::reference_file("FXC4819.MP4"), koda_color::reference_file("Orson.cube")) else { return };
    koda_color::register();
    let mut s = Session::default();
    let run = |s: &mut Session, id: &str, p: Value| -> Value {
        match s.execute(id, p.clone()) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{id} {p}: {e}");
                Value::Null
            }
        }
    };
    run(&mut s, "file.newProject", json!({ "name": "desempenho" }));
    let r = run(&mut s, "file.import", json!({ "paths": [clip.to_string_lossy()] }));
    let Some(item) = r["items"].get(0).and_then(Value::as_u64) else {
        eprintln!("import: {r}");
        return;
    };
    run(&mut s, "file.newSequence", json!({ "name": "FX30 30s", "fromItems": [item, item, item] }));
    let reference = run(&mut s, "lut.import", json!({ "path": lut.to_string_lossy() }))["ref"].as_str().unwrap_or_default().to_string();
    let mut g = Grade::default();
    let mut l = NodeKind::from_type("lut").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] });
    let _ = l.set_param("lut", &json!(reference));
    for (name, kind) in [
        ("global", NodeKind::Primaries(Box::new(koda_color::primaries::koda_global()))),
        ("CST", NodeKind::from_type("cst").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("nitidez", NodeKind::from_type("sharpen").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("glow", NodeKind::from_type("glow").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("LUT", l),
    ] {
        if only.as_ref().is_none_or(|o| o.iter().any(|x| x == name)) {
            let _ = g.add(kind, Some(name), None);
        }
    }
    let clips: Vec<u64> = s.active_sequence().map(|q| q.video_tracks.iter().flat_map(|t| t.items.iter().map(|i| i.id.0)).collect()).unwrap_or_default();
    if graded {
        for c in &clips {
            run(&mut s, "koda.color.setGrade", json!({ "clip": c, "grade": g }));
        }
    }
    let q = s.active_sequence().map(|q| (q.settings.width, q.settings.height, q.settings.frame_rate)).unwrap_or_default();
    println!("sequência {q:?}, {} clipes, grade: {graded}", clips.len());
    run(&mut s, "file.saveAs", json!({ "path": out }));
    println!("gravado {out}");
}

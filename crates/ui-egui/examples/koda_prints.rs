//! KODA Pro: prints of the app drawn off screen (no display needed).
//!   cargo run --release -p filmcraft-ui-egui --example koda_prints -- <out dir> [edit|color|cor-grade ...]
//! `cor-grade`: the página Cor with the KODA powergrade's nodes on the demo clip under the
//! playhead (a generated LUT stands in for the commercial one). `KODA_PRINT_SIZE=1915x1049`
//! sets the window (default 1600 × 980).
use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(out) = args.first().cloned() else {
        eprintln!("uso: koda_prints <pasta> [edit|color]...");
        return;
    };
    koda_color::register();
    let mut s = Session::default();
    if let Err(e) = s.execute("file.openDemoProject", json!({})) {
        eprintln!("demo: {e}");
        return;
    }
    // Portuguese, and no in-window menu bar (on macOS the menus live in the system menu bar)
    let _ = s.execute("prefs.set", json!({"key": "general.interfaceLanguage", "value": "pt-br"}));
    let mut app = FilmcraftApp::new(s);
    app.ui.show_menu_bar = false;
    let (w, hh) = std::env::var("KODA_PRINT_SIZE")
        .ok()
        .and_then(|v| v.split_once('x').and_then(|(a, b)| Some((a.parse::<f32>().ok()?, b.parse::<f32>().ok()?))))
        .unwrap_or((1600.0, 980.0));
    if args.iter().any(|a| a == "cor-grade") {
        build_grade(&mut app);
    }
    let mut h = Harness::builder().with_size(egui::vec2(w, hh)).with_max_steps(10_000).build_eframe(move |_cc| app);
    for page in args.iter().skip(1) {
        let cmd = match page.as_str() {
            "color" | "cor-grade" => "page.color",
            "export" => "page.export",
            _ => "page.edit",
        };
        let ctx = h.ctx.clone();
        h.state_mut().apply_prefs(&ctx);
        let _ = filmcraft_ui_egui::menus::invoke(h.state_mut(), &ctx, cmd, json!({}));
        // the monitor and thumbnails come from frame workers: give them time
        for _ in 0..80 {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        match h.render() {
            Ok(img) => {
                let path = std::path::Path::new(&out).join(format!("{page}.png"));
                match img.save(&path) {
                    Ok(()) => println!("gravado {}", path.display()),
                    Err(e) => eprintln!("{}: {e}", path.display()),
                }
            }
            Err(e) => eprintln!("render: {e}"),
        }
    }
}

/// The KODA powergrade's chain on the clip under the playhead: global (primaries), CST, nitidez,
/// glow, LUT, and a second LUT node disabled.
fn build_grade(app: &mut FilmcraftApp) {
    use koda_color::{Grade, NodeKind};
    let clips = filmcraft_ui_egui::koda::cor::media_clips(app);
    let Some(clip) = filmcraft_ui_egui::koda::cor::current_clip(app, &clips) else { return };
    let cube = filmcraft_color::Lut3d::from_fn(17, |c| [(c[0] * 1.04 + 0.01).min(1.0), c[1] * 0.98 + 0.01, c[2] * 0.92 + 0.03]);
    let file = std::env::temp_dir().join("koda-print-look.cube");
    let reference = std::fs::write(&file, cube.to_cube())
        .ok()
        .and_then(|_| app.session.execute("lut.import", json!({ "path": file.to_string_lossy(), "name": "KODA look" })).ok())
        .and_then(|v| v.get("ref").and_then(|r| r.as_str()).map(str::to_string))
        .unwrap_or_default();
    let mut g = Grade::default();
    let mut lut = NodeKind::from_type("lut").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] });
    let _ = lut.set_param("lut", &json!(reference));
    for (name, kind) in [
        ("global", NodeKind::Primaries(Box::new(koda_color::primaries::koda_global()))),
        ("CST", NodeKind::from_type("cst").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("nitidez", NodeKind::from_type("sharpen").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("glow", NodeKind::from_type("glow").unwrap_or(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] })),
        ("LUT", lut.clone()),
        ("LUT", lut),
    ] {
        let _ = g.add(kind, Some(name), None);
    }
    if let Some(last) = g.nodes.last_mut() {
        last.enabled = false;
    }
    if let Err(e) = app.session.execute("koda.color.setGrade", json!({ "clip": clip, "grade": g })) {
        eprintln!("grade: {e}");
    }
    app.session.state.selection = vec![clip];
}

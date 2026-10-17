//! KODA Pro: prints of the app drawn off screen (no display needed).
//!   cargo run --release -p filmcraft-ui-egui --example koda_prints -- <out dir> [edit|color ...]
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
    let mut s = Session::default();
    if let Err(e) = s.execute("file.openDemoProject", json!({})) {
        eprintln!("demo: {e}");
        return;
    }
    // Portuguese, and no in-window menu bar (on macOS the menus live in the system menu bar)
    let _ = s.execute("prefs.set", json!({"key": "general.interfaceLanguage", "value": "pt-br"}));
    let mut app = FilmcraftApp::new(s);
    app.ui.show_menu_bar = false;
    let mut h = Harness::builder().with_size(egui::vec2(1600.0, 980.0)).with_max_steps(10_000).build_eframe(move |_cc| app);
    for page in args.iter().skip(1) {
        let cmd = match page.as_str() {
            "color" => "page.color",
            "export" => "page.export",
            _ => "page.edit",
        };
        let ctx = h.ctx.clone();
        h.state_mut().apply_prefs(&ctx);
        let _ = filmcraft_ui_egui::menus::invoke(h.state_mut(), &ctx, cmd, json!({}));
        for _ in 0..6 {
            h.step();
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

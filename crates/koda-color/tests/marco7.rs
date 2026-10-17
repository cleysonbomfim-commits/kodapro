//! Marco 7 da aba Cor: the page state saved beside the project (never an undo step), and the
//! double-conversion check (a clip interpreted as a camera space with a CST in its grade).

use filmcraft_engine::Session;
use filmcraft_project::{ClipId, ItemKind};
use serde_json::{Value, json};

fn demo() -> (Session, Vec<(ClipId, filmcraft_project::ItemId)>) {
    koda_color::register();
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).expect("demo project");
    let seq = s.active_sequence().expect("sequence");
    let clips = seq
        .video_tracks
        .iter()
        .flat_map(|t| t.items.iter())
        .filter(|i| s.project.item(i.item).is_some_and(|p| matches!(p.kind, ItemKind::Media(_))))
        .map(|i| (i.id, i.item))
        .collect();
    (s, clips)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

#[test]
fn the_page_state_is_saved_beside_the_project() {
    let (mut s, c) = demo();
    let n = run(&mut s, "koda.color.addNode", json!({ "clip": c[0].0, "type": "primaries" }))["result"]["node"].as_u64().unwrap();
    let undo_before = s.history.undo.len();
    run(&mut s, "koda.color.setPageState", json!({ "palette": "curves", "clip": c[0].0, "node": n }));
    assert_eq!(s.history.undo.len(), undo_before, "not an undo step");
    let dir = std::env::temp_dir().join(format!("koda-m7-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("p.fcproj");
    run(&mut s, "file.saveAs", json!({ "path": path.to_string_lossy() }));
    let mut t = Session::default();
    run(&mut t, "file.open", json!({ "path": path.to_string_lossy() }));
    let st = run(&mut t, "koda.color.pageState", json!({}));
    assert_eq!(st["palette"], "curves");
    assert_eq!(st["nodes"][c[0].0.0.to_string()], n);
    // hostile input
    assert!(t.execute("koda.color.setPageState", json!({ "palette": "qualifier" })).is_err());
    assert!(t.execute("koda.color.setPageState", json!({ "clip": c[0].0, "node": "x" })).is_err());
    // a damaged state in the file reads as the defaults
    t.state.view_extensions.insert(koda_color::page::KEY.into(), json!({ "palette": 7, "nodes": "no" }));
    assert_eq!(run(&mut t, "koda.color.pageState", json!({}))["palette"], "primaries");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cst_on_a_clip_interpreted_as_log_is_flagged() {
    let (mut s, c) = demo();
    let (clip, item) = c[0];
    let warn = |s: &mut Session| run(s, "koda.color.inspect", json!({ "clip": clip }))["doubleConversion"].clone();
    run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "cst" }));
    assert!(warn(&mut s).is_null(), "default interpretation: no warning");
    run(&mut s, "clip.interpretFootage", json!({ "items": [item], "colorSpace": "slog3-sgamut3cine" }));
    let w = warn(&mut s);
    assert!(w.as_str().is_some_and(|w| w.contains("S-Log3")), "{w}");
    // the CST disabled: nothing converts twice
    let node = run(&mut s, "koda.color.inspect", json!({ "clip": clip }))["grade"]["nodes"][0]["id"].clone();
    run(&mut s, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": node, "enabled": false }));
    assert!(warn(&mut s).is_null());
    run(&mut s, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": node, "enabled": true }));
    run(&mut s, "clip.interpretFootage", json!({ "items": [item], "colorSpace": "auto" }));
    assert!(warn(&mut s).is_null(), "back to the default interpretation");
}

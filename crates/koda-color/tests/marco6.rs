//! Marco 6 da aba Cor (engine only): powergrades saved to files and applied to clips, copying a
//! grade from one clip to others, copying and pasting a node — all `koda.color.*` commands, each
//! edit one undo step. The clipboard is process-wide, so its checks live in one test.

use filmcraft_color::Lut3d;
use filmcraft_engine::Session;
use filmcraft_project::{ClipId, ItemKind};
use koda_color::{Grade, NodeKind};
use serde_json::{Value, json};

fn demo() -> (Session, Vec<ClipId>) {
    koda_color::register();
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).expect("demo project");
    let seq = s.active_sequence().expect("sequence");
    let clips: Vec<ClipId> = seq
        .video_tracks
        .iter()
        .flat_map(|t| t.items.iter())
        .filter(|i| s.project.item(i.item).is_some_and(|p| matches!(p.kind, ItemKind::Media(_))))
        .map(|i| i.id)
        .collect();
    assert!(clips.len() >= 3, "the demo has several media clips");
    (s, clips)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn grade(s: &mut Session, clip: ClipId) -> Grade {
    serde_json::from_value(run(s, "koda.color.inspect", json!({ "clip": clip }))["grade"].clone()).unwrap()
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("koda-m6-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A grade with primaries, a LUT from the project's library and a disabled glow.
fn build(s: &mut Session, clip: ClipId, dir: &std::path::Path) -> Grade {
    run(s, "koda.color.addNode", json!({ "clip": clip, "type": "primaries", "name": "global" }));
    let n = grade(s, clip).nodes[0].id;
    run(s, "koda.color.setParam", json!({ "clip": clip, "node": n, "param": "temp", "value": -69.9 }));
    let lut = run(s, "koda.color.addNode", json!({ "clip": clip, "type": "lut" }))["result"]["node"].clone();
    let file = dir.join("frio.cube");
    std::fs::write(&file, Lut3d::from_fn(9, |c| [c[0] * 0.9, c[1], c[2].min(1.0) * 0.95 + 0.05]).to_cube()).unwrap();
    run(s, "koda.color.loadLut", json!({ "clip": clip, "node": lut, "path": file.to_string_lossy() }));
    let glow = run(s, "koda.color.addNode", json!({ "clip": clip, "type": "glow" }))["result"]["node"].clone();
    run(s, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": glow, "enabled": false }));
    grade(s, clip)
}

fn kinds(g: &Grade) -> Vec<(String, bool, String)> {
    g.nodes.iter().map(|n| (n.name.clone(), n.enabled, serde_json::to_string(&n.kind).unwrap())).collect()
}

#[test]
fn copy_and_paste_grades_and_nodes() {
    let (mut s, c) = demo();
    let dir = tmp("clip");
    let g0 = build(&mut s, c[0], &dir);

    // copy a grade, paste it onto two clips: one undo step for both
    run(&mut s, "koda.color.copyGrade", json!({ "clip": c[0] }));
    run(&mut s, "koda.color.pasteGrade", json!({ "clips": [c[1], c[2]] }));
    assert_eq!(kinds(&grade(&mut s, c[1])), kinds(&g0));
    assert_eq!(kinds(&grade(&mut s, c[2])), kinds(&g0));
    assert_eq!(s.project.luts.len(), 1, "the LUT is reused, not imported again");
    s.execute("edit.undo", json!({})).unwrap();
    assert!(grade(&mut s, c[1]).nodes.is_empty() && grade(&mut s, c[2]).nodes.is_empty());
    s.execute("edit.redo", json!({})).unwrap();
    assert_eq!(kinds(&grade(&mut s, c[2])), kinds(&g0));

    // append after the clip's own nodes; the selection is the target when no clip is named
    s.state.selection = vec![c[1]];
    run(&mut s, "koda.color.pasteGrade", json!({ "mode": "append" }));
    let g1 = grade(&mut s, c[1]);
    assert_eq!(g1.nodes.len(), 6);
    let ids: std::collections::BTreeSet<u32> = g1.nodes.iter().map(|n| n.id).collect();
    assert_eq!(ids.len(), 6, "appended nodes get new ids");

    // apply grade from a clip to others
    run(&mut s, "koda.color.applyGradeFrom", json!({ "from": c[1], "clips": [c[2], c[1]] }));
    assert_eq!(kinds(&grade(&mut s, c[2])), kinds(&g1));

    // copy a node; paste it as a new node, then onto another node's settings
    let lut_node = g0.nodes[1].id;
    run(&mut s, "koda.color.copyNode", json!({ "clip": c[0], "node": lut_node }));
    assert!(s.execute("koda.color.pasteGrade", json!({ "clips": [c[2]] })).is_err(), "the clipboard holds a node");
    let new = run(&mut s, "koda.color.pasteNode", json!({ "clip": c[0], "index": 0 }))["node"].as_u64().unwrap() as u32;
    let g = grade(&mut s, c[0]);
    assert_eq!(g.nodes[0].id, new);
    assert_eq!(g.nodes[0].kind, g0.nodes[1].kind);
    let prim = g0.nodes[0].id;
    run(&mut s, "koda.color.pasteNode", json!({ "clip": c[0], "replace": prim }));
    let g = grade(&mut s, c[0]);
    let replaced = g.nodes.iter().find(|n| n.id == prim).unwrap();
    assert_eq!((replaced.name.as_str(), replaced.kind.type_name()), ("global", "lut"), "id and name kept, settings replaced");
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(grade(&mut s, c[0]).nodes.iter().find(|n| n.id == prim).unwrap().kind.type_name(), "primaries");

    // hostile
    for (id, p) in [
        ("koda.color.pasteNode", json!({ "clip": c[0], "replace": 9999 })),
        ("koda.color.pasteNode", json!({ "clip": c[0], "index": "a" })),
        ("koda.color.copyNode", json!({ "clip": c[0], "node": 9999 })),
        ("koda.color.pasteGrade", json!({ "clips": [c[1]], "mode": "merge" })),
        ("koda.color.applyGradeFrom", json!({ "from": c[1], "clips": [c[1]] })),
        ("koda.color.applyGradeFrom", json!({ "clips": [c[1]] })),
        ("koda.color.copyGrade", json!({ "clip": 123456789 })),
    ] {
        assert!(s.execute(id, p.clone()).is_err(), "{id} {p}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn powergrades_travel_between_projects() {
    let (mut s, c) = demo();
    let dir = tmp("pg");
    let g0 = build(&mut s, c[0], &dir);
    let file = dir.join("lib").join("KODA teste.kpg");
    let r = run(&mut s, "koda.color.savePowergrade", json!({ "clip": c[0], "name": "KODA teste", "path": file.to_string_lossy() }));
    assert_eq!((r["nodes"].as_u64(), r["luts"].as_u64()), (Some(3), Some(1)));
    assert!(file.is_file());
    let listed = run(&mut s, "koda.color.listPowergrades", json!({ "dir": dir.join("lib").to_string_lossy() }));
    assert_eq!(listed["powergrades"][0]["name"], "KODA teste");

    // a fresh project: applying brings the LUT into its library
    let (mut t, d) = demo();
    assert!(t.project.luts.is_empty());
    run(&mut t, "koda.color.applyPowergrade", json!({ "path": file.to_string_lossy(), "clips": [d[0], d[1]] }));
    assert_eq!(t.project.luts.len(), 1);
    let g = grade(&mut t, d[1]);
    assert_eq!(g.nodes.len(), 3);
    let NodeKind::Lut { lut, .. } = &g.nodes[1].kind else { panic!("{g:?}") };
    assert_eq!(lut, &format!("lib:{}", t.project.luts[0].id), "the node points at the imported LUT");
    assert_eq!(&*t.project.luts[0].text, &*s.project.luts[0].text);
    assert_eq!(g.nodes[0].kind, g0.nodes[0].kind);
    assert!(!g.nodes[2].enabled);
    // again: the LUT is not duplicated; one undo step undoes one apply
    run(&mut t, "koda.color.applyPowergrade", json!({ "path": file.to_string_lossy(), "clips": [d[2]], "mode": "append" }));
    assert_eq!(t.project.luts.len(), 1);
    t.execute("edit.undo", json!({})).unwrap();
    assert!(grade(&mut t, d[2]).nodes.is_empty());
    assert_eq!(grade(&mut t, d[1]).nodes.len(), 3);

    // hostile files
    let bad = dir.join("ruim.kpg");
    for text in
        ["", "{", "[1,2,3]", r#"{"format":"koda-powergrade","version":7,"name":"x","grade":{}}"#, r#"{"format":"outra","version":1,"name":"x","grade":{}}"#]
    {
        std::fs::write(&bad, text).unwrap();
        assert!(t.execute("koda.color.applyPowergrade", json!({ "path": bad.to_string_lossy(), "clips": [d[0]] })).is_err(), "{text}");
    }
    assert!(t.execute("koda.color.applyPowergrade", json!({ "path": dir.join("nao-existe.kpg").to_string_lossy(), "clips": [d[0]] })).is_err());
    assert!(t.execute("koda.color.applyPowergrade", json!({ "path": dir.to_string_lossy(), "clips": [d[0]] })).is_err(), "a folder");
    assert!(t.execute("koda.color.savePowergrade", json!({ "clip": d[2], "name": "vazio", "path": dir.join("v.kpg").to_string_lossy() })).is_err(), "no grade");
    let listed = run(&mut t, "koda.color.listPowergrades", json!({ "dir": dir.to_string_lossy() }));
    assert!(listed["powergrades"].as_array().unwrap().iter().any(|p| p.get("error").is_some()), "broken files are listed with their error");
    let _ = std::fs::remove_dir_all(&dir);
}

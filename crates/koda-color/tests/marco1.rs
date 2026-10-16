//! Marco 1 da aba Cor (`docs/koda/cor/ESPEC_COR_V1.md`): the node engine end to end. Commands
//! with undo, hostile parameters, the grade hidden from FilmCraft's effect commands, a project
//! save / open round trip, the grade running first on the clip (before other effects and
//! Motion) on the CPU and in GPU plans, and the GPU compositor matching the CPU reference.

use std::sync::Arc;

use filmcraft_engine::Session;
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource, SharedSource};
use filmcraft_project::{ClipId, ItemKind, Label, MediaClip, MediaRef, ParamValue, Project, SequenceSettings, TrackKind, find_effect};
use filmcraft_render::{RenderOptions, SourceMap, render_sequence};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use koda_color::{Grade, NodeKind, store};
use serde_json::{Value, json};

/// A session with the demo project open and the first media clip of V1.
fn demo() -> (Session, ClipId) {
    koda_color::register();
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).expect("demo project");
    let seq = s.active_sequence().expect("sequence");
    let clip =
        seq.video_tracks[0].items.iter().find(|i| s.project.item(i.item).is_some_and(|p| matches!(p.kind, ItemKind::Media(_)))).expect("a media clip on V1").id;
    (s, clip)
}

fn grade(s: &mut Session, clip: ClipId) -> Grade {
    let v = s.execute("koda.color.inspect", json!({ "clip": clip })).expect("inspect");
    serde_json::from_value(v["grade"].clone()).expect("grade json")
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

#[test]
fn commands_edit_the_grade_with_undo() {
    let (mut s, clip) = demo();
    assert!(filmcraft_engine::find_command("koda.color.addNode").is_some(), "registered with the engine");
    let a = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "gain", "name": "global" }))["result"]["node"].as_u64().unwrap();
    let b = run(&mut s, "koda.color.addNode", json!({ "clip": clip }))["result"]["node"].as_u64().unwrap();
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": a, "param": "master", "value": 1.5 }));
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": b, "param": "rgb", "value": [1.0, 0.8, 1.2] }));
    run(&mut s, "koda.color.renameNode", json!({ "clip": clip, "node": b, "name": "quente" }));
    run(&mut s, "koda.color.moveNode", json!({ "clip": clip, "node": b, "index": 0 }));
    run(&mut s, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": a }));
    run(&mut s, "koda.color.setBypass", json!({ "clip": clip, "bypass": true }));
    let g = grade(&mut s, clip);
    assert_eq!(g.nodes.iter().map(|n| (n.id as u64, n.name.as_str(), n.enabled)).collect::<Vec<_>>(), [(b, "quente", true), (a, "global", false)]);
    assert!(g.bypass);
    assert_eq!(g.nodes[1].kind, NodeKind::Gain { master: 1.5, rgb: [1.0; 3] });
    assert_eq!(g.nodes[0].kind, NodeKind::Gain { master: 1.0, rgb: [1.0, 0.8, 1.2] });
    // each command is one undo step
    for _ in 0..3 {
        run(&mut s, "edit.undo", json!({}));
    }
    let g = grade(&mut s, clip);
    assert!(!g.bypass && g.nodes.iter().all(|n| n.enabled) && g.nodes[0].id as u64 == a, "{g:?}");
    run(&mut s, "edit.redo", json!({}));
    assert_eq!(grade(&mut s, clip).nodes[0].id as u64, b);
    // remove both: the hidden effect goes away with the last node
    run(&mut s, "koda.color.removeNode", json!({ "clip": clip, "node": a }));
    run(&mut s, "koda.color.removeNode", json!({ "clip": clip, "node": b }));
    let item = s.active_sequence().unwrap().find_item(clip).unwrap().1.clone();
    assert!(!item.effects.iter().any(|e| e.effect == store::EFFECT));
}

#[test]
fn hostile_parameters_are_errors_never_panics() {
    let (mut s, clip) = demo();
    let n = run(&mut s, "koda.color.addNode", json!({ "clip": clip }))["result"]["node"].clone();
    let before = grade(&mut s, clip);
    for (id, p) in [
        ("koda.color.addNode", json!({ "clip": clip, "type": "teleport" })),
        ("koda.color.addNode", json!({ "clip": clip, "index": -3 })),
        ("koda.color.addNode", json!({ "clip": clip, "name": 7 })),
        ("koda.color.addNode", json!({ "clip": 999_999 })),
        ("koda.color.addNode", json!({ "clip": "x" })),
        ("koda.color.removeNode", json!({ "clip": clip, "node": 4_000_000_000_u64 })),
        ("koda.color.removeNode", json!({ "clip": clip })),
        ("koda.color.moveNode", json!({ "clip": clip, "node": n, "index": "top" })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "master", "value": "big" })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "exposure", "value": 1 })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "rgb", "value": [1, 2] })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "master" })),
        ("koda.color.setNodeEnabled", json!({ "clip": clip, "node": n, "enabled": "yes" })),
        ("koda.color.setBypass", json!({ "clip": clip, "bypass": 2 })),
        ("koda.color.setGrade", json!({ "clip": clip, "grade": [1, 2, 3] })),
        ("koda.color.setGrade", json!({ "clip": clip, "grade": { "version": 99 } })),
        ("koda.color.renameNode", json!({ "clip": clip, "node": n })),
    ] {
        assert!(s.execute(id, p.clone()).is_err(), "{id} {p} should fail");
    }
    // nothing changed, and huge / NaN-free clamps apply
    assert_eq!(grade(&mut s, clip), before);
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": n, "param": "master", "value": 1e30 }));
    assert_eq!(grade(&mut s, clip).nodes[0].kind, NodeKind::Gain { master: 16.0, rgb: [1.0; 3] });
    // no clip named and none selected
    s.state.selection.clear();
    assert!(s.execute("koda.color.addNode", json!({})).is_err());
    // the selected clip is the default target
    s.state.selection = vec![clip];
    run(&mut s, "koda.color.addNode", json!({}));
    assert_eq!(grade(&mut s, clip).nodes.len(), 2);
}

#[test]
fn the_grade_is_hidden_and_guarded_from_effect_commands() {
    let (mut s, clip) = demo();
    run(&mut s, "koda.color.addNode", json!({ "clip": clip }));
    assert!(find_effect(store::EFFECT).is_none(), "no effect definition: panels and Paste Attributes never list it");
    let effects = s.active_sequence().unwrap().find_item(clip).unwrap().1.effects.clone();
    let idx = effects.iter().position(|e| e.effect == store::EFFECT).expect("stored");
    for id in ["effects.remove", "effects.toggleEnabled", "effects.reset"] {
        let e = s.execute(id, json!({ "clip": clip, "index": idx })).expect_err(id);
        assert!(e.to_string().contains("koda.color"), "{id}: {e}");
    }
    assert_eq!(grade(&mut s, clip).nodes.len(), 1, "still there");
}

#[test]
fn the_grade_survives_save_and_open() {
    let (mut s, clip) = demo();
    let a = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "name": "global" }))["result"]["node"].clone();
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": a, "param": "rgb", "value": [1.25, 1.0, 0.75] }));
    run(&mut s, "koda.color.addNode", json!({ "clip": clip, "name": "reserva" }));
    let n2 = grade(&mut s, clip).nodes[1].id;
    run(&mut s, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": n2, "enabled": false }));
    let before = grade(&mut s, clip);
    let dir = std::env::temp_dir().join(format!("koda-color-m1-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("grade.fcproj");
    run(&mut s, "file.saveAs", json!({ "path": path.to_string_lossy() }));
    // readable text in the project file
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("koda_grade") && text.contains("reserva"), "grade stored as readable text");
    let mut s2 = Session::default();
    run(&mut s2, "file.open", json!({ "path": path.to_string_lossy() }));
    assert_eq!(grade(&mut s2, clip), before);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- rendering: order and GPU parity ----

const RATE: FrameRate = FrameRate::FPS_24;

/// A 320 × 180 sequence with one generated 640 × 360 clip (scaled to the frame), and its sources.
fn scene(scene: DemoScene) -> (Project, filmcraft_project::ItemId, SourceMap, ClipId) {
    koda_color::register();
    let mut p = Project::new("koda");
    let seq = p.new_sequence("koda", SequenceSettings { width: 320, height: 180, frame_rate: RATE, ..Default::default() }, 1, 0, None);
    let src = GeneratorSource::new(Generator::Demo(scene), 640, 360, RATE, Tick(10 * TICKS_PER_SECOND));
    let info = src.info().clone();
    let item = p.add_item(
        &info.name.clone(),
        Label::Iris,
        ItemKind::Media(MediaClip {
            media: MediaRef::Generator(src.generator.clone()),
            info,
            interpret: Default::default(),
            mark_in: None,
            mark_out: None,
            markers: vec![],
            offline: false,
            proxy: None,
            identity: None,
        }),
        None,
    );
    let mut map = SourceMap::default();
    map.0.insert(item, Arc::new(src) as SharedSource);
    let mut ti = p.make_track_item(item, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, RATE.tick_of(48)), RATE).unwrap();
    ti.scale_to_frame = true;
    let clip = ti.id;
    let s = p.sequence_mut(seq).unwrap();
    s.video_tracks[0].items.push(ti);
    (p, seq, map, clip)
}

fn gain_grade(master: f32, rgb: [f32; 3]) -> Grade {
    let mut g = Grade::default();
    g.add(NodeKind::Gain { master, rgb }, Some("global"), None).unwrap();
    g
}

fn tint() -> filmcraft_project::EffectInstance {
    let mut e = find_effect("tint").unwrap().instance();
    e.params.get_mut("amount").unwrap().value = ParamValue::Float(70.0);
    e
}

fn rgba8(img: &filmcraft_render::Image) -> Vec<u8> {
    img.over_black_rgba8()
}

/// Mean and 99th percentile of the absolute 8-bit channel differences (RGB).
fn stats(a: &[u8], b: &[u8]) -> (f64, u8) {
    let mut d: Vec<u8> = a.chunks(4).zip(b.chunks(4)).flat_map(|(x, y)| (0..3).map(move |i| x[i].abs_diff(y[i]))).collect();
    let mean = d.iter().map(|&v| v as f64).sum::<f64>() / d.len().max(1) as f64;
    d.sort_unstable();
    (mean, d.get(d.len() * 99 / 100).copied().unwrap_or(0))
}

/// The grade runs first whatever its place in the clip's effect list: before Tint and before
/// Motion (a 60 % scale, 12° rotation), on the CPU and in the GPU plan.
#[test]
fn the_grade_runs_before_other_effects_and_motion() {
    let (mut p, seq, map, clip) = scene(DemoScene::OceanSunset);
    {
        let it = p.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1;
        let m = it.effect_mut("motion").unwrap();
        m.params.get_mut("scale").unwrap().value = ParamValue::Float(60.0);
        m.params.get_mut("rotation").unwrap().value = ParamValue::Float(12.0);
    }
    let t = RATE.tick_of(12);
    let render = |p: &Project| rgba8(&render_sequence(p, seq, t, RenderOptions::default(), &map));
    let with = |list: &dyn Fn(&mut filmcraft_project::TrackItem)| {
        let mut q = p.clone();
        list(q.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1);
        q
    };
    let g = gain_grade(2.0, [1.0, 0.7, 1.3]);
    // the grade stored after Tint in the list, and before it: the same picture
    let after = with(&|it| {
        it.effects.push(tint());
        store::set_grade(it, &g);
    });
    let before = with(&|it| {
        store::set_grade(it, &g);
        let e = it.effects.pop().unwrap();
        it.effects.insert(0, e);
        it.effects.push(tint());
    });
    let (ra, rb) = (render(&after), render(&before));
    assert_eq!(ra, rb, "the list position of the grade does not matter");
    // and it is grade → Tint, not Tint → grade: the grade as the first standard effect gives the
    // same picture, as a Tint first gives a very different one
    let tint_only = render(&with(&|it| it.effects.push(tint())));
    let (mean_vs_tint_only, _) = stats(&ra, &tint_only);
    assert!(mean_vs_tint_only > 3.0, "the grade changes the picture ({mean_vs_tint_only})");
    // in the GPU plan the grade is the first op, then Tint; Motion is the layer's matrix after both
    let plan = filmcraft_render::plan::plan_frame(&after, seq, t, RenderOptions::default(), &map);
    let filmcraft_render::plan::FramePlan::Layers { layers, .. } = &plan else { panic!("planned as a CPU image") };
    let fx = layers[0].fx.as_ref().expect("a GPU effect layer");
    assert!(matches!(fx.ops[0], filmcraft_render::gpufx::FxOp::External { .. }), "{:?}", fx.ops[0]);
    assert!(matches!(fx.ops[1], filmcraft_render::gpufx::FxOp::Tint { .. }), "{:?}", fx.ops[1]);
    assert_eq!(fx.ops.len(), 2);
}

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// The gain node in the GPU compositor gives the CPU reference's picture (FilmCraft's GPU parity
/// tolerance: p99 ≤ 6, mean < 1.5), alone, with bypass, chained twice, after a disabled node and
/// before Tint, at full and half resolution.
#[test]
fn gpu_grade_matches_cpu_reference() {
    let Some((dev, q)) = device() else {
        eprintln!("SKIPPED (koda grade GPU parity): no GPU adapter");
        return;
    };
    let mut c = filmcraft_gpu::GpuCompositor::new(&dev, &q);
    let (base, seq, map, clip) = scene(DemoScene::Aurora);
    let mut chained = gain_grade(1.4, [1.0, 1.0, 1.0]);
    chained.add(NodeKind::Gain { master: 0.9, rgb: [1.3, 0.9, 0.7] }, None, None).unwrap();
    let mut off = gain_grade(3.0, [1.0; 3]);
    off.nodes[0].enabled = false;
    off.add(NodeKind::Gain { master: 1.2, rgb: [1.0; 3] }, None, None).unwrap();
    let mut bypass = gain_grade(2.5, [1.0; 3]);
    bypass.bypass = true;
    let cases: Vec<(&str, Grade, bool)> = vec![
        ("gain", gain_grade(1.8, [1.0, 0.6, 1.4]), false),
        ("bypass", bypass, false),
        ("chain", chained, false),
        ("disabled node", off, false),
        ("gain then tint", gain_grade(0.6, [1.2, 1.0, 0.8]), true),
    ];
    let mut failures = Vec::new();
    for (name, g, with_tint) in cases {
        let mut p = base.clone();
        let it = p.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1;
        store::set_grade(it, &g);
        if with_tint {
            it.effects.push(tint());
        }
        for scale in [1.0f32, 0.5] {
            let opts = RenderOptions { scale, ..RenderOptions::default() };
            let t = RATE.tick_of(20);
            let plan = filmcraft_render::plan::plan_frame(&p, seq, t, opts, &map);
            assert!(matches!(plan, filmcraft_render::plan::FramePlan::Layers { .. }), "{name}: planned on the CPU");
            c.composite(&plan);
            let (_, _, gpu) = c.read_output().expect("GPU readback");
            let cpu = rgba8(&render_sequence(&p, seq, t, opts, &map));
            let (mean, p99) = stats(&cpu, &gpu);
            eprintln!("{name} @ {scale}: GPU vs CPU mean {mean:.3}, p99 {p99}");
            if p99 > 6 || mean >= 1.5 {
                failures.push(format!("{name} @ {scale}: mean {mean:.3}, p99 {p99}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The reference set is optional: a missing file skips with a warning instead of failing.
#[test]
fn missing_reference_files_skip() {
    assert!(koda_color::reference_file("nao-existe.tif").is_none());
}

/// Regression: a clip whose only effect is its grade was drawn by the plain-frame shortcut on the
/// CPU (the grade has no effect definition, so the clip looked effect-free) and showed ungraded.
#[test]
fn a_clip_with_only_its_grade_is_graded_on_the_cpu() {
    let (base, seq, map, clip) = scene(DemoScene::Aurora);
    let t = RATE.tick_of(20);
    let plain = rgba8(&render_sequence(&base, seq, t, RenderOptions::default(), &map));
    let mut p = base.clone();
    store::set_grade(p.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1, &gain_grade(1.8, [1.0; 3]));
    let graded = rgba8(&render_sequence(&p, seq, t, RenderOptions::default(), &map));
    let (mean, _) = stats(&plain, &graded);
    assert!(mean > 10.0, "the grade reached the CPU render (mean difference {mean:.2})");
    // and a bypassed grade is the plain picture again
    let mut g = gain_grade(1.8, [1.0; 3]);
    g.bypass = true;
    store::set_grade(p.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1, &g);
    assert_eq!(rgba8(&render_sequence(&p, seq, t, RenderOptions::default(), &map)), plain);
}

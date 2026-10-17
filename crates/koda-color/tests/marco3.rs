//! Marco 3 da aba Cor: primaries and curves. Against DaVinci's node 01 (the owner's local
//! reference set, skipped with a warning when missing), GPU against CPU in the compositor, and
//! the commands.

use std::sync::Arc;

use filmcraft_engine::Session;
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource, SharedSource};
use filmcraft_project::{ClipId, ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_render::{RenderOptions, SourceMap, render_sequence};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use koda_color::primaries::{CurvePoints, Primaries};
use koda_color::{Grade, NodeKind, deltae, store};
use serde_json::{Value, json};

fn frame(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

fn grade_of(kind: NodeKind) -> Grade {
    let mut g = Grade::default();
    g.add(kind, None, None).unwrap();
    g
}

fn node_01() -> Primaries {
    koda_color::primaries::koda_global()
}

/// Primaries on DaVinci's raw log frame match DaVinci's node-01 output (ESPEC_COR_V1: mean
/// ΔE 2000 < 1, max < 3).
#[test]
fn primaries_match_davinci_node_01() {
    let (Some(src), Some(want)) = (frame("00_log.tif"), frame("01_global.tif")) else { return };
    use rayon::prelude::*;
    let r = node_01().ready();
    let ours: Vec<[f32; 3]> = src.par_iter().map(|c| r.apply(*c)).collect();
    let s = deltae::compare(&ours, &want);
    eprintln!("Primárias(00_log) × 01_global: {s}");
    assert!(s.mean < 1.0 && s.max < 3.0, "{s}");
    // and through the grade (apply_signal), the same numbers
    let g = grade_of(NodeKind::Primaries(Box::new(node_01())));
    let via: Vec<[f32; 3]> = src.par_iter().step_by(97).map(|c| koda_color::eval::apply_signal(&g, &Default::default(), *c)).collect();
    let direct: Vec<[f32; 3]> = ours.iter().step_by(97).copied().collect();
    assert_eq!(via, direct);
}

// ---- GPU against CPU in the compositor ----

const RATE: FrameRate = FrameRate::FPS_24;

fn scene() -> (Project, filmcraft_project::ItemId, SourceMap, ClipId) {
    koda_color::register();
    let mut p = Project::new("koda");
    let seq = p.new_sequence("koda", SequenceSettings { width: 320, height: 180, frame_rate: RATE, ..Default::default() }, 1, 0, None);
    let src = GeneratorSource::new(Generator::Demo(DemoScene::Dunes), 640, 360, RATE, Tick(10 * TICKS_PER_SECOND));
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
    p.sequence_mut(seq).unwrap().video_tracks[0].items.push(ti);
    (p, seq, map, clip)
}

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

fn stats(a: &[u8], b: &[u8]) -> (f64, u8) {
    let mut d: Vec<u8> = a.chunks(4).zip(b.chunks(4)).flat_map(|(x, y)| (0..3).map(move |i| x[i].abs_diff(y[i]))).collect();
    let mean = d.iter().map(|&v| v as f64).sum::<f64>() / d.len().max(1) as f64;
    d.sort_unstable();
    (mean, d.get(d.len() * 99 / 100).copied().unwrap_or(0))
}

/// Every primaries control, the curves and Mid Detail give the CPU reference's picture on the GPU,
/// full and half resolution.
#[test]
fn gpu_primaries_match_cpu_reference() {
    let Some((dev, q)) = device() else {
        eprintln!("SKIPPED (koda primaries GPU parity): no GPU adapter");
        return;
    };
    let mut c = filmcraft_gpu::GpuCompositor::new(&dev, &q);
    let (base, seq, map, clip) = scene();
    let prim = |f: &dyn Fn(&mut Primaries)| {
        let mut p = Primaries::default();
        f(&mut p);
        grade_of(NodeKind::Primaries(Box::new(p.sanitized())))
    };
    let cases: Vec<(&str, Grade)> = vec![
        (
            "lift gamma gain",
            prim(&|p| {
                p.lift = [0.05, 0.02, -0.03, 0.0];
                p.gamma = [0.2, 0.0, 0.1, -0.1];
                p.gain = [1.2, 1.1, 1.0, 0.9];
            }),
        ),
        (
            "offset contrast pivot",
            prim(&|p| {
                p.offset = [27.0, 24.0, 22.0];
                p.contrast = 1.4;
                p.pivot = 0.5;
            }),
        ),
        (
            "temp tint lum mix",
            prim(&|p| {
                p.temp = -800.0;
                p.tint = 20.0;
                p.gain[0] = 1.3;
                p.lum_mix = 40.0;
            }),
        ),
        (
            "shadows highlights boost sat hue",
            prim(&|p| {
                p.shadows = 40.0;
                p.highlights = -30.0;
                p.color_boost = 50.0;
                p.sat = 70.0;
                p.hue = 58.0;
            }),
        ),
        (
            "curves",
            prim(&|p| {
                p.curves.y = CurvePoints(vec![[0.0, 0.05], [0.3, 0.22], [0.7, 0.8], [1.0, 0.97]]).sanitized();
                p.curves.linked = false;
                p.curves.r = CurvePoints(vec![[0.0, 0.0], [0.5, 0.56], [1.0, 1.0]]).sanitized();
                p.curves.b = CurvePoints(vec![[0.0, 0.03], [0.5, 0.45], [1.0, 1.0]]).sanitized();
            }),
        ),
        ("mid detail", prim(&|p| p.mid_detail = 60.0)),
        ("mid detail + node 01", {
            let mut p = node_01();
            p.mid_detail = -40.0;
            grade_of(NodeKind::Primaries(Box::new(p)))
        }),
    ];
    let mut failures = Vec::new();
    for (name, g) in cases {
        let mut p = base.clone();
        store::set_grade(p.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1, &g);
        for scale in [1.0f32, 0.5] {
            let opts = RenderOptions { scale, ..RenderOptions::default() };
            let t = RATE.tick_of(10);
            let plan = filmcraft_render::plan::plan_frame(&p, seq, t, opts, &map);
            assert!(matches!(plan, filmcraft_render::plan::FramePlan::Layers { .. }), "{name}: planned on the CPU");
            c.composite(&plan);
            let (_, _, gpu) = c.read_output().expect("GPU readback");
            let cpu = render_sequence(&p, seq, t, opts, &map).over_black_rgba8();
            let plain = {
                let mut q = base.clone();
                store::set_grade(q.sequence_mut(seq).unwrap().find_item_mut(clip).unwrap().1, &Grade::default());
                render_sequence(&q, seq, t, opts, &map).over_black_rgba8()
            };
            let (mean, p99) = stats(&cpu, &gpu);
            let (changed, _) = stats(&cpu, &plain);
            eprintln!("{name} @ {scale}: GPU vs CPU mean {mean:.3}, p99 {p99} (the grade moves the picture by {changed:.2})");
            if p99 > 6 || mean >= 1.5 {
                failures.push(format!("{name} @ {scale}: mean {mean:.3}, p99 {p99}"));
            }
            if changed < 0.5 {
                failures.push(format!("{name} @ {scale}: the grade changed nothing ({changed:.3})"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// ---- commands ----

fn demo() -> (Session, ClipId) {
    koda_color::register();
    let mut s = Session::default();
    s.execute("file.openDemoProject", json!({})).expect("demo project");
    let seq = s.active_sequence().expect("sequence");
    let clip = seq.video_tracks[0].items.iter().find(|i| s.project.item(i.item).is_some_and(|p| matches!(p.kind, ItemKind::Media(_)))).expect("media clip").id;
    (s, clip)
}

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, p.clone()).unwrap_or_else(|e| panic!("{id} {p}: {e}"))
}

fn grade(s: &mut Session, clip: ClipId) -> Grade {
    serde_json::from_value(run(s, "koda.color.inspect", json!({ "clip": clip }))["grade"].clone()).unwrap()
}

#[test]
fn primaries_and_curve_commands() {
    let (mut s, clip) = demo();
    let n = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "primaries", "name": "global" }))["result"]["node"].clone();
    let prim = |s: &mut Session| -> Primaries {
        match &grade(s, clip).nodes[0].kind {
            NodeKind::Primaries(p) => (**p).clone(),
            k => panic!("{k:?}"),
        }
    };
    assert_eq!(prim(&mut s), Primaries::default());
    for (k, v) in [("temp", json!(-69.9)), ("tint", json!(-5.78)), ("shadows", json!(17.5)), ("gain.r", json!(1.1)), ("lift", json!([0.01, 0.0, 0.0, 0.02]))] {
        run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": n, "param": k, "value": v }));
    }
    let p = prim(&mut s);
    assert_eq!((p.temp, p.tint, p.shadows, p.gain[1], p.lift[3]), (-69.9, -5.78, 17.5, 1.1, 0.02));
    // curves: linked R/G/B, then Y on its own
    run(&mut s, "koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "r", "points": [[0, 0], [0.5, 0.6], [1, 1]] }));
    let p = prim(&mut s);
    assert_eq!(p.curves.r, p.curves.b);
    assert!(p.curves.y.is_identity());
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": n, "param": "curvesLinked", "value": false }));
    run(&mut s, "koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "b", "points": [[0, 0.1], [1, 1]] }));
    let p = prim(&mut s);
    assert_ne!(p.curves.r, p.curves.b);
    // undo goes back one edit at a time
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(prim(&mut s).curves.r, prim(&mut s).curves.b);
    // resets
    run(&mut s, "koda.color.resetParams", json!({ "clip": clip, "node": n, "what": "curves" }));
    assert!(prim(&mut s).curves.is_identity());
    run(&mut s, "koda.color.resetParams", json!({ "clip": clip, "node": n }));
    assert_eq!(prim(&mut s), Primaries { curves: prim(&mut s).curves, ..Default::default() });
    // hostile input
    let cst = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "cst" }))["result"]["node"].clone();
    for (id, p) in [
        ("koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "x", "points": [[0, 0], [1, 1]] })),
        ("koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "y", "points": [[0, "a"]] })),
        ("koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "y", "points": [[0, 0, 0]] })),
        ("koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "y", "points": vec![[0.5, 0.5]; 1000] })),
        ("koda.color.setCurve", json!({ "clip": clip, "node": cst, "channel": "y", "points": [[0, 0], [1, 1]] })),
        ("koda.color.resetParams", json!({ "clip": clip, "node": n, "what": "everything" })),
        ("koda.color.resetParams", json!({ "clip": clip, "node": cst })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "gain", "value": [1, 2] })),
        ("koda.color.setParam", json!({ "clip": clip, "node": n, "param": "offset.y", "value": 1 })),
    ] {
        assert!(s.execute(id, p.clone()).is_err(), "{id} {p}");
    }
    // extreme values are clamped, never NaN
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": n, "param": "contrast", "value": 1e30 }));
    run(&mut s, "koda.color.setCurve", json!({ "clip": clip, "node": n, "channel": "y", "points": [[-5, 1e9], [0.5, -1e9], [9, 0.5]] }));
    let p = prim(&mut s);
    assert_eq!(p.contrast, 2.0);
    assert!(p.curves.y.0.iter().all(|q| q[0].is_finite() && q[1].abs() <= 1.5));
    let r = p.ready();
    assert!(r.apply([0.4, 0.5, 0.6]).iter().all(|v| v.is_finite()));
}

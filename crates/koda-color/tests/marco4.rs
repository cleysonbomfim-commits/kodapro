//! Marco 4 da aba Cor: sharpening and glow. Against DaVinci's nodes 03 and 04 (the owner's local
//! reference set, skipped with a warning when missing), GPU against CPU in the compositor, and
//! the commands.

use std::sync::Arc;

use filmcraft_engine::Session;
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource, SharedSource};
use filmcraft_project::{ClipId, ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_render::{Image, RenderOptions, SourceMap, render_sequence};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use koda_color::eval::{Luts, apply_cpu, to_linear, to_signal};
use koda_color::spatial::{Glow, GlowStyle, Sharpen};
use koda_color::{Grade, NodeKind, deltae, store};
use serde_json::{Value, json};

fn frame(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

fn grade_of(kinds: Vec<NodeKind>) -> Grade {
    let mut g = Grade::default();
    for k in kinds {
        g.add(k, None, None).unwrap();
    }
    g
}

/// A reference frame (signals, 1080 wide) through `g` on the CPU, at full resolution.
fn graded(g: &Grade, v: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut img = Image { w: 1080, h: v.len() / 1080, px: v.iter().flat_map(|c| [to_linear(c[0]), to_linear(c[1]), to_linear(c[2]), 1.0]).collect() };
    apply_cpu(g, &Luts::new(), &mut img, 1.0);
    img.px.chunks(4).map(|p| [to_signal(p[0]), to_signal(p[1]), to_signal(p[2])]).collect()
}

/// Nodes 03 (sharpening) and 04 (glow) at the KODA defaults against DaVinci's frames. The
/// values of DaVinci's nodes are unknown: the defaults were fitted to these frames
/// (`examples/calibra_marco4.rs`), so this guards the fit; the limits are the fit's own numbers
/// with a margin, not the spec's < 1.
#[test]
fn sharpen_and_glow_match_davinci_nodes_03_and_04() {
    let (Some(f2), Some(f3), Some(f4)) = (frame("02_cst.tif"), frame("03_nitidez.tif.tif"), frame("04_glow.tif.tif")) else { return };
    let sharpen = grade_of(vec![NodeKind::Sharpen(Sharpen::default())]);
    let glow = grade_of(vec![NodeKind::Glow(Glow::default())]);
    let both = grade_of(vec![NodeKind::Sharpen(Sharpen::default()), NodeKind::Glow(Glow::default())]);
    let s3 = deltae::compare(&graded(&sharpen, &f2), &f3);
    let s4 = deltae::compare(&graded(&glow, &f3), &f4);
    let s24 = deltae::compare(&graded(&both, &f2), &f4);
    eprintln!("Nitidez(02_cst) × 03_nitidez: {s3}  (sem nitidez: {})", deltae::compare(&f2, &f3));
    eprintln!("Glow(03_nitidez) × 04_glow: {s4}  (sem glow: {})", deltae::compare(&f3, &f4));
    eprintln!("Nitidez + glow(02_cst) × 04_glow: {s24}  (sem os dois: {})", deltae::compare(&f2, &f4));
    assert!(s3.mean < 0.25, "{s3}");
    assert!(s4.mean < 1.2, "{s4}");
    assert!(s24.mean < 1.3, "{s24}");
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

/// Sharpening and every glow style give the CPU reference's picture on the GPU, full and half
/// resolution.
#[test]
fn gpu_sharpen_and_glow_match_cpu_reference() {
    let Some((dev, q)) = device() else {
        eprintln!("SKIPPED (koda sharpen/glow GPU parity): no GPU adapter");
        return;
    };
    let mut c = filmcraft_gpu::GpuCompositor::new(&dev, &q);
    let (base, seq, map, clip) = scene();
    let glow = |style: GlowStyle, rec: f32| NodeKind::Glow(Glow { style, highlight_recovery: rec, spread: 30.0, ..Default::default() });
    let cases: Vec<(&str, Grade)> = vec![
        ("sharpen", grade_of(vec![NodeKind::Sharpen(Sharpen { amount: 1.5, radius: 1.5 })])),
        ("glow soft light", grade_of(vec![glow(GlowStyle::SoftLight, 0.0)])),
        ("glow screen", grade_of(vec![glow(GlowStyle::Screen, 0.0)])),
        ("glow add + recovery", grade_of(vec![glow(GlowStyle::Add, 1.0)])),
        ("glow normal", grade_of(vec![glow(GlowStyle::Normal, 0.0)])),
        ("glow wide (KODA)", grade_of(vec![NodeKind::Glow(Glow::default())])),
        (
            "sharpen + glow + primaries",
            grade_of(vec![
                NodeKind::Primaries(Box::new(koda_color::primaries::koda_global())),
                NodeKind::Sharpen(Sharpen::default()),
                NodeKind::Glow(Glow::default()),
            ]),
        ),
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
            if changed < 0.1 {
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
fn sharpen_and_glow_commands() {
    let (mut s, clip) = demo();
    let sh = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "sharpen" }))["result"]["node"].clone();
    let gl = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "glow" }))["result"]["node"].clone();
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": sh, "param": "amount", "value": 2.0 }));
    for (k, v) in [("style", json!("screen")), ("spread", json!(40)), ("highlightRecovery", json!(0.5)), ("threshold", json!(0.6))] {
        run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": gl, "param": k, "value": v }));
    }
    let g = grade(&mut s, clip);
    assert_eq!(g.nodes[0].name, "Nitidez");
    assert!(matches!(g.nodes[0].kind, NodeKind::Sharpen(Sharpen { amount, .. }) if amount == 2.0));
    let NodeKind::Glow(x) = g.nodes[1].kind else { panic!("{g:?}") };
    assert_eq!((x.style, x.spread, x.highlight_recovery, x.threshold), (GlowStyle::Screen, 40.0, 0.5, 0.6));
    s.execute("edit.undo", json!({})).unwrap();
    let NodeKind::Glow(x) = grade(&mut s, clip).nodes[1].kind else { panic!() };
    assert_eq!(x.threshold, Glow::default().threshold, "undo");
    for (n, k, v) in
        [(&sh, "radius", json!("x")), (&sh, "style", json!("add")), (&gl, "style", json!("burn")), (&gl, "gain", json!(null)), (&gl, "nope", json!(1))]
    {
        assert!(s.execute("koda.color.setParam", json!({ "clip": clip, "node": n, "param": k, "value": v })).is_err(), "{k} {v}");
    }
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": gl, "param": "spread", "value": 1e30 }));
    let NodeKind::Glow(x) = grade(&mut s, clip).nodes[1].kind else { panic!() };
    assert_eq!(x.spread, 1000.0);
}

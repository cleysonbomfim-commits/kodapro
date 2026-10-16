//! Marco 2 da aba Cor: CST and LUT nodes. Against DaVinci's own frames (the owner's local
//! reference set, `docs/koda/cor/referencia/`, skipped with a warning when missing), GPU against
//! CPU in the compositor, and the commands.

use std::sync::Arc;

use filmcraft_color::Lut3d;
use filmcraft_engine::Session;
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource, SharedSource};
use filmcraft_project::{ClipId, ItemKind, Label, MediaClip, MediaRef, Project, ProjectLut, SequenceSettings, TrackKind};
use filmcraft_render::{RenderOptions, SourceMap, render_sequence};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use koda_color::eval::{LoadedLut, Luts, apply_signal};
use koda_color::{Grade, NodeKind, deltae, store};
use serde_json::{Value, json};

/// A reference frame as Rec.709 / Gamma 2.4 (or S-Log3) signals, or `None` when it is missing.
fn frame(name: &str) -> Option<Vec<[f32; 3]>> {
    let p = koda_color::reference_file(name)?;
    let img = image::open(&p).ok()?.into_rgb16();
    Some(img.pixels().map(|p| [p[0] as f32 / 65535.0, p[1] as f32 / 65535.0, p[2] as f32 / 65535.0]).collect())
}

/// 3 × 3 box average of a 1080-wide frame (removes single-pixel export noise, keeps colours).
fn smooth(v: &[[f32; 3]], w: usize) -> Vec<[f32; 3]> {
    let h = v.len() / w;
    let mut o = vec![[0f32; 3]; v.len()];
    for y in 0..h {
        for x in 0..w {
            let (mut s, mut n) = ([0f32; 3], 0.0);
            for yy in y.saturating_sub(1)..(y + 2).min(h) {
                for xx in x.saturating_sub(1)..(x + 2).min(w) {
                    let p = v[yy * w + xx];
                    for i in 0..3 {
                        s[i] += p[i];
                    }
                    n += 1.0;
                }
            }
            o[y * w + x] = s.map(|c| c / n);
        }
    }
    o
}

fn grade_of(kind: NodeKind) -> Grade {
    let mut g = Grade::default();
    g.add(kind, None, None).unwrap();
    g
}

fn graded(g: &Grade, luts: &Luts, src: &[[f32; 3]]) -> Vec<[f32; 3]> {
    use rayon::prelude::*;
    src.par_iter().map(|c| apply_signal(g, luts, *c)).collect()
}

/// Node 02 of the KODA powergrade: CST on DaVinci's node-01 output matches DaVinci's node-02
/// output (ESPEC_COR_V1: mean ΔE 2000 < 1, max < 3; the max on 3 × 3 averages, since single
/// pixels differ by export noise). CST on the raw log frame is reported too: node 01 (the
/// owner's global correction, not available) is missing from it, so it cannot match.
#[test]
fn cst_matches_davinci_node_02() {
    let (Some(n01), Some(n02)) = (frame("01_global.tif"), frame("02_cst.tif")) else { return };
    let g = grade_of(NodeKind::from_type("cst").unwrap());
    let ours = graded(&g, &Luts::new(), &n01);
    let px = deltae::compare(&ours, &n02);
    let sm = deltae::compare(&smooth(&ours, 1080), &smooth(&n02, 1080));
    eprintln!("CST(01_global) vs 02_cst, pixel a pixel: {px}");
    eprintln!("CST(01_global) vs 02_cst, média 3x3:   {sm}");
    if let Some(n00) = frame("00_log.tif") {
        eprintln!("CST(00_log) vs 02_cst (sem o nó 01):   {}", deltae::compare(&graded(&g, &Luts::new(), &n00), &n02));
    }
    assert!(px.mean < 1.0, "{px}");
    assert!(sm.mean < 1.0 && sm.max < 3.0, "{sm}");
}

/// Node 05: the owner's LUT (Orson.cube) on DaVinci's node-04 output matches node 05; on node 02
/// (as asked: nodes 03 and 04 do not exist yet) the difference is reported.
#[test]
fn lut_matches_davinci_node_05() {
    let (Some(n04), Some(n05), Some(path)) = (frame("04_glow.tif.tif"), frame("05_lut.tif.tif"), koda_color::reference_file("Orson.cube")) else { return };
    let cube = Lut3d::parse_cube(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut luts = Luts::new();
    luts.insert("lib:orson".into(), Arc::new(LoadedLut::new(cube).unwrap()));
    let mut g = grade_of(NodeKind::from_type("lut").unwrap());
    let id = g.nodes[0].id;
    g.node_mut(id).unwrap().kind.set_param("lut", &json!("lib:orson")).unwrap();
    let px = deltae::compare(&graded(&g, &luts, &n04), &n05);
    eprintln!("LUT(04_glow) vs 05_lut: {px}");
    if let Some(n02) = frame("02_cst.tif") {
        eprintln!("LUT(02_cst) vs 05_lut (sem os nós 03 e 04): {}", deltae::compare(&graded(&g, &luts, &n02), &n05));
    }
    assert!(px.mean < 1.0 && px.max < 3.0, "{px}");
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
    // a warm, contrasty creative LUT in the project's library
    let cube = Lut3d::from_fn(17, |c| [(c[0] * 1.1 + 0.03).powf(0.9), c[1].powf(1.1), (c[2] * 0.8).powf(1.2)]);
    p.luts.push(ProjectLut { id: "warm".into(), name: "warm".into(), source_path: None, format: "cube".into(), text: Arc::from(cube.to_cube()) });
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

/// CST and LUT nodes (both interpolations, partial intensity, the default CST and one with gamut
/// compression and an sRGB output) give the CPU reference's picture on the GPU.
#[test]
fn gpu_cst_and_lut_match_cpu_reference() {
    let Some((dev, q)) = device() else {
        eprintln!("SKIPPED (koda CST/LUT GPU parity): no GPU adapter");
        return;
    };
    let mut c = filmcraft_gpu::GpuCompositor::new(&dev, &q);
    let (base, seq, map, clip) = scene();
    let lut = |interp: &str, k: f64| {
        let mut n = NodeKind::from_type("lut").unwrap();
        n.set_param("lut", &json!("lib:warm")).unwrap();
        n.set_param("interpolation", &json!(interp)).unwrap();
        n.set_param("intensity", &json!(k)).unwrap();
        n
    };
    let mut cst2 = NodeKind::from_type("cst").unwrap();
    for (k, v) in [("gamutMapping", json!("compress")), ("outputCurve", json!("srgb")), ("contrast", json!(1.4))] {
        cst2.set_param(k, &v).unwrap();
    }
    let mut chain = grade_of(NodeKind::from_type("cst").unwrap());
    chain.add(lut("trilinear", 1.0), None, None).unwrap();
    let cases: Vec<(&str, Grade)> = vec![
        ("cst", grade_of(NodeKind::from_type("cst").unwrap())),
        ("cst compress srgb", grade_of(cst2)),
        ("lut trilinear", grade_of(lut("trilinear", 1.0))),
        ("lut tetrahedral 60%", grade_of(lut("tetrahedral", 0.6))),
        ("cst + lut", chain),
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
            let (mean, p99) = stats(&cpu, &gpu);
            eprintln!("{name} @ {scale}: GPU vs CPU mean {mean:.3}, p99 {p99}");
            if p99 > 6 || mean >= 1.5 {
                failures.push(format!("{name} @ {scale}: mean {mean:.3}, p99 {p99}"));
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
fn cst_and_lut_commands() {
    let (mut s, clip) = demo();
    let cst = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "cst" }))["result"]["node"].clone();
    for (k, v) in [("inputGamut", json!("sGamut3")), ("outputCurve", json!("gamma22")), ("toneMapping", json!("none")), ("gamutMapping", json!("compress"))] {
        run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": cst, "param": k, "value": v }));
    }
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": cst, "param": "contrast", "value": 1.5 }));
    let g = grade(&mut s, clip);
    let NodeKind::Cst(c) = &g.nodes[0].kind else { panic!("{g:?}") };
    assert_eq!(
        (c.input_gamut, c.output_curve, c.gamut_mapping),
        (koda_color::cst::Gamut::SGamut3, koda_color::cst::Curve::Gamma22, koda_color::cst::GamutMapping::Compress)
    );
    assert!(
        matches!(c.tone_mapping, koda_color::cst::ToneMapping::Filmic { contrast, .. } if (contrast - 1.5).abs() < 1e-6),
        "setting a curve parameter turns the filmic curve on"
    );
    // hostile CST parameters
    for (k, v) in [
        ("inputGamut", json!("aces")),
        ("inputCurve", json!(3)),
        ("toneMapping", json!("davinci")),
        ("white", json!("x")),
        ("mid", json!(null)),
        ("nope", json!(1)),
    ] {
        assert!(s.execute("koda.color.setParam", json!({ "clip": clip, "node": cst, "param": k, "value": v })).is_err(), "{k} {v}");
    }
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": cst, "param": "mid", "value": 1e9 }));
    let NodeKind::Cst(c) = &grade(&mut s, clip).nodes[0].kind else { panic!() };
    assert!(matches!(c.tone_mapping, koda_color::cst::ToneMapping::Filmic { mid, .. } if mid <= 10.0), "clamped");

    // a LUT node loaded from a file, through the project's LUT library
    let lut = run(&mut s, "koda.color.addNode", json!({ "clip": clip, "type": "lut" }))["result"]["node"].clone();
    let dir = std::env::temp_dir().join(format!("koda-m2-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("teste.cube");
    std::fs::write(&file, Lut3d::from_fn(9, |c| c.map(|v| v * 0.9)).to_cube()).unwrap();
    let r = run(&mut s, "koda.color.loadLut", json!({ "clip": clip, "node": lut, "path": file.to_string_lossy() }));
    let reference = r["lut"].as_str().unwrap().to_string();
    assert!(reference.starts_with("lib:") && s.project.luts.iter().any(|l| format!("lib:{}", l.id) == reference));
    let NodeKind::Lut { lut: got, intensity, .. } = &grade(&mut s, clip).nodes[1].kind else { panic!() };
    assert_eq!((got.as_str(), *intensity), (reference.as_str(), 1.0));
    run(&mut s, "koda.color.setParam", json!({ "clip": clip, "node": lut, "param": "intensity", "value": 0.4 }));
    // hostile LUT parameters
    for (k, v) in [("intensity", json!("half")), ("interpolation", json!("cubic")), ("lut", json!(7)), ("lut", json!("x".repeat(1000)))] {
        assert!(s.execute("koda.color.setParam", json!({ "clip": clip, "node": lut, "param": k, "value": v })).is_err(), "{k}");
    }
    assert!(s.execute("koda.color.loadLut", json!({ "clip": clip, "node": cst, "path": file.to_string_lossy() })).is_err(), "not a LUT node");
    assert!(s.execute("koda.color.loadLut", json!({ "clip": clip, "node": lut, "path": dir.join("nao-existe.cube").to_string_lossy() })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

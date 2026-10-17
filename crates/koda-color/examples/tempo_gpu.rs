//! Tempo da grade na GPU (compositor do FilmCraft) numa cena 4K vertical sintética.
//!   cargo run --release -p koda-color --example tempo_gpu
use std::sync::Arc;

use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, Generator, MediaSource, SharedSource};
use filmcraft_project::{ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_render::{RenderOptions, SourceMap};
use filmcraft_time::{FrameRate, TICKS_PER_SECOND, Tick, TimeRange};
use koda_color::{Grade, NodeKind, store};

fn main() {
    koda_color::register();
    let rate = FrameRate::FPS_24;
    let mut p = Project::new("t");
    let seq = p.new_sequence("t", SequenceSettings { width: 2160, height: 3840, frame_rate: rate, ..Default::default() }, 1, 0, None);
    let src = GeneratorSource::new(Generator::Demo(DemoScene::Dunes), 2160, 3840, rate, Tick(10 * TICKS_PER_SECOND));
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
    let Some(ti) = p.make_track_item(item, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, rate.tick_of(48)), rate) else { return };
    let clip = ti.id;
    if let Some(s) = p.sequence_mut(seq) {
        s.video_tracks[0].items.push(ti);
    }
    let instance = wgpu::Instance::default();
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
    let Ok((dev, q)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) else { return };
    let mut c = filmcraft_gpu::GpuCompositor::new(&dev, &q);
    let one = |k: &str| -> Grade {
        let mut g = Grade::default();
        if let Some(n) = NodeKind::from_type(k) {
            let _ = g.add(n, None, None);
        }
        g
    };
    for (name, g) in [("nada", Grade::default()), ("primaries", one("primaries")), ("cst", one("cst")), ("sharpen", one("sharpen")), ("glow", one("glow"))] {
        let mut pp = p.clone();
        if let Some((_, it)) = pp.sequence_mut(seq).and_then(|s| s.find_item_mut(clip)) {
            store::set_grade(it, &g);
        }
        let opts = RenderOptions::default();
        let plan = filmcraft_render::plan::plan_frame(&pp, seq, rate.tick_of(10), opts, &map);
        c.composite(&plan);
        let _ = c.read_output();
        let t = std::time::Instant::now();
        for _ in 0..10 {
            c.composite(&plan);
            let _ = c.read_output();
        }
        println!("{name:10} {:.1} ms/quadro (com leitura)", t.elapsed().as_secs_f64() * 100.0);
    }
}

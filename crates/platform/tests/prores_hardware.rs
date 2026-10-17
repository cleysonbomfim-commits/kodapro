//! KODA Pro: VideoToolbox ProRes 422 in the export (Apple silicon with a ProRes engine), the
//! encoder the export dialog names, and the fallback to the software encoder when a hardware
//! encoder fails in the middle of an export. Tests that need a hardware encoder skip without one.
#![cfg(target_os = "macos")]

use std::sync::Arc;

use filmcraft_export::{
    EncodedPacket, EncoderFrame, ExportError, ExportSettings, Format, HardwareEncoding, Progress, SOFTWARE_ENCODER, VideoEncoder, describe_video_encoder,
    export, rgba_to_yuv422_10,
};
use filmcraft_frame::PixelData;
use filmcraft_isobmff::{CodecConfig, SampleEntry};
use filmcraft_media::generators::GeneratorSource;
use filmcraft_media::{DemoScene, MediaSource};
use filmcraft_platform::hardware_encode::{VT_HARDWARE, prores_available, prores_config_for, prores_kind, videotoolbox_prores_factory};
use filmcraft_platform::videotoolbox_encode::ProResKind;
use filmcraft_project::{ItemKind, Label, MediaClip, MediaRef, Project, SequenceSettings, TrackKind};
use filmcraft_render::SourceMap;
use filmcraft_time::{FrameRate, Tick, TimeRange};

const W: u32 = 640;
const H: u32 = 360;
const FPS: FrameRate = FrameRate::FPS_30;

/// A moving test picture: gradients plus a bar travelling across.
fn picture(i: u64) -> Vec<u8> {
    let (w, h) = (W as usize, H as usize);
    let mut px = vec![255u8; w * h * 4];
    let bar = (i as usize * 7) % (w - 40);
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 4;
            let on_bar = x >= bar && x < bar + 40 && y > h / 3 && y < 2 * h / 3;
            px[o] = if on_bar { 255 } else { ((x * 255 / w + i as usize * 3) % 256) as u8 };
            px[o + 1] = if on_bar { 255 } else { (y * 255 / h) as u8 };
            px[o + 2] = if on_bar { 255 } else { (((x + y) / 2 + i as usize * 5) % 256) as u8 };
        }
    }
    px
}

fn prores_settings() -> ExportSettings {
    ExportSettings {
        format: Format::ProRes,
        hardware_encoding: HardwareEncoding::Auto,
        prores_profile: "hq".into(),
        include_audio: false,
        ..Default::default()
    }
}

fn psnr10(a: &[u16], b: &[u16]) -> f64 {
    assert_eq!(a.len(), b.len());
    let mse = a.iter().zip(b).map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2)).sum::<f64>() / a.len() as f64;
    if mse == 0.0 { 99.0 } else { 10.0 * (1023.0f64 * 1023.0 / mse).log10() }
}

#[test]
fn prores_declines_what_the_hardware_path_does_not_take() {
    let ok = prores_settings();
    assert!(prores_config_for(W, H, FPS, &ok).is_ok());
    let declined = |s: &ExportSettings, what: &str| assert!(prores_config_for(W, H, FPS, s).is_err(), "{what}");
    declined(&ExportSettings { hardware_encoding: HardwareEncoding::Off, ..ok.clone() }, "hardware off");
    declined(&ExportSettings { prores_profile: "4444".into(), ..ok.clone() }, "4444");
    declined(&ExportSettings { prores_profile: "4444xq".into(), ..ok.clone() }, "4444 XQ");
    declined(&ExportSettings { signal: filmcraft_export::ColorSignal::PQ, ..ok.clone() }, "HDR");
    declined(&ExportSettings { pixel_aspect: Some((4, 3)), ..ok.clone() }, "non-square pixels");
    assert!(prores_config_for(W + 1, H, FPS, &ok).is_err(), "odd width");
    assert!(prores_config_for(W, H, FrameRate { num: 0, den: 1 }, &ok).is_err());
    assert_eq!(prores_kind("lt"), Some(ProResKind::Lt));
    assert_eq!(prores_kind("proxy").map(ProResKind::fourcc), Some(u32::from_be_bytes(*b"apco")));
    assert_eq!(prores_kind("whatever"), Some(ProResKind::Hq));
    assert!(videotoolbox_prores_factory(Format::H264, W, H, FPS, &ok).is_none(), "only ProRes");
}

/// The hardware ProRes 422 HQ stream decodes (our own ProRes decoder) to 10-bit 4:2:2 pictures that
/// match the built-in encoder's input conversion: the colour and the chroma resolution are kept.
#[test]
fn prores_hardware_round_trip_keeps_10_bit_422() {
    let s = prores_settings();
    let mut enc = match videotoolbox_prores_factory(Format::ProRes, W, H, FPS, &s) {
        Some(Ok(e)) => e,
        Some(Err(e)) => panic!("factory error: {e}"),
        None => {
            eprintln!("SKIPPED: no hardware ProRes encoder on this machine");
            return;
        }
    };
    assert_eq!(enc.name(), VT_HARDWARE);
    assert!(enc.is_hardware());
    let mut packets: Vec<EncodedPacket> = Vec::new();
    let frames = 6u64;
    for i in 0..frames {
        let rgba = picture(i);
        packets.extend(enc.encode(&EncoderFrame { width: W, height: H, rgba: &rgba, hdr: None, index: i }).expect("encode"));
    }
    packets.extend(enc.flush().expect("flush"));
    assert_eq!(packets.len(), frames as usize);
    assert!(packets.iter().all(|p| p.key && p.composition_offset == 0));
    let entry: SampleEntry = enc.sample_entry();
    assert!(matches!(&entry.codec, CodecConfig::ProRes { .. }), "{}", entry.codec.name());
    let mut dec = filmcraft_codecs::software_video_decoder(&entry).expect("ProRes decoder");
    let (w, h, cw) = (W as usize, H as usize, (W as usize).div_ceil(2));
    for (i, p) in packets.iter().enumerate() {
        let out = dec.decode(&p.data, i as i64).expect("decode");
        let f = out.first().expect("a picture");
        let PixelData::Yuv16 { planes, bits, .. } = &f.frame.data else { panic!("expected 10-bit planes") };
        assert_eq!(*bits, 10);
        assert_eq!((planes[0].len(), planes[1].len(), planes[2].len()), (w * h, cw * h, cw * h), "4:2:2: half-width, full-height chroma");
        let (mut y, mut u, mut v) = (vec![0u16; w * h], vec![0u16; cw * h], vec![0u16; cw * h]);
        rgba_to_yuv422_10(&picture(i as u64), w, h, &mut y, &mut u, &mut v);
        let (py, pu, pv) = (psnr10(&planes[0], &y), psnr10(&planes[1], &u), psnr10(&planes[2], &v));
        assert!(py > 45.0 && pu > 45.0 && pv > 45.0, "frame {i}: PSNR Y {py:.1} U {pu:.1} V {pv:.1}");
    }
}

/// Export ▸ "Encoder in use": the hardware encoder where it takes the settings, else FilmCraft's.
#[test]
fn the_dialog_names_the_encoder() {
    filmcraft_platform::register();
    let h264 = ExportSettings { format: Format::H264, hardware_encoding: HardwareEncoding::Auto, ..Default::default() };
    let off = ExportSettings { hardware_encoding: HardwareEncoding::Off, ..h264.clone() };
    assert_eq!(describe_video_encoder(W, H, FPS, &off), SOFTWARE_ENCODER);
    let p4444 = ExportSettings { prores_profile: "4444".into(), ..prores_settings() };
    assert_eq!(describe_video_encoder(W, H, FPS, &p4444), SOFTWARE_ENCODER);
    if prores_available() {
        assert_eq!(describe_video_encoder(W, H, FPS, &prores_settings()), VT_HARDWARE);
        assert_eq!(describe_video_encoder(W, H, FPS, &h264), VT_HARDWARE);
    }
    assert_eq!(describe_video_encoder(W + 1, H, FPS, &h264), SOFTWARE_ENCODER, "odd size: built-in");
}

fn project(frames: i64) -> (Arc<Project>, filmcraft_project::ItemId, SourceMap) {
    let mut p = Project::new("hw");
    let g = GeneratorSource::demo(DemoScene::Plasma);
    let info = g.info().clone();
    let item = p.add_item(
        &info.name.clone(),
        Label::Iris,
        ItemKind::Media(MediaClip {
            media: MediaRef::Generator(g.generator.clone()),
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
    let seq = p.new_sequence("s", SequenceSettings { width: W, height: H, frame_rate: FPS, ..Default::default() }, 1, 1, None);
    let mut clip = p.make_track_item(item, TrackKind::Video, Tick::ZERO, TimeRange::new(Tick::ZERO, FPS.tick_of(frames)), FPS).unwrap();
    clip.scale_to_frame = true;
    p.sequence_mut(seq).unwrap().video_tracks[0].items.push(clip);
    let mut m = SourceMap::default();
    m.0.insert(item, Arc::new(g));
    (Arc::new(p), seq, m)
}

fn tmp(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("fc-hw-encode-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.join(name).to_string_lossy().to_string()
}

/// A full ProRes export takes the hardware encoder and writes a MOV our decoder reads.
#[test]
fn prores_export_through_the_pipeline() {
    filmcraft_platform::register();
    if !prores_available() {
        eprintln!("SKIPPED: no hardware ProRes encoder");
        return;
    }
    let path = tmp("hw.mov");
    let s = ExportSettings { path: path.clone(), ..prores_settings() };
    let (p, seq, m) = project(12);
    let r = export(&p, seq, &s, &m, &Progress::default()).expect("export");
    assert_eq!((r.frames, r.encoder.as_str(), r.fallback.is_none()), (12, VT_HARDWARE, true));
    let bytes = std::fs::read(&path).unwrap();
    let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
    let vt = file.track_of_kind(filmcraft_isobmff::TrackKind::Video).unwrap();
    assert_eq!(file.tracks[vt].samples.len(), 12);
    assert!(file.tracks[vt].samples.iter().all(|s| s.is_sync));
}

/// A "hardware" encoder that fails at its third picture (only for exports named `*fails-mid*`).
struct Failing(u64);

impl VideoEncoder for Failing {
    fn sample_entry(&self) -> SampleEntry {
        SampleEntry::prores(filmcraft_isobmff::FourCc(*b"apch"), W as u16, H as u16)
    }
    fn timescale(&self) -> u32 {
        FPS.num as u32
    }
    fn encode(&mut self, _: &EncoderFrame) -> filmcraft_export::Result<Vec<EncodedPacket>> {
        self.0 += 1;
        if self.0 >= 3 { Err(ExportError::Encode("simulated hardware failure".into())) } else { Ok(Vec::new()) }
    }
    fn flush(&mut self) -> filmcraft_export::Result<Vec<EncodedPacket>> {
        Ok(Vec::new())
    }
    fn is_hardware(&self) -> bool {
        true
    }
    fn name(&self) -> String {
        "simulated hardware".into()
    }
}

fn failing_factory(format: Format, _: u32, _: u32, _: FrameRate, s: &ExportSettings) -> Option<filmcraft_export::Result<Box<dyn VideoEncoder>>> {
    (format == Format::ProRes && s.hardware_encoding == HardwareEncoding::Auto && s.path.contains("fails-mid"))
        .then(|| Ok(Box::new(Failing(0)) as Box<dyn VideoEncoder>))
}

/// A hardware encoder failing mid-export: the export runs again with the software encoder, and the
/// report says so.
#[test]
fn a_failing_hardware_encoder_falls_back_to_software() {
    // the platform's encoders first: registering puts a factory in front, once, so the simulated
    // one stays ahead of VideoToolbox whichever test registers the platform later
    filmcraft_platform::register();
    filmcraft_export::register_encoder(failing_factory);
    let path = tmp("fails-mid.mov");
    let s = ExportSettings { path: path.clone(), ..prores_settings() };
    let (p, seq, m) = project(10);
    let r = export(&p, seq, &s, &m, &Progress::default()).expect("the software encoder finishes the export");
    assert_eq!(r.frames, 10);
    assert_eq!(r.encoder, SOFTWARE_ENCODER);
    assert!(r.fallback.as_deref().is_some_and(|f| f.contains("simulated hardware failure")), "{:?}", r.fallback);
    let bytes = std::fs::read(&path).unwrap();
    let file = filmcraft_isobmff::open(bytes.as_slice()).unwrap();
    let vt = file.track_of_kind(filmcraft_isobmff::TrackKind::Video).unwrap();
    assert_eq!(file.tracks[vt].samples.len(), 10);
}

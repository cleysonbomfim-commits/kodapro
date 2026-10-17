//! KODA Pro aba Cor (`docs/koda/cor/ESPEC_COR_V1.md`): clip grades made of colour nodes.
//!
//! - [`grade`]: the data model (a chain of nodes), stored per clip as a hidden effect ([`store`]).
//! - [`eval`]: the CPU reference and the GPU passes (WGSL), which must agree.
//! - [`commands`]: `koda.color.*`, the one path for the UI, CLI, control channel and MCP.
//!
//! [`register`] plugs it into FilmCraft: the renderer runs a clip's grade before its other effects
//! and its Motion (CPU and GPU compositor), and the engine lists the commands. Apps call it once
//! at startup, before the first session is made.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod commands;
pub mod cst;
pub mod deltae;
pub mod eval;
pub mod grade;
pub mod library;
pub mod primaries;
pub mod spatial;
pub mod store;

use std::sync::Arc;

pub use grade::{Grade, Interpolation, Node, NodeKind};
pub use store::EFFECT;

/// Register the grade effect with the renderer and the `koda.color.*` commands with the engine.
/// Safe to call more than once; call it before the first session is made.
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        filmcraft_render::external::register(filmcraft_render::external::ExternalFx { id: store::EFFECT, prepare });
        filmcraft_engine::commands::register_external(commands::commands);
    });
}

/// The grade's operation at one frame: the CPU reference and the GPU passes, with its LUTs
/// loaded from the project's LUT library. `None` when it changes nothing or does not parse.
fn prepare(e: &filmcraft_project::EffectInstance, cx: &filmcraft_render::effects::FxCtx) -> Option<filmcraft_render::external::ExternalOp> {
    let g = store::grade_of_effect(e)?;
    if g.is_identity() {
        return None;
    }
    let luts = load_luts(&g, cx.project);
    let scale = cx.px_scale;
    let passes = eval::gpu_passes(&g, &luts, scale);
    if passes.is_empty() {
        return None;
    }
    let cpu = filmcraft_render::external::CpuFx(Arc::new(move |img: &mut filmcraft_render::Image| eval::apply_cpu(&g, &luts, img, scale)));
    Some(filmcraft_render::external::ExternalOp { cpu, gpu: Some(filmcraft_render::external::GpuProgram { wgsl: shader(), passes }) })
}

/// The LUTs `g` references, from the project's library (`lib:<id>`). Parsed LUTs are cached by
/// their text, so a frame does not parse a megabyte of `.cube` again.
fn load_luts(g: &Grade, project: Option<&filmcraft_project::Project>) -> eval::Luts {
    let mut out = eval::Luts::new();
    let Some(project) = project else { return out };
    for n in g.active() {
        let NodeKind::Lut { lut, .. } = &n.kind else { continue };
        if out.contains_key(lut) {
            continue;
        }
        let Some(id) = lut.strip_prefix("lib:") else { continue };
        let Some(entry) = project.luts.iter().find(|l| l.id == id) else { continue };
        if let Some(l) = cached_lut(&entry.text, &entry.format) {
            out.insert(lut.clone(), l);
        }
    }
    out
}

/// A LUT parsed from `text` (1D LUTs and shapers are baked into a 33³ cube), cached.
fn cached_lut(text: &str, format: &str) -> Option<Arc<eval::LoadedLut>> {
    use std::hash::{Hash, Hasher};
    static CACHE: std::sync::Mutex<Vec<(u64, usize, Arc<eval::LoadedLut>)>> = std::sync::Mutex::new(Vec::new());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format.hash(&mut h);
    let key = (h.finish(), text.len());
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, _, l)) = cache.iter().find(|(k, n, _)| (*k, *n) == key) {
        return Some(l.clone());
    }
    let fmt = if format == "3dl" { Some(filmcraft_color::LutFormat::ThreeDl) } else { Some(filmcraft_color::LutFormat::Cube) };
    let parsed = filmcraft_color::Lut::parse(text, fmt).ok()?;
    let cube = match (&parsed.shaper, parsed.cube.clone()) {
        (None, Some(c)) => c,
        _ => parsed.baked_cube(33).ok()?,
    };
    let l = Arc::new(eval::LoadedLut::new(cube)?);
    // a handful of LUTs at a time is plenty (a powergrade uses one or two)
    if cache.len() >= 8 {
        cache.remove(0);
    }
    cache.push((key.0, key.1, l.clone()));
    Some(l)
}

/// The grade shader source, shared (the GPU stage caches pipelines by it).
fn shader() -> Arc<str> {
    static S: std::sync::OnceLock<Arc<str>> = std::sync::OnceLock::new();
    S.get_or_init(|| Arc::from(eval::WGSL)).clone()
}

/// A file of the DaVinci reference set (`docs/koda/cor/referencia/`, kept out of git: client
/// footage and a commercial LUT). Tests that compare against it skip, with a warning, when it
/// is not there.
pub fn reference_file(name: &str) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/koda/cor/referencia").join(name);
    if p.is_file() {
        Some(p)
    } else {
        eprintln!("aviso: {} não existe; teste de referência pulado", p.display());
        None
    }
}

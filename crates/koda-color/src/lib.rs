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
pub mod eval;
pub mod grade;
pub mod store;

use std::sync::Arc;

pub use grade::{Grade, Node, NodeKind};
pub use store::EFFECT;

/// Register the grade effect with the renderer and the `koda.color.*` commands with the engine.
/// Safe to call more than once; call it before the first session is made.
pub fn register() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        filmcraft_render::external::register(filmcraft_render::external::ExternalFx { id: store::EFFECT, cpu: cpu_fx, gpu: gpu_fx });
        filmcraft_engine::commands::register_external(commands::commands);
    });
}

/// The grade's CPU reference on a clip's working image.
fn cpu_fx(img: &mut filmcraft_render::Image, e: &filmcraft_project::EffectInstance, _cx: filmcraft_render::external::ExtCtx) {
    if let Some(g) = store::grade_of_effect(e) {
        eval::apply_cpu(&g, &mut img.px);
    }
}

/// The grade's GPU passes (none for a grade that changes nothing or fails to parse).
fn gpu_fx(e: &filmcraft_project::EffectInstance, _cx: filmcraft_render::external::ExtCtx) -> Option<filmcraft_render::external::GpuProgram> {
    let passes = store::grade_of_effect(e).map(|g| eval::gpu_passes(&g)).unwrap_or_default();
    Some(filmcraft_render::external::GpuProgram {
        wgsl: shader(),
        passes: passes.into_iter().map(|p| filmcraft_render::external::GpuPass { i1: p.i1, p: p.p }).collect(),
    })
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

//! Effects provided by other crates (KODA Pro's colour grade, `koda-color`): registered at
//! startup, stored on a clip as a hidden effect instance (no [`EffectDef`], so the Effects and
//! Effect Controls panels, Paste Attributes and effect reordering never see them), and run
//! **first** on the clip: before its standard effects (Lumetri included) and before its Motion,
//! on the CPU and in GPU plans.
//!
//! A registered effect gives a CPU reference and, for the GPU compositor, compute passes in its
//! own WGSL using the effect stage's bindings (`filmcraft-gpu` `fx.wgsl`: uniform `u`, `src`,
//! `dst`; entry point [`ENTRY`]). Both must give the same picture.
//!
//! [`EffectDef`]: filmcraft_project::EffectDef

use std::sync::{Arc, PoisonError, RwLock};

use filmcraft_project::{EffectInstance, TrackItem};

use crate::Image;

/// The compute entry point external WGSL defines (16 × 16 workgroups).
pub const ENTRY: &str = "ext_px";

/// An image a GPU pass reads besides the working image (bound to `aux`, binding 3): RGBA f32,
/// `w × h`, row-major. A 3D LUT, for instance, unfolded into 2D.
#[derive(Debug, PartialEq)]
pub struct AuxImage {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<f32>,
}

/// One GPU pass: integer (`u.i1`) and float (`u.p0`…`u.p5`) parameters, and an optional
/// auxiliary image (shared: the GPU stage uploads each one once).
///
/// Spatial effects (blur-based: sharpening, glow, local contrast) chain passes: `save` keeps the
/// image this pass reads; `blur` passes are the effect stage's own box blur (edge-repeating, the
/// same as [`crate::effects::box_blur`] on the CPU) instead of the shader; a `combine` pass binds
/// the saved image as `aux` (in place of [`GpuPass::aux`]) and forgets it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GpuPass {
    pub i1: [u32; 4],
    pub p: [f32; 24],
    pub aux: Option<Arc<AuxImage>>,
    pub save: bool,
    pub blur: Option<BoxPass>,
    pub combine: bool,
}

/// A box blur pass of `radius` pixels along rows (`vertical == false`) or columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxPass {
    pub radius: u32,
    pub vertical: bool,
}

/// An effect's GPU program: its shader and the passes to run (none: the effect changes nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct GpuProgram {
    pub wgsl: Arc<str>,
    pub passes: Vec<GpuPass>,
}

/// An external effect's CPU reference bound to its parameters (a [`crate::gpufx::FxOp::External`]).
#[derive(Clone)]
pub struct CpuFx(pub Arc<dyn Fn(&mut Image) + Send + Sync>);

impl std::fmt::Debug for CpuFx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CpuFx")
    }
}

impl PartialEq for CpuFx {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.0, &o.0)
    }
}

/// What a registered effect does at one frame: its CPU reference (bound to everything it needs,
/// e.g. LUTs from the project) and, when it has one, its GPU program.
pub struct ExternalOp {
    pub cpu: CpuFx,
    pub gpu: Option<GpuProgram>,
}

/// A registered effect.
#[derive(Clone, Copy)]
pub struct ExternalFx {
    /// The effect id stored on clips.
    pub id: &'static str,
    /// The effect's operation at `cx` (the project is there for data it references), or `None`
    /// when it changes nothing.
    pub prepare: fn(&EffectInstance, &crate::effects::FxCtx) -> Option<ExternalOp>,
}

static REGISTRY: RwLock<Vec<ExternalFx>> = RwLock::new(Vec::new());

/// Register `fx` (replacing an earlier registration of the same id).
pub fn register(fx: ExternalFx) {
    let mut r = REGISTRY.write().unwrap_or_else(PoisonError::into_inner);
    r.retain(|x| x.id != fx.id);
    r.push(fx);
}

/// The registered effect with `id`.
pub fn find(id: &str) -> Option<ExternalFx> {
    REGISTRY.read().unwrap_or_else(PoisonError::into_inner).iter().find(|x| x.id == id).copied()
}

/// The clip's enabled registered effects, in the clip's order: they run before everything else.
pub fn first_effects(item: &TrackItem) -> Vec<&EffectInstance> {
    item.effects.iter().filter(|e| e.enabled && find(&e.effect).is_some()).collect()
}

/// [`TrackItem::has_standard_effects`] counting registered external effects too: a clip whose
/// only effect is its grade must still take the effect path (not the plain-frame shortcut).
pub fn has_effects(item: &TrackItem) -> bool {
    item.has_standard_effects() || item.effects.iter().any(|e| find(&e.effect).is_some())
}

/// The GPU op of a registered effect (its passes and its CPU reference), or `None` when it
/// changes nothing or has no GPU program (the clip is then rendered on the CPU).
pub(crate) fn op(x: ExternalFx, e: &EffectInstance, cx: &crate::effects::FxCtx) -> Option<crate::gpufx::FxOp> {
    let ExternalOp { cpu, gpu } = (x.prepare)(e, cx)?;
    Some(crate::gpufx::FxOp::External { program: gpu?, cpu })
}

/// Apply a registered effect on the CPU.
pub(crate) fn apply_cpu(x: ExternalFx, img: &mut Image, e: &EffectInstance, cx: &crate::effects::FxCtx) {
    if let Some(op) = (x.prepare)(e, cx) {
        (op.cpu.0)(img);
    }
}

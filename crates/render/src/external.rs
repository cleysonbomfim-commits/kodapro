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
use filmcraft_time::Tick;

use crate::Image;

/// The compute entry point external WGSL defines (16 × 16 workgroups).
pub const ENTRY: &str = "ext_px";

/// What an external effect is evaluated at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExtCtx {
    /// Media time (keyframes).
    pub t: Tick,
    /// Working-image pixels per full-resolution clip pixel.
    pub px_scale: f32,
}

/// One GPU pass: integer (`u.i1`) and float (`u.p0`…`u.p5`) parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuPass {
    pub i1: [u32; 4],
    pub p: [f32; 24],
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

/// A registered effect.
#[derive(Clone, Copy)]
pub struct ExternalFx {
    /// The effect id stored on clips.
    pub id: &'static str,
    /// CPU reference: changes the clip's working image (linear premultiplied RGBA f32).
    pub cpu: fn(&mut Image, &EffectInstance, ExtCtx),
    /// GPU program, or `None` to render the clip on the CPU.
    pub gpu: fn(&EffectInstance, ExtCtx) -> Option<GpuProgram>,
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

/// The GPU op of a registered effect (its passes and its CPU reference), or `None` when it has
/// no GPU program (the clip is then rendered on the CPU).
pub(crate) fn op(x: ExternalFx, e: &EffectInstance, cx: ExtCtx) -> Option<crate::gpufx::FxOp> {
    let program = (x.gpu)(e, cx)?;
    let e = e.clone();
    let cpu = CpuFx(Arc::new(move |img: &mut Image| (x.cpu)(img, &e, cx)));
    Some(crate::gpufx::FxOp::External { program, cpu })
}

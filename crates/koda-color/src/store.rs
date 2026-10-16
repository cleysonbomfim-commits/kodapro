//! Where a clip keeps its grade: a hidden effect instance ([`EFFECT`]) whose `grade` parameter is
//! the grade's JSON text. It has no effect definition, so the Effects and Effect Controls panels,
//! Paste Attributes and effect reordering never see it; the renderer runs it before the clip's
//! other effects and its Motion (`filmcraft_render::external`). No project schema change: older
//! FilmCraft builds keep the text and ignore the effect.

use std::collections::BTreeMap;

use filmcraft_project::{EffectInstance, Param, ParamValue, TrackItem};

use crate::grade::Grade;

/// The hidden effect id.
pub const EFFECT: &str = "koda_grade";
/// The parameter holding the JSON text.
pub const PARAM: &str = "grade";

/// The grade stored in `e`, if it is one and it parses.
pub fn grade_of_effect(e: &EffectInstance) -> Option<Grade> {
    if e.effect != EFFECT {
        return None;
    }
    match &e.params.get(PARAM)?.value {
        ParamValue::Text(t) => Grade::from_json(t).ok(),
        _ => None,
    }
}

/// The grade of `item`: its stored one, or an empty grade.
pub fn grade_of(item: &TrackItem) -> Result<Grade, String> {
    match item.effects.iter().find(|e| e.effect == EFFECT) {
        None => Ok(Grade::default()),
        Some(e) => match e.params.get(PARAM).map(|p| &p.value) {
            Some(ParamValue::Text(t)) => Grade::from_json(t),
            _ => Err("the clip's grade is damaged (no grade text)".into()),
        },
    }
}

/// Store `grade` on `item`, replacing an earlier one. An empty grade removes the effect.
pub fn set_grade(item: &mut TrackItem, grade: &Grade) {
    item.effects.retain(|e| e.effect != EFFECT);
    if grade.nodes.is_empty() && !grade.bypass {
        return;
    }
    let mut params = BTreeMap::new();
    params.insert(PARAM.to_string(), Param::new(ParamValue::Text(grade.to_json())));
    item.effects.push(EffectInstance {
        effect: EFFECT.to_string(),
        enabled: true,
        params,
        masks: Vec::new(),
        post_fader: false,
        essential: false,
        layer: None,
    });
}

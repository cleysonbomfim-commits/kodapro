//! The grade of a clip: an ordered chain of colour nodes, evaluated in series (aba Cor v1,
//! `docs/koda/cor/ESPEC_COR_V1.md`). Plain serde data: it is stored in the project file as the
//! JSON text of a hidden clip effect ([`crate::EFFECT`]) and in powergrade files.
//!
//! Everything read from a project, a powergrade or a command parameter is untrusted: [`Grade::sanitized`]
//! bounds the node count, name lengths and every number before a grade is used.

use serde::{Deserialize, Serialize};

/// Format version written into every grade (bumped when the JSON changes incompatibly).
pub const VERSION: u32 = 1;
/// Most nodes a grade may have (a project or command asking for more is refused).
pub const MAX_NODES: usize = 64;
/// Longest node name, in characters.
pub const MAX_NAME: usize = 64;
/// Gain range (per channel and master), as the DaVinci gain controls allow it.
pub const GAIN_RANGE: (f32, f32) = (0.0, 16.0);

/// A clip's grade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Grade {
    pub version: u32,
    /// Bypass the whole grade (before / after).
    pub bypass: bool,
    /// The nodes, in processing order.
    pub nodes: Vec<Node>,
    /// The id the next node gets (ids are stable across reordering, never reused).
    pub next_id: u32,
}

impl Default for Grade {
    fn default() -> Self {
        Self { version: VERSION, bypass: false, nodes: Vec::new(), next_id: 1 }
    }
}

/// One node of the chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub kind: NodeKind,
}

/// What a node does, with its parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    /// Multiplies the signal: `out = in · master · channel` (DaVinci's Gain, without the wheel
    /// UI). Marco 1's test node.
    Gain {
        #[serde(default = "one")]
        master: f32,
        #[serde(default = "ones")]
        rgb: [f32; 3],
    },
    /// Color Space Transform (S-Gamut3.Cine / S-Log3 → Rec.709 / Gamma 2.4 by default).
    Cst(crate::cst::Cst),
    /// A 3D LUT from the project's LUT library (`lib:<id>`), mixed in at `intensity` (0–1).
    Lut {
        #[serde(default)]
        lut: String,
        #[serde(default = "one")]
        intensity: f32,
        #[serde(default)]
        interpolation: Interpolation,
    },
}

/// How a LUT is sampled between its grid points. DaVinci's default (and the KODA powergrade's
/// LUT) matches trilinear best: ΔE 0.091 mean against 0.095 tetrahedral (`docs/koda/DIARIO.md`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Interpolation {
    #[default]
    Trilinear,
    Tetrahedral,
}

/// Longest LUT reference kept.
pub const MAX_LUT_REF: usize = 256;

fn one() -> f32 {
    1.0
}
fn ones() -> [f32; 3] {
    [1.0; 3]
}

impl NodeKind {
    /// The kind for a type name (`"gain"`) with default parameters.
    pub fn from_type(t: &str) -> Option<Self> {
        match t {
            "gain" => Some(NodeKind::Gain { master: 1.0, rgb: [1.0; 3] }),
            "cst" => Some(NodeKind::Cst(crate::cst::Cst::default())),
            "lut" => Some(NodeKind::Lut { lut: String::new(), intensity: 1.0, interpolation: Interpolation::Trilinear }),
            _ => None,
        }
    }

    /// The type name as stored in JSON.
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Gain { .. } => "gain",
            NodeKind::Cst(_) => "cst",
            NodeKind::Lut { .. } => "lut",
        }
    }

    /// The default node name for this kind.
    pub fn default_name(&self) -> &'static str {
        match self {
            NodeKind::Gain { .. } => "Ganho",
            NodeKind::Cst(_) => "CST",
            NodeKind::Lut { .. } => "LUT",
        }
    }

    /// Set one parameter by name. Errors name the parameter and what was wrong; values are
    /// clamped to their range, never stored as NaN or infinity.
    pub fn set_param(&mut self, name: &str, value: &serde_json::Value) -> Result<(), String> {
        match self {
            NodeKind::Gain { master, rgb } => match name {
                "master" => *master = gain(value).ok_or("master: expected a finite number")?,
                "r" | "g" | "b" => {
                    let i = match name {
                        "r" => 0,
                        "g" => 1,
                        _ => 2,
                    };
                    rgb[i] = gain(value).ok_or_else(|| format!("{name}: expected a finite number"))?;
                }
                "rgb" => {
                    let v = value.as_array().filter(|a| a.len() == 3).ok_or("rgb: expected [r, g, b]")?;
                    let mut out = [1.0; 3];
                    for (o, x) in out.iter_mut().zip(v) {
                        *o = gain(x).ok_or("rgb: expected three finite numbers")?;
                    }
                    *rgb = out;
                }
                other => return Err(format!("gain node has no parameter \"{other}\" (master, r, g, b, rgb)")),
            },
            NodeKind::Cst(c) => set_cst(c, name, value)?,
            NodeKind::Lut { lut, intensity, interpolation } => match name {
                "lut" => {
                    let s = value.as_str().ok_or("lut: expected a library reference (\"lib:<id>\") or \"\"")?;
                    if s.chars().count() > MAX_LUT_REF {
                        return Err("lut: reference too long".into());
                    }
                    *lut = s.to_string();
                }
                "intensity" => {
                    *intensity =
                        value.as_f64().filter(|x| x.is_finite()).map(|x| (x as f32).clamp(0.0, 1.0)).ok_or("intensity: expected a number from 0 to 1")?
                }
                "interpolation" => {
                    *interpolation = value
                        .as_str()
                        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
                        .ok_or("interpolation: expected \"trilinear\" or \"tetrahedral\"")?
                }
                other => return Err(format!("lut node has no parameter \"{other}\" (lut, intensity, interpolation)")),
            },
        }
        Ok(())
    }

    /// The parameters clamped to their ranges, NaN replaced by the default.
    fn sanitized(&self) -> Self {
        match self {
            NodeKind::Gain { master, rgb } => NodeKind::Gain { master: clamp_gain(*master), rgb: rgb.map(clamp_gain) },
            NodeKind::Cst(c) => NodeKind::Cst(c.sanitized()),
            NodeKind::Lut { lut, intensity, interpolation } => NodeKind::Lut {
                lut: lut.chars().take(MAX_LUT_REF).collect(),
                intensity: if intensity.is_finite() { intensity.clamp(0.0, 1.0) } else { 1.0 },
                interpolation: *interpolation,
            },
        }
    }
}

/// Set one CST parameter by its JSON name.
fn set_cst(c: &mut crate::cst::Cst, name: &str, v: &serde_json::Value) -> Result<(), String> {
    use crate::cst::{Curve, Gamut, GamutMapping, ToneMapping};
    let text = |what: &str| v.as_str().ok_or_else(|| format!("{name}: expected {what}"));
    let num = || v.as_f64().filter(|x| x.is_finite()).map(|x| x as f32).ok_or_else(|| format!("{name}: expected a finite number"));
    const GAMUTS: &str = "sGamut3Cine, sGamut3, rec709, rec2020 or p3D65";
    const CURVES: &str = "sLog3, gamma24, gamma22, srgb or linear";
    match name {
        "inputGamut" => c.input_gamut = Gamut::from_name(text(GAMUTS)?).ok_or_else(|| format!("{name}: {GAMUTS}"))?,
        "outputGamut" => c.output_gamut = Gamut::from_name(text(GAMUTS)?).ok_or_else(|| format!("{name}: {GAMUTS}"))?,
        "inputCurve" => c.input_curve = Curve::from_name(text(CURVES)?).ok_or_else(|| format!("{name}: {CURVES}"))?,
        "outputCurve" => c.output_curve = Curve::from_name(text(CURVES)?).ok_or_else(|| format!("{name}: {CURVES}"))?,
        "toneMapping" => {
            c.tone_mapping = match text("\"none\" or \"filmic\"")? {
                "none" => ToneMapping::None,
                "filmic" => {
                    let (white, mid, contrast) = crate::cst::FILMIC_DEFAULT;
                    ToneMapping::Filmic { white, mid, contrast }
                }
                _ => return Err(format!("{name}: \"none\" or \"filmic\"")),
            }
        }
        "gamutMapping" => {
            c.gamut_mapping = match text("\"none\" or \"compress\"")? {
                "none" => GamutMapping::None,
                "compress" => GamutMapping::Compress,
                _ => return Err(format!("{name}: \"none\" or \"compress\"")),
            }
        }
        "white" | "mid" | "contrast" => {
            let x = num()?;
            let (mut w, mut m, mut k) = match c.tone_mapping {
                ToneMapping::Filmic { white, mid, contrast } => (white, mid, contrast),
                ToneMapping::None => crate::cst::FILMIC_DEFAULT,
            };
            match name {
                "white" => w = x,
                "mid" => m = x,
                _ => k = x,
            }
            c.tone_mapping = ToneMapping::Filmic { white: w, mid: m, contrast: k };
        }
        other => {
            return Err(format!(
                "cst node has no parameter \"{other}\" (inputGamut, inputCurve, outputGamut, outputCurve, toneMapping, white, mid, contrast, gamutMapping)"
            ));
        }
    }
    *c = c.sanitized();
    Ok(())
}

fn gain(v: &serde_json::Value) -> Option<f32> {
    v.as_f64().filter(|x| x.is_finite()).map(|x| clamp_gain(x as f32))
}

fn clamp_gain(x: f32) -> f32 {
    if x.is_finite() { x.clamp(GAIN_RANGE.0, GAIN_RANGE.1) } else { 1.0 }
}

impl Grade {
    /// Parse a grade from its JSON text, sanitized. Errors on JSON that is not a grade.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let g: Grade = serde_json::from_str(text).map_err(|e| format!("not a KODA grade: {e}"))?;
        if g.version > VERSION {
            return Err(format!("grade version {} is newer than this KODA Pro reads ({VERSION})", g.version));
        }
        Ok(g.sanitized())
    }

    /// The JSON text stored in the project.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// A copy that is safe to evaluate: at most [`MAX_NODES`] nodes, unique ids, names cut to
    /// [`MAX_NAME`] characters, every parameter in range.
    pub fn sanitized(&self) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        let mut next = self.next_id.max(1);
        let nodes: Vec<Node> = self
            .nodes
            .iter()
            .take(MAX_NODES)
            .map(|n| {
                let mut id = n.id;
                if id == 0 || !seen.insert(id) {
                    id = next;
                    seen.insert(id);
                }
                next = next.max(id.saturating_add(1));
                Node { id, name: n.name.chars().take(MAX_NAME).collect(), enabled: n.enabled, kind: n.kind.sanitized() }
            })
            .collect();
        Grade { version: VERSION, bypass: self.bypass, nodes, next_id: next }
    }

    /// Index of the node with `id`.
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Add a node at `index` (end when `None` or past the end); returns its id.
    pub fn add(&mut self, kind: NodeKind, name: Option<&str>, index: Option<usize>) -> Result<u32, String> {
        if self.nodes.len() >= MAX_NODES {
            return Err(format!("a grade has at most {MAX_NODES} nodes"));
        }
        let id = self.next_id.max(1);
        self.next_id = id.checked_add(1).ok_or("node ids exhausted")?;
        let name = name.map(|s| s.chars().take(MAX_NAME).collect()).unwrap_or_else(|| kind.default_name().to_string());
        let at = index.unwrap_or(self.nodes.len()).min(self.nodes.len());
        self.nodes.insert(at, Node { id, name, enabled: true, kind });
        Ok(id)
    }

    /// Remove the node `id`.
    pub fn remove(&mut self, id: u32) -> Result<Node, String> {
        let i = self.index_of(id).ok_or_else(|| format!("no node {id} in this grade"))?;
        Ok(self.nodes.remove(i))
    }

    /// Move the node `id` to position `to` (clamped to the chain).
    pub fn move_to(&mut self, id: u32, to: usize) -> Result<(), String> {
        let i = self.index_of(id).ok_or_else(|| format!("no node {id} in this grade"))?;
        let n = self.nodes.remove(i);
        let to = to.min(self.nodes.len());
        self.nodes.insert(to, n);
        Ok(())
    }

    /// The node `id`, for changes.
    pub fn node_mut(&mut self, id: u32) -> Result<&mut Node, String> {
        self.nodes.iter_mut().find(|n| n.id == id).ok_or_else(|| format!("no node {id} in this grade"))
    }

    /// The nodes that change the picture: none when bypassed, else the enabled ones in order.
    pub fn active(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(move |n| !self.bypass && n.enabled)
    }

    /// Whether evaluating the grade changes nothing (bypassed, no enabled node, or only identity nodes).
    pub fn is_identity(&self) -> bool {
        self.active().all(|n| match &n.kind {
            NodeKind::Gain { master, rgb } => *master == 1.0 && *rgb == [1.0; 3],
            NodeKind::Cst(_) => false,
            NodeKind::Lut { lut, intensity, .. } => lut.is_empty() || *intensity <= 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip_keeps_everything() {
        let mut g = Grade::default();
        let a = g.add(NodeKind::Gain { master: 1.5, rgb: [1.0, 0.5, 2.0] }, Some("global"), None).unwrap();
        let b = g.add(NodeKind::from_type("gain").unwrap(), None, None).unwrap();
        g.node_mut(b).unwrap().enabled = false;
        g.move_to(b, 0).unwrap();
        g.bypass = true;
        let back = Grade::from_json(&g.to_json()).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.nodes.iter().map(|n| n.id).collect::<Vec<_>>(), [b, a]);
        assert_eq!(back.nodes[1].name, "global");
        assert_eq!(back.nodes[0].name, "Ganho");
    }

    #[test]
    fn hostile_grades_are_sanitized_not_trusted() {
        // NaN / huge gains, duplicate and zero ids, an endless name, too many nodes
        let mut nodes = String::new();
        for i in 0..100 {
            if i > 0 {
                nodes.push(',');
            }
            nodes.push_str(&format!(r#"{{"id":{},"name":"{}","enabled":true,"type":"gain","master":1e30,"rgb":[-5,0.5,2]}}"#, i % 3, "n".repeat(500)));
        }
        let g = Grade::from_json(&format!(r#"{{"version":1,"nodes":[{nodes}],"nextId":0}}"#)).unwrap();
        assert_eq!(g.nodes.len(), MAX_NODES);
        let ids: std::collections::BTreeSet<u32> = g.nodes.iter().map(|n| n.id).collect();
        assert_eq!(ids.len(), MAX_NODES, "ids unique");
        assert!(!ids.contains(&0));
        assert!(g.next_id > *ids.iter().max().unwrap());
        assert!(g.nodes.iter().all(|n| n.name.chars().count() == MAX_NAME));
        assert_eq!(g.nodes[0].kind, NodeKind::Gain { master: 16.0, rgb: [0.0, 0.5, 2.0] });
        // not a grade, a newer version
        assert!(Grade::from_json("[1,2]").is_err());
        assert!(Grade::from_json(r#"{"version":99}"#).is_err());
        // unknown node type
        assert!(Grade::from_json(r#"{"nodes":[{"id":1,"name":"x","enabled":true,"type":"teleport"}]}"#).is_err());
    }

    #[test]
    fn params_are_checked_and_clamped() {
        let mut k = NodeKind::from_type("gain").unwrap();
        assert!(k.set_param("master", &serde_json::json!("big")).is_err());
        assert!(k.set_param("master", &serde_json::json!(null)).is_err());
        assert!(k.set_param("rgb", &serde_json::json!([1, 2])).is_err());
        assert!(k.set_param("exposure", &serde_json::json!(1)).is_err());
        k.set_param("master", &serde_json::json!(99)).unwrap();
        k.set_param("g", &serde_json::json!(-1)).unwrap();
        assert_eq!(k, NodeKind::Gain { master: 16.0, rgb: [1.0, 0.0, 1.0] });
    }

    #[test]
    fn chain_edits() {
        let mut g = Grade::default();
        let ids: Vec<u32> = (0..3).map(|_| g.add(NodeKind::from_type("gain").unwrap(), None, None).unwrap()).collect();
        g.move_to(ids[2], 0).unwrap();
        assert_eq!(g.nodes.iter().map(|n| n.id).collect::<Vec<_>>(), [ids[2], ids[0], ids[1]]);
        g.move_to(ids[2], 99).unwrap();
        assert_eq!(g.nodes.last().unwrap().id, ids[2]);
        g.remove(ids[0]).unwrap();
        assert!(g.remove(ids[0]).is_err());
        assert!(g.node_mut(12345).is_err());
        // ids are never reused
        let new = g.add(NodeKind::from_type("gain").unwrap(), None, Some(0)).unwrap();
        assert!(!ids.contains(&new));
        assert_eq!(g.nodes[0].id, new);
        assert!(g.is_identity());
        g.node_mut(new).unwrap().kind.set_param("master", &serde_json::json!(2)).unwrap();
        assert!(!g.is_identity());
        g.bypass = true;
        assert!(g.is_identity() && g.active().count() == 0);
    }
}

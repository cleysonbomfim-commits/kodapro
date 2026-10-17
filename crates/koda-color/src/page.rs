//! The página Cor's own state (marco 7), saved beside the project (FilmCraft's `ProjectView`
//! extensions, key [`KEY`]), never an undo step: the selected node of each clip and the open
//! palette. (Bars or wheels is a parameter of the primaries node, so it is in the grade.)
//!
//! Also the interpretation check: a clip interpreted as a camera log space is converted by
//! FilmCraft before the grade, so a CST node in its grade converts a second time ([`double_conversion`]).

use std::collections::BTreeMap;

use filmcraft_engine::Session;
use serde::{Deserialize, Serialize};

use crate::grade::NodeKind;

/// The key under `ProjectView::extensions`.
pub const KEY: &str = "koda.color";
/// Most clips whose selected node is remembered.
pub const MAX_CLIPS: usize = 10_000;

/// Palettes of the page.
pub const PALETTES: [&str; 2] = ["primaries", "curves"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PageState {
    /// "primaries" or "curves".
    pub palette: String,
    /// Selected node by clip id.
    pub nodes: BTreeMap<u64, u32>,
}

impl PageState {
    pub fn sanitized(mut self) -> Self {
        if !PALETTES.contains(&self.palette.as_str()) {
            self.palette = PALETTES[0].to_string();
        }
        if self.nodes.len() > MAX_CLIPS {
            self.nodes = self.nodes.into_iter().take(MAX_CLIPS).collect();
        }
        self
    }
}

/// The page state of the open project (defaults when it has none or it does not parse).
pub fn get(s: &Session) -> PageState {
    s.state.view_extensions.get(KEY).and_then(|v| serde_json::from_value::<PageState>(v.clone()).ok()).unwrap_or_default().sanitized()
}

/// Store the page state (not an edit: saved with the project's view).
pub fn set(s: &mut Session, st: &PageState) {
    let v = serde_json::to_value(st.clone().sanitized()).unwrap_or_default();
    if s.state.view_extensions.get(KEY) != Some(&v) {
        s.state.view_extensions.insert(KEY.to_string(), v);
    }
}

/// When `clip` (a timeline clip of the open sequence) is interpreted in a camera colour space
/// (Interpret Footage) and its grade has an enabled CST node: that space's name. FilmCraft then
/// converts the clip before the grade and the CST converts it again.
pub fn double_conversion(s: &Session, clip: filmcraft_project::ClipId) -> Option<String> {
    let seq = s.active_sequence()?;
    let (_, item) = seq.find_item(clip)?;
    let filmcraft_project::ItemKind::Media(m) = &s.project.item(item.item)?.kind else { return None };
    let space = m.interpret.color_space?;
    if matches!(space, filmcraft_color::ColorSpace::Rec709 | filmcraft_color::ColorSpace::Srgb) {
        return None;
    }
    let g = crate::store::grade_of(item).ok()?;
    g.active().any(|n| matches!(n.kind, NodeKind::Cst(_))).then(|| space.label().to_string())
}

//! KODA Pro página Cor (aba Cor v1, marco 5): the layout of `docs/koda/cor/LAYOUT_PAGINA_COR.md`,
//! measured on the owner's DaVinci screenshot, with KODA's own icons and the green accent.
//!
//! Top to bottom on the left (71 % of the width): the panel toggles bar, the panels' toolbar, the
//! LUT browser (folders and thumbnails; the Gallery of powergrades in its place), the node editor
//! and the effects library; then the clip strip, the palette bar, the primaries (or curves)
//! palette and the scopes. The viewer takes the full height on the right (vertical video).
//!
//! Everything changes the grade through the `koda.color.*` commands (one undo step each; a drag is
//! one step), so the page, the CLI, the control channel and MCP share one path. The page's own
//! state (selected node, palette, shown panels) lives in egui memory ([`CorState`]).
//!
//! Automation ids: `cor.top.<toggle>`, `cor.lut.<id>`, `cor.lut.import`, `cor.gallery.<n>`,
//! `cor.gallery.save`, `cor.node.<id>`, `cor.nodes.add`, `cor.fx.<type>`, `cor.clip.<n>`,
//! `cor.palette.<name>`, `cor.scope.<kind>`, `cor.viewer.bypass`, and the palettes' controls
//! (`cor.prim.*`, `cor.curve.*`, see [`super::cor_paletas`]).

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};
use filmcraft_project::{ClipId, ItemId, ItemKind};
use filmcraft_time::Tick;
use koda_color::{Grade, NodeKind};
use serde_json::{Value, json};

use crate::FilmcraftApp;
use crate::theme::Tokens;

/// KODA green: the accent of the aba Cor (selected node, current clip, active controls).
pub const KODA_GREEN: Color32 = Color32::from_rgb(0x2E, 0xD5, 0x73);

/// The page's colours (`LAYOUT_PAGINA_COR.md`, "Estilo visual").
pub mod c {
    use egui::Color32;
    pub const BAR: Color32 = Color32::from_rgb(0x17, 0x18, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x28, 0x28, 0x2e);
    pub const VIEWER: Color32 = Color32::from_rgb(0x19, 0x19, 0x19);
    pub const BLACK: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0c);
    pub const FIELD: Color32 = Color32::from_rgb(0x1d, 0x1d, 0x21);
    pub const SEP: Color32 = Color32::from_rgb(0x0f, 0x0f, 0x11);
    pub const TEXT: Color32 = Color32::from_rgb(0xf1, 0xf1, 0xf1);
    pub const DIM: Color32 = Color32::from_rgb(0xa6, 0xa6, 0xac);
    pub const FAINT: Color32 = Color32::from_rgb(0x5e, 0x5e, 0x66);
    pub const RED: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x5a);
    pub const GREEN: Color32 = Color32::from_rgb(0x4c, 0xc7, 0x6b);
    pub const BLUE: Color32 = Color32::from_rgb(0x5b, 0x8d, 0xef);
}

/// Which palette fills the bottom-left area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    Primaries,
    Curves,
}

/// The page's own state (egui memory, per window).
#[derive(Clone, Debug)]
pub struct CorState {
    /// The selected node of the current clip's grade.
    pub node: Option<u32>,
    /// The clip `node` belongs to (a new clip resets the selection).
    pub clip: Option<ClipId>,
    pub palette: Palette,
    pub show_luts: bool,
    pub show_gallery: bool,
    pub show_clips: bool,
    pub show_nodes: bool,
    pub show_effects: bool,
    pub show_scopes: bool,
    /// The curve being edited (0 = Y, 1–3 = R, G, B).
    pub curve: usize,
    /// The scope shown.
    pub scope: filmcraft_scopes::ScopeKind,
    /// Effects library categories open.
    pub fx_open: [bool; 4],
    /// A numeric field being typed into: (field id, text).
    pub edit: Option<(String, String)>,
    /// Counter that names a drag's undo step (one step per drag).
    pub drag_seq: u64,
    /// Powergrades listed in the Gallery (refreshed when it opens or after a save).
    pub gallery: Vec<(String, String)>,
}

impl Default for CorState {
    fn default() -> Self {
        CorState {
            node: None,
            clip: None,
            palette: Palette::Primaries,
            show_luts: true,
            show_gallery: false,
            show_clips: true,
            show_nodes: true,
            show_effects: true,
            show_scopes: true,
            curve: 0,
            scope: filmcraft_scopes::ScopeKind::Parade,
            fx_open: [true; 4],
            edit: None,
            drag_seq: 0,
            gallery: Vec::new(),
        }
    }
}

fn state_id() -> Id {
    Id::new("koda-cor-state")
}

pub fn state(ctx: &egui::Context) -> CorState {
    ctx.data(|d| d.get_temp::<CorState>(state_id())).unwrap_or_default()
}

pub fn set_state(ctx: &egui::Context, st: CorState) {
    ctx.data_mut(|d| d.insert_temp(state_id(), st));
}

/// Run a command (UI or engine); an error goes to the status bar.
pub fn run(app: &mut FilmcraftApp, ctx: &egui::Context, id: &str, p: Value) -> Option<Value> {
    match crate::menus::invoke(app, ctx, id, p) {
        Ok(v) => Some(v),
        Err(e) => {
            app.status(format!("{id}: {e}"));
            None
        }
    }
}

/// A media clip of the open sequence, as the clip strip shows it.
#[derive(Clone, Debug)]
pub struct ClipInfo {
    pub id: ClipId,
    pub item: ItemId,
    pub start: Tick,
    pub end: Tick,
    pub source_in: Tick,
    pub track: String,
    pub codec: String,
}

/// "H.264 4:2:2 10-bit"-style label of a video stream.
fn codec_label(codec: &str, pix: &str) -> String {
    let c = codec.to_ascii_lowercase();
    let name = if c.contains("264") || c == "avc1" || c == "avc" {
        "H.264".to_string()
    } else if c.contains("265") || c.contains("hevc") || c == "hvc1" {
        "H.265".to_string()
    } else if c.contains("prores") || c.starts_with("ap") {
        "ProRes".to_string()
    } else {
        codec.to_uppercase()
    };
    let chroma = if pix.contains("444") {
        " 4:4:4"
    } else if pix.contains("422") {
        " 4:2:2"
    } else if pix.contains("420") {
        " 4:2:0"
    } else {
        ""
    };
    let bits = if pix.contains("12") {
        " 12-bit"
    } else if pix.contains("10") {
        " 10-bit"
    } else {
        ""
    };
    format!("{name}{chroma}{bits}")
}

/// The media clips of the open sequence in timeline order (start, then track).
pub fn media_clips(app: &FilmcraftApp) -> Vec<ClipInfo> {
    let Some(seq) = app.session.active_sequence() else { return Vec::new() };
    let mut out = Vec::new();
    for (ti, t) in seq.video_tracks.iter().enumerate() {
        for it in &t.items {
            let Some(pi) = app.session.project.item(it.item) else { continue };
            let info = match &pi.kind {
                ItemKind::Media(m) => Some(&m.info),
                ItemKind::Subclip { .. } => None,
                _ => continue,
            };
            let codec = info.and_then(|i| i.video.as_ref()).map(|v| codec_label(&v.codec, &v.pixel_format)).unwrap_or_default();
            out.push(ClipInfo {
                id: it.id,
                item: it.item,
                start: it.start,
                end: Tick(it.start.0.saturating_add(it.duration.0)),
                source_in: it.source_in,
                track: format!("V{}", ti + 1),
                codec,
            });
        }
    }
    out.sort_by_key(|c| (c.start, c.track.clone()));
    out
}

/// The clip the page grades: the one selected clip, else the topmost media clip under the
/// playhead, else the first.
pub fn current_clip(app: &FilmcraftApp, clips: &[ClipInfo]) -> Option<ClipId> {
    if let [one] = app.session.state.selection.as_slice()
        && clips.iter().any(|c| c.id == *one)
    {
        return Some(*one);
    }
    let ph = app.session.playhead();
    clips
        .iter()
        .filter(|c| c.start <= ph && ph < c.end)
        .max_by_key(|c| c.track.trim_start_matches('V').parse::<u32>().unwrap_or(0))
        .or(clips.first())
        .map(|c| c.id)
}

/// The grade of `clip` (empty when it has none or it does not parse).
pub fn grade_of(app: &FilmcraftApp, clip: ClipId) -> Grade {
    app.session.active_sequence().and_then(|s| s.find_item(clip)).and_then(|(_, it)| koda_color::store::grade_of(it).ok()).unwrap_or_default()
}

/// The open sequence's name.
fn sequence_name(app: &FilmcraftApp) -> String {
    app.session.state.active_sequence.and_then(|id| app.session.project.item(id)).map(|i| i.name.clone()).unwrap_or_default()
}

/// Regions of the page, from the percentages measured on the reference (body = window 0–97 %).
struct Layout {
    top: Rect,
    tools: Rect,
    lut_tree: Rect,
    lut_grid: Rect,
    nodes: Rect,
    effects: Rect,
    viewer: Rect,
    clips: Rect,
    palettes: Rect,
    palette: Rect,
    scopes: Rect,
}

fn layout(r: Rect, st: &CorState) -> Layout {
    let x = |p: f32| r.min.x + r.width() * p / 100.0;
    let y = |p: f32| r.min.y + r.height() * p / 97.0;
    let left = x(71.0);
    let top_end = if st.show_clips { y(51.0) } else { y(63.5) };
    let luts_end = if st.show_luts || st.show_gallery { x(26.5) } else { r.min.x };
    let fx_start = if st.show_effects { x(49.5) } else { left };
    let (nodes_l, nodes_r) = if st.show_nodes { (luts_end, fx_start) } else { (luts_end, luts_end) };
    let fx_l = if st.show_nodes { fx_start } else { luts_end };
    Layout {
        top: Rect::from_min_max(r.min, pos2(r.max.x, y(3.0))),
        tools: Rect::from_min_max(pos2(r.min.x, y(3.0)), pos2(left, y(5.5))),
        lut_tree: Rect::from_min_max(pos2(r.min.x, y(5.5)), pos2(x(7.5).min(luts_end), top_end)),
        lut_grid: Rect::from_min_max(pos2(x(7.5).min(luts_end), y(5.5)), pos2(luts_end, top_end)),
        nodes: Rect::from_min_max(pos2(nodes_l, y(5.5)), pos2(nodes_r, top_end)),
        effects: Rect::from_min_max(pos2(fx_l, y(5.5)), pos2(left, top_end)),
        viewer: Rect::from_min_max(pos2(x(71.3), y(3.0)), r.max),
        clips: Rect::from_min_max(pos2(r.min.x, y(52.0)), pos2(left, y(63.5))),
        palettes: Rect::from_min_max(pos2(r.min.x, y(64.5)), pos2(left, y(67.5))),
        palette: Rect::from_min_max(pos2(r.min.x, y(68.0)), pos2(if st.show_scopes { x(35.5) } else { left }, r.max.y)),
        scopes: Rect::from_min_max(pos2(x(35.5), y(68.0)), pos2(left, r.max.y)),
    }
}

/// Small text helpers.
pub fn text(ui: &egui::Ui, p: Pos2, a: Align2, s: impl ToString, size: f32, col: Color32) {
    ui.painter().text(p, a, s.to_string(), Tokens::ui(size), col);
}

pub fn panel_bg(ui: &egui::Ui, r: Rect, col: Color32) {
    if r.width() > 0.0 && r.height() > 0.0 {
        ui.painter().rect_filled(r, 0.0, col);
        ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, c::SEP), StrokeKind::Inside);
    }
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    koda_color::register();
    let ctx = ui.ctx().clone();
    let mut st = state(&ctx);
    ui.painter().rect_filled(rect, 0.0, c::SEP);
    let clips = media_clips(app);
    let clip = current_clip(app, &clips);
    if st.clip != clip {
        st.clip = clip;
        st.node = None;
    }
    let grade = clip.map(|id| grade_of(app, id)).unwrap_or_default();
    // the selection follows the grade: a removed node, or none yet → the first primaries node
    if st.node.is_none_or(|n| grade.index_of(n).is_none()) {
        st.node = grade.nodes.iter().find(|n| matches!(n.kind, NodeKind::Primaries(_))).or(grade.nodes.first()).map(|n| n.id);
    }
    let l = layout(rect, &st);
    top_bar(app, ui, l.top, &mut st);
    toolbar(app, ui, &l, &st, &grade);
    if st.show_gallery {
        gallery(app, ui, Rect::from_min_max(l.lut_tree.min, l.lut_grid.max), &mut st, clip);
    } else if st.show_luts {
        luts(app, ui, l.lut_tree, l.lut_grid, &mut st, clip, &grade);
    }
    if st.show_nodes {
        nodes(app, ui, l.nodes, &mut st, clip, &grade, &clips);
    }
    if st.show_effects {
        effects(app, ui, l.effects, &mut st, clip, &grade);
    }
    if st.show_clips {
        clip_strip(app, ui, l.clips, clip, &clips);
    }
    palettes_bar(app, ui, l.palettes, &mut st);
    match st.palette {
        Palette::Primaries => super::cor_paletas::primaries(app, ui, l.palette, &mut st, clip, &grade),
        Palette::Curves => super::cor_paletas::curves(app, ui, l.palette, &mut st, clip, &grade),
    }
    if st.show_scopes {
        scopes(app, ui, l.scopes, &mut st);
    }
    viewer(app, ui, l.viewer, clip, &grade);
    set_state(&ctx, st);
}

// ---------------------------------------------------------------------------- icons (KODA's own)

/// The page's thin-line glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Gallery,
    Luts,
    Media,
    Clips,
    Timeline,
    Nodes,
    Effects,
    Lightbox,
    Export,
    Primaries,
    Hdr,
    Mixer,
    Curves,
    Qualifier,
    Windows,
    Tracker,
    Blur,
    Key,
    Sizing,
    Keyframes,
    Scopes,
    Info,
    Cst,
    Lut,
    Sharpen,
    Glow,
    Gain,
}

pub fn glyph(p: &egui::Painter, r: Rect, g: Glyph, col: Color32) {
    let s = Stroke::new(1.2, col);
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let line = |a: (f32, f32), b: (f32, f32)| p.line_segment([at(a.0, a.1), at(b.0, b.1)], s);
    match g {
        Glyph::Gallery => {
            p.rect_stroke(rect(0.1, 0.25, 0.7, 0.85), 1.0, s, StrokeKind::Middle);
            line((0.3, 0.15), (0.9, 0.15));
            line((0.9, 0.15), (0.9, 0.65));
        }
        Glyph::Luts | Glyph::Lut => {
            p.rect_stroke(rect(0.15, 0.15, 0.85, 0.85), 1.0, s, StrokeKind::Middle);
            line((0.5, 0.15), (0.5, 0.85));
            line((0.15, 0.5), (0.85, 0.5));
        }
        Glyph::Media => {
            p.rect_stroke(rect(0.1, 0.3, 0.9, 0.85), 1.0, s, StrokeKind::Middle);
            line((0.1, 0.3), (0.3, 0.15));
            line((0.3, 0.15), (0.5, 0.3));
        }
        Glyph::Clips | Glyph::Lightbox => {
            for (x0, x1) in [(0.08, 0.32), (0.38, 0.62), (0.68, 0.92)] {
                p.rect_stroke(rect(x0, 0.3, x1, 0.7), 1.0, s, StrokeKind::Middle);
            }
        }
        Glyph::Timeline => {
            line((0.1, 0.35), (0.6, 0.35));
            line((0.3, 0.65), (0.9, 0.65));
            line((0.45, 0.15), (0.45, 0.85));
        }
        Glyph::Nodes => {
            p.rect_stroke(rect(0.05, 0.2, 0.4, 0.5), 1.0, s, StrokeKind::Middle);
            p.rect_stroke(rect(0.6, 0.5, 0.95, 0.8), 1.0, s, StrokeKind::Middle);
            line((0.4, 0.35), (0.6, 0.65));
        }
        Glyph::Effects => {
            p.add(Shape::closed_line(
                vec![
                    at(0.5, 0.1),
                    at(0.62, 0.4),
                    at(0.92, 0.45),
                    at(0.68, 0.65),
                    at(0.75, 0.92),
                    at(0.5, 0.76),
                    at(0.25, 0.92),
                    at(0.32, 0.65),
                    at(0.08, 0.45),
                    at(0.38, 0.4),
                ],
                s,
            ));
        }
        Glyph::Export => {
            line((0.5, 0.1), (0.5, 0.6));
            line((0.3, 0.3), (0.5, 0.1));
            line((0.7, 0.3), (0.5, 0.1));
            p.add(Shape::line(vec![at(0.15, 0.5), at(0.15, 0.88), at(0.85, 0.88), at(0.85, 0.5)], s));
        }
        Glyph::Primaries => {
            for (cx, cy) in [(0.3, 0.38), (0.7, 0.38), (0.5, 0.72)] {
                p.circle_stroke(at(cx, cy), r.width() * 0.2, s);
            }
        }
        Glyph::Hdr => {
            p.circle_stroke(r.center(), r.width() * 0.38, s);
            p.circle_stroke(r.center(), r.width() * 0.18, s);
        }
        Glyph::Mixer => {
            for x in [0.25, 0.5, 0.75] {
                line((x, 0.12), (x, 0.88));
            }
            for (x, y) in [(0.25, 0.65), (0.5, 0.35), (0.75, 0.55)] {
                p.rect_filled(Rect::from_center_size(at(x, y), vec2(r.width() * 0.16, r.height() * 0.1)), 1.0, col);
            }
        }
        Glyph::Curves => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    at(0.1 + 0.8 * t, 0.9 - 0.8 * (t * t * (3.0 - 2.0 * t)))
                })
                .collect();
            p.add(Shape::line(pts, s));
            p.rect_stroke(rect(0.1, 0.1, 0.9, 0.9), 1.0, Stroke::new(0.8, col.gamma_multiply(0.5)), StrokeKind::Middle);
        }
        Glyph::Qualifier => {
            line((0.75, 0.15), (0.35, 0.55));
            p.add(Shape::closed_line(vec![at(0.35, 0.55), at(0.2, 0.85), at(0.45, 0.7)], s));
        }
        Glyph::Windows => {
            p.circle_stroke(at(0.38, 0.5), r.width() * 0.25, s);
            p.rect_stroke(rect(0.5, 0.25, 0.9, 0.75), 1.0, s, StrokeKind::Middle);
        }
        Glyph::Tracker => {
            p.circle_stroke(r.center(), r.width() * 0.3, s);
            line((0.5, 0.05), (0.5, 0.95));
            line((0.05, 0.5), (0.95, 0.5));
        }
        Glyph::Blur => {
            for (i, rad) in [0.4f32, 0.28, 0.16].iter().enumerate() {
                p.circle_stroke(r.center(), r.width() * rad, Stroke::new(1.0, col.gamma_multiply(0.4 + 0.3 * i as f32)));
            }
        }
        Glyph::Key => {
            p.circle_stroke(at(0.3, 0.5), r.width() * 0.18, s);
            line((0.48, 0.5), (0.9, 0.5));
            line((0.78, 0.5), (0.78, 0.68));
        }
        Glyph::Sizing => {
            p.rect_stroke(rect(0.15, 0.15, 0.85, 0.85), 1.0, s, StrokeKind::Middle);
            line((0.3, 0.7), (0.7, 0.3));
            line((0.5, 0.3), (0.7, 0.3));
            line((0.7, 0.3), (0.7, 0.5));
        }
        Glyph::Keyframes => {
            let cc = r.center();
            let d = r.width() * 0.3;
            p.add(Shape::closed_line(vec![cc + vec2(0.0, -d), cc + vec2(d, 0.0), cc + vec2(0.0, d), cc + vec2(-d, 0.0)], s));
        }
        Glyph::Scopes => {
            p.rect_stroke(rect(0.1, 0.15, 0.9, 0.85), 1.0, s, StrokeKind::Middle);
            p.add(Shape::line(vec![at(0.15, 0.7), at(0.35, 0.4), at(0.55, 0.6), at(0.85, 0.3)], s));
        }
        Glyph::Info => {
            p.circle_stroke(r.center(), r.width() * 0.38, s);
            line((0.5, 0.45), (0.5, 0.72));
            p.circle_filled(at(0.5, 0.32), 1.2, col);
        }
        Glyph::Cst => {
            p.circle_stroke(r.center(), r.width() * 0.36, s);
            line((0.3, 0.5), (0.7, 0.5));
            line((0.58, 0.38), (0.7, 0.5));
            line((0.58, 0.62), (0.7, 0.5));
        }
        Glyph::Sharpen => {
            p.add(Shape::closed_line(vec![at(0.5, 0.15), at(0.85, 0.82), at(0.15, 0.82)], s));
        }
        Glyph::Glow => {
            p.circle_stroke(r.center(), r.width() * 0.2, s);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let (dx, dy) = (a.cos(), a.sin());
                p.line_segment([r.center() + vec2(dx, dy) * r.width() * 0.3, r.center() + vec2(dx, dy) * r.width() * 0.45], s);
            }
        }
        Glyph::Gain => {
            line((0.2, 0.8), (0.8, 0.2));
            line((0.55, 0.2), (0.8, 0.2));
            line((0.8, 0.2), (0.8, 0.45));
        }
    }
}

/// The glyph of a node's kind.
pub fn node_glyph(k: &NodeKind) -> Glyph {
    match k {
        NodeKind::Gain { .. } => Glyph::Gain,
        NodeKind::Cst(_) => Glyph::Cst,
        NodeKind::Lut { .. } => Glyph::Lut,
        NodeKind::Primaries(_) => Glyph::Mixer,
        NodeKind::Sharpen(_) => Glyph::Sharpen,
        NodeKind::Glow(_) => Glyph::Glow,
    }
}

// ------------------------------------------------------------------------------ top bar, toolbar

/// A toggle button (glyph + label); returns true when clicked.
/// Width of a top-bar toggle for `label`.
fn toggle_w(ui: &egui::Ui, label: &str) -> f32 {
    let w = ui.painter().layout_no_wrap(label.to_string(), Tokens::ui(11.5), c::TEXT).size().x;
    w + 23.0 + 18.0
}

fn toggle(app: &mut FilmcraftApp, ui: &egui::Ui, r: Rect, id: &str, g: Glyph, label: &str, on: bool, enabled: bool) -> bool {
    let resp = ui.interact(r, Id::new(("cor-top", id)), if enabled { Sense::click() } else { Sense::hover() });
    app.auto.add(&format!("cor.top.{id}"), r, label);
    let col = if !enabled {
        c::FAINT
    } else if on || resp.hovered() {
        c::TEXT
    } else {
        c::DIM
    };
    glyph(ui.painter(), Rect::from_center_size(pos2(r.min.x + 11.0, r.center().y), vec2(14.0, 14.0)), g, if on && enabled { KODA_GREEN } else { col });
    text(ui, pos2(r.min.x + 23.0, r.center().y), Align2::LEFT_CENTER, label, 11.5, col);
    enabled && resp.clicked()
}

fn top_bar(app: &mut FilmcraftApp, ui: &egui::Ui, r: Rect, st: &mut CorState) {
    panel_bg(ui, r, c::BAR);
    let name = sequence_name(app);
    text(ui, r.center(), Align2::CENTER_CENTER, &name, 12.5, c::TEXT);
    let mut x = r.min.x + 8.0;
    let left: [(&str, Glyph, &str, bool, bool); 4] = [
        ("gallery", Glyph::Gallery, tl!("Gallery"), st.show_gallery, true),
        ("luts", Glyph::Luts, tl!("LUTs"), st.show_luts && !st.show_gallery, true),
        ("media", Glyph::Media, tl!("Media"), false, false),
        ("clips", Glyph::Clips, tl!("Clips"), st.show_clips, true),
    ];
    for (id, g, label, on, en) in left {
        let w = toggle_w(ui, label);
        let b = Rect::from_min_size(pos2(x, r.min.y), vec2(w, r.height()));
        x += w;
        if toggle(app, ui, b, id, g, label, on, en) {
            match id {
                "gallery" => {
                    st.show_gallery = !st.show_gallery;
                    if st.show_gallery {
                        st.gallery = list_gallery(app, ui.ctx());
                    }
                }
                "luts" => {
                    st.show_luts = !st.show_luts || st.show_gallery;
                    st.show_gallery = false;
                }
                _ => st.show_clips = !st.show_clips,
            }
        }
    }
    let right: [(&str, Glyph, &str, bool, bool); 5] = [
        ("export", Glyph::Export, tl!("Quick Export"), false, false),
        ("timeline", Glyph::Timeline, tl!("Timeline"), false, false),
        ("nodes", Glyph::Nodes, tl!("Nodes"), st.show_nodes, true),
        ("effects", Glyph::Effects, tl!("Effects"), st.show_effects, true),
        ("lightbox", Glyph::Lightbox, tl!("Lightbox"), false, false),
    ];
    let mut x = r.max.x - 8.0 - right.iter().map(|b| toggle_w(ui, b.2)).sum::<f32>();
    for (id, g, label, on, en) in right {
        let w = toggle_w(ui, label);
        let b = Rect::from_min_size(pos2(x, r.min.y), vec2(w, r.height()));
        x += w;
        if toggle(app, ui, b, id, g, label, on, en) {
            match id {
                "nodes" => st.show_nodes = !st.show_nodes,
                _ => st.show_effects = !st.show_effects,
            }
        }
    }
}

fn toolbar(_app: &mut FilmcraftApp, ui: &egui::Ui, l: &Layout, st: &CorState, grade: &Grade) {
    panel_bg(ui, l.tools, c::BAR);
    let y = l.tools.center().y;
    if st.show_gallery {
        text(ui, pos2(l.tools.min.x + 10.0, y), Align2::LEFT_CENTER, tl!("Powergrades"), 11.0, c::DIM);
    } else if st.show_luts {
        text(ui, pos2(l.tools.min.x + 10.0, y), Align2::LEFT_CENTER, tl!("LUTs"), 11.0, c::DIM);
    }
    if st.show_nodes && l.nodes.width() > 60.0 {
        text(ui, pos2(l.nodes.center().x, y), Align2::CENTER_CENTER, tl!("Clip"), 11.0, c::TEXT);
        text(ui, pos2(l.nodes.max.x - 10.0, y), Align2::RIGHT_CENTER, format!("{} {}", grade.nodes.len(), tl!("nodes")), 10.5, c::DIM);
    }
    if st.show_effects && l.effects.width() > 60.0 {
        text(ui, pos2(l.effects.min.x + 10.0, y), Align2::LEFT_CENTER, tl!("Library"), 11.0, c::TEXT);
        ui.painter()
            .line_segment([pos2(l.effects.min.x + 10.0, l.tools.max.y - 2.0), pos2(l.effects.min.x + 60.0, l.tools.max.y - 2.0)], Stroke::new(1.5, KODA_GREEN));
        text(ui, pos2(l.effects.min.x + 80.0, y), Align2::LEFT_CENTER, tl!("Settings"), 11.0, c::FAINT);
    }
}

// ------------------------------------------------------------------------------- LUTs, Gallery

/// A LUT's preview: a hue sweep (x) by brightness (y) through it, cached per LUT text.
fn lut_preview(ctx: &egui::Context, key: &str, text: &str, format: &str) -> Option<egui::TextureHandle> {
    let id = Id::new(("koda-lut-preview", key, text.len()));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let fmt = if format == "3dl" { filmcraft_color::LutFormat::ThreeDl } else { filmcraft_color::LutFormat::Cube };
    let lut = filmcraft_color::Lut::parse(text, Some(fmt)).ok()?;
    let cube = match (&lut.shaper, lut.cube) {
        (None, Some(c)) => c,
        _ => filmcraft_color::Lut::parse(text, Some(fmt)).ok()?.baked_cube(17).ok()?,
    };
    let (w, h) = (64usize, 36usize);
    let mut px = Vec::with_capacity(w * h * 4);
    for yy in 0..h {
        let v = 0.95 - 0.85 * yy as f32 / (h - 1) as f32;
        for xx in 0..w {
            let hue = xx as f32 / w as f32 * 6.0;
            let f = hue.fract();
            let rgb = match hue as u32 {
                0 => [1.0, f, 0.0],
                1 => [1.0 - f, 1.0, 0.0],
                2 => [0.0, 1.0, f],
                3 => [0.0, 1.0 - f, 1.0],
                4 => [f, 0.0, 1.0],
                _ => [1.0, 0.0, 1.0 - f],
            };
            // a gently saturated swatch around the brightness of the row
            let src = rgb.map(|c: f32| v * (0.55 + 0.45 * c));
            let o = cube.apply_trilinear(src);
            px.extend(o.iter().map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8));
            px.push(255);
        }
    }
    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
    let tex = ctx.load_texture(format!("koda-lut-{key}"), img, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

fn luts(app: &mut FilmcraftApp, ui: &mut egui::Ui, tree: Rect, grid: Rect, st: &mut CorState, clip: Option<ClipId>, grade: &Grade) {
    panel_bg(ui, tree, c::PANEL);
    panel_bg(ui, grid, c::PANEL);
    let ctx = ui.ctx().clone();
    let n = app.session.project.luts.len();
    // the folder tree: the project's LUT library, and importing
    let row = |i: usize| Rect::from_min_size(pos2(tree.min.x + 4.0, tree.min.y + 6.0 + i as f32 * 24.0), vec2(tree.width() - 8.0, 22.0));
    let r0 = row(0);
    ui.painter().rect_filled(r0, 3.0, c::FIELD);
    glyph(ui.painter(), Rect::from_center_size(pos2(r0.min.x + 10.0, r0.center().y), vec2(10.0, 10.0)), Glyph::Media, c::DIM);
    text(ui, pos2(r0.min.x + 20.0, r0.center().y), Align2::LEFT_CENTER, tl!("Project"), 11.0, c::TEXT);
    text(ui, pos2(r0.max.x - 4.0, r0.center().y), Align2::RIGHT_CENTER, n, 10.0, c::DIM);
    let r1 = row(1);
    let resp = ui.interact(r1, Id::new("cor-lut-import"), Sense::click());
    app.auto.add("cor.lut.import", r1, tl!("Import LUT…"));
    text(ui, pos2(r1.min.x + 6.0, r1.center().y), Align2::LEFT_CENTER, tl!("Import LUT…"), 11.0, if resp.hovered() { c::TEXT } else { c::DIM });
    if resp.clicked() {
        app.pick_ui(crate::native_dialogs::FileDialog::open_file("LUT", &["cube", "3dl"]), |app, paths| {
            if let Some(path) = paths.first()
                && let Err(e) = app.session.execute("lut.import", json!({ "path": path }))
            {
                app.status(format!("lut.import: {e}"));
            }
        });
    }
    // the thumbnails of the folder
    let entries: Vec<(String, String, String, String)> =
        app.session.project.luts.iter().map(|l| (l.id.clone(), l.name.clone(), l.text.to_string(), l.format.clone())).collect();
    if entries.is_empty() {
        text(ui, grid.center(), Align2::CENTER_CENTER, tl!("No LUTs in the project"), 11.0, c::FAINT);
        return;
    }
    let tw = ((grid.width() - 16.0) / 2.0).clamp(60.0, 140.0);
    let th = tw * 9.0 / 16.0;
    let cols = ((grid.width() - 8.0) / (tw + 8.0)).floor().max(1.0) as usize;
    let selected_lut = st.node.and_then(|n| grade.nodes.iter().find(|x| x.id == n)).and_then(|n| match &n.kind {
        NodeKind::Lut { lut, .. } => Some(lut.clone()),
        _ => None,
    });
    for (i, (id, name, txt, fmt)) in entries.iter().enumerate() {
        let (cx, cy) = (i % cols, i / cols);
        let tile = Rect::from_min_size(pos2(grid.min.x + 8.0 + cx as f32 * (tw + 8.0), grid.min.y + 8.0 + cy as f32 * (th + 24.0)), vec2(tw, th));
        if tile.max.y > grid.max.y {
            break;
        }
        let reference = format!("lib:{id}");
        let resp = ui.interact(tile.expand(2.0), Id::new(("cor-lut", id.as_str())), Sense::click());
        app.auto.add(&format!("cor.lut.{id}"), tile, name);
        match lut_preview(&ctx, id, txt, fmt) {
            Some(tex) => {
                ui.painter().image(tex.id(), tile, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(tile, 2.0, c::FIELD);
            }
        }
        let on = selected_lut.as_deref() == Some(reference.as_str());
        ui.painter().rect_stroke(
            tile,
            2.0,
            Stroke::new(
                if on || resp.hovered() { 1.5 } else { 1.0 },
                if on {
                    KODA_GREEN
                } else if resp.hovered() {
                    c::DIM
                } else {
                    c::SEP
                },
            ),
            StrokeKind::Outside,
        );
        let label: String = name.chars().take(22).collect();
        text(ui, pos2(tile.center().x, tile.max.y + 9.0), Align2::CENTER_CENTER, label, 10.0, c::DIM);
        if resp.clicked()
            && let Some(clip) = clip
        {
            apply_lut(app, &ctx, st, clip, grade, &reference);
        }
    }
}

/// Put a library LUT on the selected LUT node, or add a LUT node after the selected node.
fn apply_lut(app: &mut FilmcraftApp, ctx: &egui::Context, st: &mut CorState, clip: ClipId, grade: &Grade, reference: &str) {
    let sel = st.node.and_then(|n| grade.nodes.iter().find(|x| x.id == n));
    let node = match sel {
        Some(n) if matches!(n.kind, NodeKind::Lut { .. }) => n.id,
        _ => {
            let index = st.node.and_then(|n| grade.index_of(n)).map_or(grade.nodes.len(), |i| i + 1);
            let Some(v) = run(app, ctx, "koda.color.addNode", json!({ "clip": clip, "type": "lut", "index": index })) else { return };
            let Some(id) = v.pointer("/result/node").and_then(Value::as_u64) else { return };
            id as u32
        }
    };
    run(app, ctx, "koda.color.setParam", json!({ "clip": clip, "node": node, "param": "lut", "value": reference }));
    st.node = Some(node);
}

fn list_gallery(app: &mut FilmcraftApp, ctx: &egui::Context) -> Vec<(String, String)> {
    run(app, ctx, "koda.color.listPowergrades", json!({}))
        .and_then(|v| v.get("powergrades").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|p| Some((p.get("name")?.as_str()?.to_string(), p.get("path")?.as_str()?.to_string())))
        .collect()
}

fn gallery(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, clip: Option<ClipId>) {
    panel_bg(ui, r, c::PANEL);
    let ctx = ui.ctx().clone();
    let tw = 112.0;
    let th = 40.0;
    let cols = ((r.width() - 8.0) / (tw + 8.0)).floor().max(1.0) as usize;
    let mut tiles: Vec<(String, Option<String>)> = vec![(format!("+ {}", tl!("Save grade")), None)];
    tiles.extend(st.gallery.iter().map(|(n, p)| (n.clone(), Some(p.clone()))));
    for (i, (name, path)) in tiles.iter().enumerate() {
        let tile = Rect::from_min_size(pos2(r.min.x + 8.0 + (i % cols) as f32 * (tw + 8.0), r.min.y + 8.0 + (i / cols) as f32 * (th + 8.0)), vec2(tw, th));
        if tile.max.y > r.max.y {
            break;
        }
        let id = if path.is_some() { format!("cor.gallery.{}", i - 1) } else { "cor.gallery.save".to_string() };
        let resp = ui.interact(tile, Id::new(("cor-gallery", i)), Sense::click());
        app.auto.add(&id, tile, name);
        ui.painter().rect_filled(tile, 3.0, if resp.hovered() { c::FIELD.gamma_multiply(1.4) } else { c::FIELD });
        glyph(
            ui.painter(),
            Rect::from_center_size(pos2(tile.min.x + 14.0, tile.center().y), vec2(14.0, 14.0)),
            Glyph::Gallery,
            if path.is_some() { c::DIM } else { KODA_GREEN },
        );
        let label: String = name.chars().take(16).collect();
        text(ui, pos2(tile.min.x + 28.0, tile.center().y), Align2::LEFT_CENTER, label, 10.5, c::TEXT);
        if !resp.clicked() {
            continue;
        }
        let Some(clip) = clip else { continue };
        match path {
            None => {
                let name = format!("Powergrade {}", st.gallery.len() + 1);
                if run(app, &ctx, "koda.color.savePowergrade", json!({ "clip": clip, "name": name })).is_some() {
                    st.gallery = list_gallery(app, &ctx);
                }
            }
            Some(p) => {
                run(app, &ctx, "koda.color.applyPowergrade", json!({ "path": p, "clips": [clip] }));
                st.node = None;
            }
        }
    }
    if st.gallery.is_empty() {
        text(ui, r.center() + vec2(0.0, 20.0), Align2::CENTER_CENTER, tl!("No powergrades saved yet"), 11.0, c::FAINT);
    }
}

// ------------------------------------------------------------------------------- node editor

const NODE_TYPES: [(&str, &str); 5] =
    [("primaries", "Primaries and curves"), ("cst", "Color Space Transform"), ("lut", "LUT"), ("sharpen", "Sharpen"), ("glow", "Glow")];

/// Add a node of `ty` after the selected node (or at the end) and select it.
pub fn add_node(app: &mut FilmcraftApp, ctx: &egui::Context, st: &mut CorState, clip: ClipId, grade: &Grade, ty: &str) {
    let index = st.node.and_then(|n| grade.index_of(n)).map_or(grade.nodes.len(), |i| i + 1);
    if let Some(v) = run(app, ctx, "koda.color.addNode", json!({ "clip": clip, "type": ty, "index": index })) {
        st.node = v.pointer("/result/node").and_then(Value::as_u64).map(|n| n as u32);
    }
}

fn nodes(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, clip: Option<ClipId>, grade: &Grade, clips: &[ClipInfo]) {
    panel_bg(ui, r, c::PANEL);
    let ctx = ui.ctx().clone();
    let Some(clip) = clip else {
        text(ui, r.center(), Align2::CENTER_CENTER, tl!("No clip to grade"), 11.0, c::FAINT);
        return;
    };
    // background: right-click adds a node
    let bg = ui.interact(r, Id::new("cor-nodes-bg"), Sense::click());
    app.auto.add("cor.nodes.add", r, tl!("Add node"));
    bg.context_menu(|ui| {
        ui.label(tl!("Add serial node"));
        for (ty, label) in NODE_TYPES {
            if ui.button(fx_label(label)).clicked() {
                add_node(app, &ctx, st, clip, grade, ty);
                ui.close();
            }
        }
        if crate::koda::cor::paste_available() && ui.button(tl!("Paste node")).clicked() {
            run(app, &ctx, "koda.color.pasteNode", json!({ "clip": clip }));
            ui.close();
        }
    });
    // input and output of the grade
    let inp = pos2(r.min.x + 12.0, r.min.y + r.height() * 0.42);
    let out = pos2(r.max.x - 12.0, r.min.y + r.height() * 0.42);
    let p = ui.painter();
    for q in [inp, out] {
        p.circle_filled(q, 5.0, c::GREEN.gamma_multiply(0.85));
        p.circle_stroke(q, 5.0, Stroke::new(1.0, c::SEP));
    }
    let n = grade.nodes.len();
    if n == 0 {
        p.line_segment([inp, out], Stroke::new(1.0, c::FAINT));
        text(ui, r.center() + vec2(0.0, 30.0), Align2::CENTER_CENTER, tl!("Right-click or pick an effect to add a node"), 11.0, c::FAINT);
        return;
    }
    // serial chain laid out as a staircase, like the reference
    let (nw, nh) = (62.0f32, 46.0f32);
    let span_x = (r.width() - 70.0 - nw).max(0.0);
    let span_y = (r.height() - 50.0 - nh).max(0.0);
    let dx = if n > 1 { (span_x / (n - 1) as f32).min(92.0) } else { 0.0 };
    let dy = if n > 1 { (span_y / (n - 1) as f32).min(54.0) } else { 0.0 };
    let total_w = dx * (n.saturating_sub(1)) as f32 + nw;
    let x0 = r.center().x - total_w / 2.0;
    let y0 = r.min.y + 24.0;
    let thumb = clips.iter().find(|c| c.id == clip).map(|c| (c.item, c.source_in));
    let mut prev_out = inp;
    let mut rects = Vec::with_capacity(n);
    for i in 0..n {
        let nr = Rect::from_min_size(pos2(x0 + i as f32 * dx, y0 + i as f32 * dy), vec2(nw, nh));
        rects.push(nr);
        // link from the previous output into this node's input (elbow)
        let a = prev_out;
        let b = pos2(nr.min.x - 4.0, nr.min.y + 18.0);
        let mid = (a.x + b.x) / 2.0;
        ui.painter().add(Shape::line(vec![a, pos2(mid, a.y), pos2(mid, b.y), b], Stroke::new(1.0, c::DIM)));
        prev_out = pos2(nr.max.x + 4.0, nr.min.y + 18.0);
    }
    let last = prev_out;
    let mid = (last.x + out.x) / 2.0;
    ui.painter().add(Shape::line(vec![last, pos2(mid, last.y), pos2(mid, out.y), out], Stroke::new(1.0, c::DIM)));
    for (i, node) in grade.nodes.iter().enumerate() {
        let nr = rects[i];
        let selected = st.node == Some(node.id);
        let resp = ui.interact(nr, Id::new(("cor-node", node.id)), Sense::click());
        app.auto.add(&format!("cor.node.{}", node.id), nr, &node.name);
        let p = ui.painter();
        // label above
        let name_col = if node.enabled { if selected { c::TEXT } else { c::DIM } } else { c::FAINT };
        let label: String = node.name.chars().take(14).collect();
        text(ui, pos2(nr.min.x + 2.0, nr.min.y - 8.0), Align2::LEFT_CENTER, label, 10.5, name_col);
        p.rect_filled(nr, 3.0, c::FIELD);
        // thumbnail of the clip in the top part
        let th = Rect::from_min_max(nr.min + vec2(4.0, 4.0), pos2(nr.max.x - 4.0, nr.max.y - 15.0));
        p.rect_filled(th, 1.0, Color32::BLACK);
        if let Some((item, at)) = thumb
            && let Some((tex, sz)) = app.thumbnail(&ctx, item, at, 96)
        {
            let fit = crate::panels::monitor::fit(th, sz.x, sz.y);
            let tint = if node.enabled { Color32::WHITE } else { Color32::from_gray(90) };
            ui.painter().image(tex, fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
        }
        let p = ui.painter();
        // number and the tool glyph along the bottom
        let num_col = if node.enabled { c::TEXT } else { c::FAINT };
        text(ui, pos2(nr.min.x + 6.0, nr.max.y - 7.5), Align2::LEFT_CENTER, format!("{:02}", i + 1), 10.0, num_col);
        glyph(p, Rect::from_center_size(pos2(nr.max.x - 12.0, nr.max.y - 7.5), vec2(10.0, 10.0)), node_glyph(&node.kind), num_col);
        if let NodeKind::Primaries(pr) = &node.kind
            && !pr.curves.is_identity()
        {
            glyph(p, Rect::from_center_size(pos2(nr.max.x - 25.0, nr.max.y - 7.5), vec2(10.0, 10.0)), Glyph::Curves, num_col);
        }
        // in / out connectors
        p.rect_filled(Rect::from_center_size(pos2(nr.min.x - 2.0, nr.min.y + 18.0), vec2(5.0, 5.0)), 0.0, c::GREEN);
        p.rect_filled(Rect::from_center_size(pos2(nr.max.x + 2.0, nr.min.y + 18.0), vec2(5.0, 5.0)), 0.0, c::GREEN);
        p.rect_filled(Rect::from_center_size(pos2(nr.min.x - 2.0, nr.min.y + 30.0), vec2(5.0, 5.0)), 0.0, c::BLUE);
        p.rect_filled(Rect::from_center_size(pos2(nr.max.x + 2.0, nr.min.y + 30.0), vec2(5.0, 5.0)), 0.0, c::BLUE);
        let border = if selected {
            Stroke::new(1.8, KODA_GREEN)
        } else if resp.hovered() {
            Stroke::new(1.0, c::DIM)
        } else {
            Stroke::new(1.0, c::SEP)
        };
        p.rect_stroke(nr, 3.0, border, StrokeKind::Outside);
        if !node.enabled {
            ui.painter().add(Shape::dashed_line(
                &[nr.left_top(), nr.right_top(), nr.right_bottom(), nr.left_bottom(), nr.left_top()],
                Stroke::new(1.0, c::FAINT),
                3.0,
                3.0,
            ));
        }
        if resp.clicked() {
            st.node = Some(node.id);
        }
        let id = node.id;
        let enabled = node.enabled;
        resp.context_menu(|ui| {
            st.node = Some(id);
            if ui.button(if enabled { tl!("Disable node") } else { tl!("Enable node") }).clicked() {
                run(app, &ctx, "koda.color.setNodeEnabled", json!({ "clip": clip, "node": id, "enabled": !enabled }));
                ui.close();
            }
            ui.menu_button(tl!("Add serial node"), |ui| {
                for (ty, label) in NODE_TYPES {
                    if ui.button(fx_label(label)).clicked() {
                        add_node(app, &ctx, st, clip, grade, ty);
                        ui.close();
                    }
                }
            });
            if i > 0 && ui.button(tl!("Move left")).clicked() {
                run(app, &ctx, "koda.color.moveNode", json!({ "clip": clip, "node": id, "index": i - 1 }));
                ui.close();
            }
            if i + 1 < n && ui.button(tl!("Move right")).clicked() {
                run(app, &ctx, "koda.color.moveNode", json!({ "clip": clip, "node": id, "index": i + 1 }));
                ui.close();
            }
            if ui.button(tl!("Copy node")).clicked() {
                run(app, &ctx, "koda.color.copyNode", json!({ "clip": clip, "node": id }));
                ui.close();
            }
            if paste_available() && ui.button(tl!("Paste node settings")).clicked() {
                run(app, &ctx, "koda.color.pasteNode", json!({ "clip": clip, "replace": id }));
                ui.close();
            }
            if ui.button(tl!("Delete node")).clicked() {
                run(app, &ctx, "koda.color.removeNode", json!({ "clip": clip, "node": id }));
                st.node = None;
                ui.close();
            }
        });
    }
}

/// Whether a node is on the grade clipboard.
pub fn paste_available() -> bool {
    matches!(koda_color::library::clipboard(), Some(koda_color::library::Copied::Node(..)))
}

// ------------------------------------------------------------------------------ effects library

const FX: [(&str, &[(&str, &str, Glyph)]); 4] = [
    ("Color correction", &[("primaries", "Primaries and curves", Glyph::Primaries)]),
    ("Color transform", &[("cst", "Color Space Transform", Glyph::Cst), ("lut", "LUT", Glyph::Lut)]),
    ("Sharpen and blur", &[("sharpen", "Sharpen", Glyph::Sharpen)]),
    ("Light effects", &[("glow", "Glow", Glyph::Glow)]),
];

/// A node type's name as the page shows it: DaVinci's names stay (Glow, LUT), the rest translated.
fn fx_label(label: &str) -> &str {
    match label {
        "Glow" | "LUT" => label,
        other => crate::i18n::t(other),
    }
}

fn effects(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, clip: Option<ClipId>, grade: &Grade) {
    panel_bg(ui, r, c::PANEL);
    let ctx = ui.ctx().clone();
    let mut y = r.min.y + 6.0;
    for (ci, (cat, items)) in FX.iter().enumerate() {
        let hr = Rect::from_min_size(pos2(r.min.x + 6.0, y), vec2(r.width() - 12.0, 24.0));
        if hr.max.y > r.max.y {
            break;
        }
        let open = st.fx_open.get(ci).copied().unwrap_or(true);
        let resp = ui.interact(hr, Id::new(("cor-fx-cat", ci)), Sense::click());
        app.auto.add(&format!("cor.fxcat.{ci}"), hr, crate::i18n::t(cat));
        text(ui, pos2(hr.min.x + 4.0, hr.center().y), Align2::LEFT_CENTER, crate::i18n::t(cat), 11.5, c::TEXT);
        text(ui, pos2(hr.max.x - 6.0, hr.center().y), Align2::RIGHT_CENTER, if open { "⌃" } else { "⌄" }, 11.0, c::DIM);
        if resp.clicked()
            && let Some(o) = st.fx_open.get_mut(ci)
        {
            *o = !open;
        }
        y += 26.0;
        if !open {
            continue;
        }
        for (ty, label, g) in items.iter() {
            let ir = Rect::from_min_size(pos2(r.min.x + 8.0, y), vec2(r.width() - 16.0, 30.0));
            if ir.max.y > r.max.y {
                break;
            }
            let resp = ui.interact(ir, Id::new(("cor-fx", *ty)), if clip.is_some() { Sense::click() } else { Sense::hover() });
            let shown = fx_label(label);
            app.auto.add(&format!("cor.fx.{ty}"), ir, shown);
            ui.painter().rect_filled(ir, 3.0, if resp.hovered() { c::FIELD.gamma_multiply(1.5) } else { c::FIELD });
            ui.painter().rect_stroke(ir, 3.0, Stroke::new(1.0, c::SEP), StrokeKind::Inside);
            let ib = Rect::from_min_size(pos2(ir.min.x + 6.0, ir.min.y + 5.0), vec2(28.0, 20.0));
            ui.painter().rect_filled(ib, 2.0, c::BAR);
            glyph(ui.painter(), Rect::from_center_size(ib.center(), vec2(13.0, 13.0)), *g, c::DIM);
            text(ui, pos2(ib.max.x + 10.0, ir.center().y), Align2::LEFT_CENTER, shown, 11.0, c::TEXT);
            if resp.clicked()
                && let Some(clip) = clip
            {
                add_node(app, &ctx, st, clip, grade, ty);
            }
            y += 34.0;
        }
        y += 4.0;
    }
}

// ---------------------------------------------------------------------------------- clip strip

fn clip_strip(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, current: Option<ClipId>, clips: &[ClipInfo]) {
    panel_bg(ui, r, c::BLACK);
    let ctx = ui.ctx().clone();
    let tw = (r.width() * 0.1).clamp(96.0, 150.0);
    let gap = 4.0;
    let rate = app.session.sequence_rate();
    let mut clicked: Option<&ClipInfo> = None;
    let inner = r.shrink2(vec2(4.0, 3.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
        egui::ScrollArea::horizontal().id_salt("cor-clips").show(ui, |ui| {
            let (strip, _) = ui.allocate_exact_size(vec2(clips.len() as f32 * (tw + gap), inner.height()), Sense::hover());
            for (i, ci) in clips.iter().enumerate() {
                let tile = Rect::from_min_size(pos2(strip.min.x + i as f32 * (tw + gap), strip.min.y), vec2(tw, strip.height()));
                let is_cur = current == Some(ci.id);
                let resp = ui.interact(tile, Id::new(("cor-clip", ci.id.0)), Sense::click());
                app.auto.add(&format!("cor.clip.{}", i + 1), tile, &format!("{:02}", i + 1));
                let p = ui.painter();
                let head = Rect::from_min_size(tile.min, vec2(tw, 16.0));
                // number badge, timecode, track
                let badge = Rect::from_min_size(head.min + vec2(1.0, 1.0), vec2(20.0, 14.0));
                p.rect_stroke(badge, 2.0, Stroke::new(1.0, if is_cur { KODA_GREEN } else { c::DIM }), StrokeKind::Inside);
                text(ui, badge.center(), Align2::CENTER_CENTER, format!("{:02}", i + 1), 9.5, c::TEXT);
                let tc = filmcraft_time::format_time(ci.start, rate, false, filmcraft_time::TimeDisplay::Timecode, 48_000);
                text(ui, pos2(badge.max.x + 6.0, head.center().y), Align2::LEFT_CENTER, tc, 10.0, if is_cur { KODA_GREEN } else { c::BLUE });
                text(ui, pos2(head.max.x - 3.0, head.center().y), Align2::RIGHT_CENTER, &ci.track, 10.0, c::DIM);
                let th = Rect::from_min_max(pos2(tile.min.x, head.max.y + 3.0), pos2(tile.max.x, tile.max.y - 15.0));
                ui.painter().rect_filled(th, 1.0, Color32::from_gray(18));
                if let Some((tex, sz)) = app.thumbnail(&ctx, ci.item, ci.source_in, 160) {
                    let fit = crate::panels::monitor::fit(th, sz.x, sz.y);
                    ui.painter().image(tex, fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                ui.painter().rect_stroke(
                    th,
                    1.0,
                    Stroke::new(if is_cur { 1.6 } else { 1.0 }, if is_cur { KODA_GREEN } else { Color32::from_gray(40) }),
                    StrokeKind::Outside,
                );
                text(ui, pos2(tile.min.x + 1.0, tile.max.y - 6.0), Align2::LEFT_CENTER, &ci.codec, 9.5, c::DIM);
                if resp.clicked() {
                    clicked = Some(ci);
                }
            }
        });
    });
    if let Some(ci) = clicked {
        app.session.state.selection = vec![ci.id];
        run(app, &ctx, "playhead.set", json!({ "time": ci.start.0 }));
    }
}

// --------------------------------------------------------------------------------- palette bar

fn palettes_bar(app: &mut FilmcraftApp, ui: &egui::Ui, r: Rect, st: &mut CorState) {
    panel_bg(ui, r, c::PANEL);
    let left: [(&str, Glyph, &str, Option<Palette>); 10] = [
        ("primaries", Glyph::Primaries, "Primaries", Some(Palette::Primaries)),
        ("hdr", Glyph::Hdr, "HDR", None),
        ("mixer", Glyph::Mixer, "RGB Mixer", None),
        ("curves", Glyph::Curves, "Curves", Some(Palette::Curves)),
        ("qualifier", Glyph::Qualifier, "Qualifier", None),
        ("windows", Glyph::Windows, "Windows", None),
        ("tracker", Glyph::Tracker, "Tracker", None),
        ("blur", Glyph::Blur, "Blur", None),
        ("key", Glyph::Key, "Key", None),
        ("sizing", Glyph::Sizing, "Sizing", None),
    ];
    let step = (r.width() * 0.04).clamp(26.0, 44.0);
    for (i, (id, g, label, pal)) in left.iter().enumerate() {
        let b = Rect::from_center_size(pos2(r.min.x + 22.0 + i as f32 * step, r.center().y), vec2(26.0, r.height() - 4.0));
        let enabled = pal.is_some();
        let resp = ui.interact(b, Id::new(("cor-pal", *id)), if enabled { Sense::click() } else { Sense::hover() });
        let resp =
            resp.on_hover_text(if enabled { crate::i18n::t(label).to_string() } else { format!("{} — {}", crate::i18n::t(label), tl!("not available yet")) });
        app.auto.add(&format!("cor.palette.{id}"), b, crate::i18n::t(label));
        let on = *pal == Some(st.palette);
        let col = if on {
            KODA_GREEN
        } else if !enabled {
            c::FAINT
        } else if resp.hovered() {
            c::TEXT
        } else {
            c::DIM
        };
        glyph(ui.painter(), Rect::from_center_size(b.center(), vec2(16.0, 16.0)), *g, col);
        if on {
            ui.painter().line_segment([pos2(b.min.x + 4.0, r.max.y - 1.5), pos2(b.max.x - 4.0, r.max.y - 1.5)], Stroke::new(1.5, KODA_GREEN));
        }
        if let Some(p) = pal
            && resp.clicked()
        {
            st.palette = *p;
        }
    }
    let right: [(&str, Glyph, &str); 3] = [("info", Glyph::Info, "Info"), ("scopes", Glyph::Scopes, "Scopes"), ("keyframes", Glyph::Keyframes, "Keyframes")];
    for (i, (id, g, label)) in right.iter().enumerate() {
        let b = Rect::from_center_size(pos2(r.max.x - 22.0 - i as f32 * step, r.center().y), vec2(26.0, r.height() - 4.0));
        let enabled = *id == "scopes";
        let resp = ui.interact(b, Id::new(("cor-pal", *id)), if enabled { Sense::click() } else { Sense::hover() });
        let resp =
            resp.on_hover_text(if enabled { crate::i18n::t(label).to_string() } else { format!("{} — {}", crate::i18n::t(label), tl!("not available yet")) });
        app.auto.add(&format!("cor.palette.{id}"), b, crate::i18n::t(label));
        let on = enabled && st.show_scopes;
        glyph(
            ui.painter(),
            Rect::from_center_size(b.center(), vec2(16.0, 16.0)),
            *g,
            if on {
                KODA_GREEN
            } else if enabled {
                c::DIM
            } else {
                c::FAINT
            },
        );
        if enabled && resp.clicked() {
            st.show_scopes = !st.show_scopes;
        }
    }
}

// -------------------------------------------------------------------------------------- scopes

fn scopes(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState) {
    use filmcraft_scopes::ScopeKind;
    panel_bg(ui, r, c::PANEL);
    let head = Rect::from_min_size(r.min, vec2(r.width(), 24.0));
    text(ui, pos2(head.min.x + 10.0, head.center().y), Align2::LEFT_CENTER, tl!("Scopes"), 11.5, c::TEXT);
    let kinds =
        [(ScopeKind::Parade, "Parade"), (ScopeKind::Waveform, "Waveform"), (ScopeKind::VectorscopeYuv, "Vectorscope"), (ScopeKind::Histogram, "Histogram")];
    let mut x = head.max.x - 8.0;
    for (k, label) in kinds.iter().rev() {
        let w = 74.0;
        let b = Rect::from_min_max(pos2(x - w, head.min.y + 3.0), pos2(x, head.max.y - 3.0));
        x -= w + 4.0;
        let resp = ui.interact(b, Id::new(("cor-scope", k.name())), Sense::click());
        app.auto.add(&format!("cor.scope.{}", k.name()), b, crate::i18n::t(label));
        let on = st.scope == *k;
        ui.painter().rect_filled(b, 3.0, if on { c::FIELD } else { c::PANEL });
        text(ui, b.center(), Align2::CENTER_CENTER, crate::i18n::t(label), 10.0, if on { KODA_GREEN } else { c::DIM });
        if resp.clicked() {
            st.scope = *k;
        }
    }
    let plot = Rect::from_min_max(pos2(r.min.x + 34.0, head.max.y + 4.0), pos2(r.max.x - 6.0, r.max.y - 6.0));
    ui.painter().rect_filled(plot, 0.0, Color32::BLACK);
    let ctx = ui.ctx().clone();
    let mut sst = app.ui.panels.scopes.clone();
    sst.shown = vec![st.scope];
    let Some(_) = app.session.state.active_sequence else { return };
    let frame = app.session.sequence_rate().frame_at(app.session.playhead());
    let prio = if app.playback.playing { 40 } else { 2 };
    let Some(sf) = crate::panels::scopes::frame_signal(app, &ctx, "cor", frame, &sst, prio) else { return };
    if !matches!(st.scope, ScopeKind::Parade | ScopeKind::Waveform) {
        crate::panels::scopes::draw(app, ui, plot, &[st.scope], &sf, &sst, "cor.scopes");
        return;
    }
    // Parade / Waveform drawn here, on DaVinci's 10-bit scale (0–1023, a line every 128)
    let rows = (plot.height() as usize).clamp(64, 256);
    let params = filmcraft_scopes::Params { rows, ..Default::default() };
    let parade = st.scope == ScopeKind::Parade;
    let w = if parade {
        filmcraft_scopes::parade(&sf.signal, filmcraft_scopes::ParadeType::Rgb, &params)
    } else {
        filmcraft_scopes::waveform(&sf.signal, filmcraft_scopes::WaveformType::Rgb, &params)
    };
    let (lo, hi) = (w.lo, w.hi);
    let key = (sf.key, parade, rows);
    let tid = Id::new(("cor-scope-tex", parade));
    let cached: Option<((u64, bool, usize), egui::TextureHandle)> = ctx.data(|d| d.get_temp(tid));
    let tex = match cached {
        Some((k, t)) if k == key => Some(t),
        _ => {
            let (iw, ih, px) = scope_image(&w, parade);
            (iw > 0 && ih > 0).then(|| {
                let t = ctx.load_texture("cor-scope", egui::ColorImage::from_rgba_unmultiplied([iw, ih], &px), egui::TextureOptions::LINEAR);
                ctx.data_mut(|d| d.insert_temp(tid, (key, t.clone())));
                t
            })
        }
    };
    let span = (hi - lo).max(1e-6);
    let y_of = |v: f32| plot.max.y - plot.height() * ((v - lo) / span);
    for v in (0..=1023).step_by(128).chain(std::iter::once(1023)) {
        let yy = y_of(v as f32 / 1023.0);
        if yy < plot.min.y - 1.0 || yy > plot.max.y + 1.0 {
            continue;
        }
        ui.painter().line_segment([pos2(plot.min.x, yy), pos2(plot.max.x, yy)], Stroke::new(0.5, Color32::from_gray(52)));
        text(ui, pos2(plot.min.x - 4.0, yy), Align2::RIGHT_CENTER, v, 9.0, c::DIM);
    }
    if let Some(tex) = tex {
        ui.painter().image(tex.id(), plot, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    app.auto.add(&format!("cor.scopes.view.{}", st.scope.name()), plot, st.scope.label());
}

/// A parade (traces side by side) or an RGB waveform (traces over each other) as an image.
fn scope_image(w: &filmcraft_scopes::Waveform, parade: bool) -> (usize, usize, Vec<u8>) {
    let col = |name: &str| match name {
        "R" => [1.0, 0.32, 0.32],
        "G" => [0.35, 1.0, 0.45],
        "B" => [0.4, 0.55, 1.0],
        _ => [0.9, 0.9, 0.9],
    };
    let reference = filmcraft_scopes::paint::waveform_reference(w.per_column);
    if !parade {
        let layers: Vec<(&filmcraft_scopes::Grid, [f32; 3])> = w.traces.iter().map(|g| (g, col(g.name))).collect();
        return filmcraft_scopes::paint::grids(&layers, reference, 2.2);
    }
    let n = w.traces.len();
    let Some(first) = w.traces.first() else { return (0, 0, Vec::new()) };
    let (cw, rows) = (first.cols, first.rows);
    let gap = (cw / 24).max(2);
    let width = cw * n + gap * n.saturating_sub(1);
    let mut out = vec![0u8; width * rows * 4];
    for (i, g) in w.traces.iter().enumerate() {
        let (gw, gh, px) = filmcraft_scopes::paint::grids(&[(g, col(g.name))], reference, 2.2);
        if gw != cw || gh != rows {
            continue;
        }
        let x0 = i * (cw + gap);
        for y in 0..rows {
            if let (Some(src), Some(dst)) = (px.get(y * cw * 4..(y + 1) * cw * 4), out.get_mut((y * width + x0) * 4..(y * width + x0 + cw) * 4)) {
                dst.copy_from_slice(src);
            }
        }
    }
    (width, rows, out)
}

// -------------------------------------------------------------------------------------- viewer

fn viewer(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, clip: Option<ClipId>, grade: &Grade) {
    panel_bg(ui, r, c::VIEWER);
    let ctx = ui.ctx().clone();
    let head = Rect::from_min_size(r.min, vec2(r.width(), 26.0));
    ui.painter().rect_filled(head, 0.0, c::BAR);
    let name = sequence_name(app);
    text(ui, head.center(), Align2::CENTER_CENTER, &name, 11.0, c::TEXT);
    let rate = app.session.sequence_rate();
    let tc = filmcraft_time::format_time(app.session.playhead(), rate, false, filmcraft_time::TimeDisplay::Timecode, 48_000);
    text(ui, pos2(head.max.x - 10.0, head.center().y), Align2::RIGHT_CENTER, tc, 11.0, c::TEXT);
    // before / after: the grade's bypass
    let b = Rect::from_min_size(pos2(head.min.x + 8.0, head.min.y + 4.0), vec2(96.0, 18.0));
    let resp = ui.interact(b, Id::new("cor-bypass"), if clip.is_some() { Sense::click() } else { Sense::hover() });
    app.auto.add("cor.viewer.bypass", b, tl!("Before / after"));
    let on = grade.bypass;
    ui.painter().rect_filled(b, 3.0, if on { KODA_GREEN.gamma_multiply(0.35) } else { c::FIELD });
    text(ui, b.center(), Align2::CENTER_CENTER, if on { tl!("Before (bypass)") } else { tl!("Before / after") }, 10.0, if on { c::TEXT } else { c::DIM });
    if resp.clicked()
        && let Some(clip) = clip
    {
        run(app, &ctx, "koda.color.setBypass", json!({ "clip": clip, "bypass": !on }));
    }
    let body = Rect::from_min_max(pos2(r.min.x, head.max.y), r.max);
    crate::panels::monitor::show(app, ui, body, crate::panels::monitor::Which::Program);
}

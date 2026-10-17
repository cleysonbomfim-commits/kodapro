//! The palettes of the página Cor (marco 5): Primaries in bars or wheels (Lift / Gamma / Gain /
//! Offset, Temp, Tint, Contrast, Pivot, Mid Detail, Color Boost, Shadows, Highlights, Saturation,
//! Hue, Lum Mix) and the custom curves Y / R / G / B, bound to the selected primaries node.
//!
//! Every change is a `koda.color.setParam` / `koda.color.setCurve` with a `merge` key per drag, so
//! one drag is one undo step. Numeric fields: drag sideways to change (Shift: finer), double-click
//! to type a value, Enter to apply, Esc to cancel.
//!
//! Automation ids: `cor.prim.<param>` (e.g. `cor.prim.temp`, `cor.prim.lift.y` for the field,
//! `cor.prim.bar.lift.y`, `cor.prim.jog.gain`, `cor.prim.wheel.offset`), `cor.prim.mode.bars`,
//! `cor.prim.mode.wheels`, `cor.prim.reset`, `cor.prim.reset.<group>`, `cor.prim.add`;
//! `cor.curve.<y|r|g|b>`, `cor.curve.link`, `cor.curve.reset`, `cor.curve.editor`,
//! `cor.curve.point.<n>`.

use egui::{Align2, Color32, Id, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};
use filmcraft_project::ClipId;
use koda_color::primaries::{CurvePoints, Mode, Primaries};
use koda_color::{Grade, NodeKind};
use serde_json::{Value, json};

use super::cor::{CorState, KODA_GREEN, c, run, text};
use crate::FilmcraftApp;

/// The selected node when it is a primaries node: (node id, its parameters).
fn selected_primaries(st: &CorState, grade: &Grade) -> Option<(u32, Primaries)> {
    let n = grade.nodes.iter().find(|n| Some(n.id) == st.node)?;
    match &n.kind {
        NodeKind::Primaries(p) => Some((n.id, (**p).clone())),
        _ => None,
    }
}

/// Where edits go: the clip and the primaries node.
#[derive(Clone, Copy)]
struct Target {
    clip: ClipId,
    node: u32,
}

fn set(app: &mut FilmcraftApp, ctx: &egui::Context, t: Option<Target>, param: &str, value: Value, merge: Option<&str>) {
    let Some(t) = t else { return };
    let mut p = json!({ "clip": t.clip, "node": t.node, "param": param, "value": value });
    if let Some(m) = merge {
        p["merge"] = json!(m);
    }
    run(app, ctx, "koda.color.setParam", p);
}

/// A drag's undo key: a new one when the drag starts.
fn merge_key(st: &mut CorState, id: &str, started: bool) -> String {
    if started {
        st.drag_seq = st.drag_seq.wrapping_add(1);
    }
    format!("cor-{id}-{}", st.drag_seq)
}

/// A numeric field: shows `value`, drag sideways to change it, double-click to type. Returns the
/// new value and the undo key of the drag (None when typed).
#[allow(clippy::too_many_arguments)]
fn field(
    app: &mut FilmcraftApp,
    ui: &mut egui::Ui,
    r: Rect,
    st: &mut CorState,
    id: &str,
    value: f32,
    decimals: usize,
    step: f32,
    underline: Option<Color32>,
    enabled: bool,
) -> Option<(f32, Option<String>)> {
    let auto = format!("cor.prim.{id}");
    app.auto.add(&auto, r, id);
    let editing = st.edit.as_ref().is_some_and(|(e, _)| e == id);
    if editing && enabled {
        let mut s = st.edit.as_ref().map(|(_, s)| s.clone()).unwrap_or_default();
        let resp = ui.put(r, egui::TextEdit::singleline(&mut s).font(egui::FontId::proportional(11.0)).horizontal_align(egui::Align::Center));
        resp.request_focus();
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if esc {
            st.edit = None;
            return None;
        }
        if enter || resp.lost_focus() {
            st.edit = None;
            return s.trim().replace(',', ".").parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| (v, None));
        }
        st.edit = Some((id.to_string(), s));
        return None;
    }
    let resp = ui.interact(r, Id::new(("cor-field", id)), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    let col = if !enabled {
        c::FAINT
    } else if resp.hovered() || resp.dragged() {
        c::TEXT
    } else {
        Color32::from_gray(0xd8)
    };
    ui.painter().rect_filled(r, 2.0, if resp.dragged() { c::FIELD.gamma_multiply(1.6) } else { c::FIELD });
    text(ui, r.center(), Align2::CENTER_CENTER, format!("{value:.decimals$}"), 11.0, col);
    if let Some(u) = underline {
        ui.painter()
            .line_segment([pos2(r.min.x + 2.0, r.max.y + 1.5), pos2(r.max.x - 2.0, r.max.y + 1.5)], Stroke::new(1.5, if enabled { u } else { c::FAINT }));
    }
    if !enabled {
        return None;
    }
    if resp.double_clicked() {
        st.edit = Some((id.to_string(), format!("{value:.decimals$}")));
        return None;
    }
    if resp.dragged() {
        let dx = resp.drag_delta().x;
        if dx != 0.0 {
            let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
            let key = merge_key(st, id, false);
            return Some((value + dx * step * fine, Some(key)));
        }
    }
    if resp.drag_started() {
        merge_key(st, id, true);
    }
    None
}

/// A labelled field in a row ("Temp  -69.9").
#[allow(clippy::too_many_arguments)]
fn labelled(
    app: &mut FilmcraftApp,
    ui: &mut egui::Ui,
    st: &mut CorState,
    t: Option<Target>,
    r: Rect,
    label: &str,
    id: &str,
    value: f32,
    decimals: usize,
    step: f32,
) {
    let ctx = ui.ctx().clone();
    text(ui, pos2(r.min.x, r.center().y), Align2::LEFT_CENTER, label, 10.5, c::DIM);
    let lw = (label.len() as f32 * 6.2 + 8.0).min(r.width() * 0.55);
    let fr = Rect::from_min_max(pos2(r.min.x + lw, r.min.y + 2.0), pos2(r.max.x - 4.0, r.max.y - 3.0));
    if let Some((v, merge)) = field(app, ui, fr, st, id, value, decimals, step, Some(c::DIM), t.is_some()) {
        set(app, &ctx, t, id, json!(v), merge.as_deref());
    }
}

/// Channel colours: Y, R, G, B.
const CH: [Color32; 4] = [c::TEXT, c::RED, c::GREEN, c::BLUE];
const CH_NAME: [&str; 4] = ["y", "r", "g", "b"];

/// A group's values, neutral, visual span and drag steps.
struct Group {
    id: &'static str,
    title: &'static str,
    values: Vec<f32>,
    neutral: f32,
    /// The bar shows neutral ± span.
    span: f32,
    field_step: f32,
    decimals: usize,
}

fn groups(p: &Primaries) -> [Group; 4] {
    [
        Group { id: "lift", title: "Lift", values: p.lift.to_vec(), neutral: 0.0, span: 0.5, field_step: 0.002, decimals: 2 },
        Group { id: "gamma", title: "Gamma", values: p.gamma.to_vec(), neutral: 0.0, span: 0.5, field_step: 0.002, decimals: 2 },
        Group { id: "gain", title: "Gain", values: p.gain.to_vec(), neutral: 1.0, span: 1.0, field_step: 0.004, decimals: 2 },
        Group { id: "offset", title: "Offset", values: p.offset.to_vec(), neutral: 25.0, span: 25.0, field_step: 0.1, decimals: 2 },
    ]
}

/// Channel index of value `i` in a group (Offset has no Y).
fn channel(g: &Group, i: usize) -> usize {
    if g.id == "offset" { i + 1 } else { i }
}

pub fn primaries(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, clip: Option<ClipId>, grade: &Grade) {
    super::cor::panel_bg(ui, r, c::PANEL);
    let ctx = ui.ctx().clone();
    let sel = selected_primaries(st, grade);
    let t = clip.zip(sel.as_ref()).map(|(clip, (node, _))| Target { clip, node: *node });
    let p = sel.map(|(_, p)| p).unwrap_or_default();
    let wheels = p.mode == Mode::Wheels;
    // header: title, bars / wheels, reset
    let head = Rect::from_min_size(r.min, vec2(r.width(), 24.0));
    let title = if wheels { tl!("Primaries – Wheels") } else { tl!("Primaries – Bars") };
    text(ui, pos2(head.min.x + 10.0, head.center().y), Align2::LEFT_CENTER, title, 11.5, c::TEXT);
    let mut x = head.max.x - 8.0;
    for (id, label, on) in [("reset", "↺", false), ("mode.bars", "▮▮", !wheels), ("mode.wheels", "◎", wheels)] {
        let b = Rect::from_min_max(pos2(x - 24.0, head.min.y + 3.0), pos2(x, head.max.y - 3.0));
        x -= 28.0;
        let resp = ui.interact(b, Id::new(("cor-prim-head", id)), if t.is_some() { Sense::click() } else { Sense::hover() });
        app.auto.add(&format!("cor.prim.{id}"), b, id);
        ui.painter().rect_filled(b, 3.0, if on { c::FIELD } else { c::PANEL });
        text(
            ui,
            b.center(),
            Align2::CENTER_CENTER,
            label,
            11.0,
            if on {
                KODA_GREEN
            } else if t.is_some() {
                c::DIM
            } else {
                c::FAINT
            },
        );
        if resp.clicked() {
            match id {
                "reset" => {
                    if let Some(t) = t {
                        run(app, &ctx, "koda.color.resetParams", json!({ "clip": t.clip, "node": t.node, "what": "all" }));
                    }
                }
                "mode.bars" => set(app, &ctx, t, "mode", json!("bars"), None),
                _ => set(app, &ctx, t, "mode", json!("wheels"), None),
            }
        }
    }
    // top row: Temp, Tint, Cont, Pivot, MD
    let row1 = Rect::from_min_size(pos2(r.min.x + 30.0, head.max.y + 2.0), vec2(r.width() - 40.0, 22.0));
    super::cor::glyph(ui.painter(), Rect::from_center_size(pos2(r.min.x + 16.0, row1.center().y), vec2(14.0, 14.0)), super::cor::Glyph::Qualifier, c::FAINT);
    let top: [(&str, &str, f32, usize, f32); 5] = [
        ("Temp", "temp", p.temp, 1, 1.0),
        ("Tint", "tint", p.tint, 2, 0.05),
        ("Cont", "contrast", p.contrast, 3, 0.002),
        ("Pivot", "pivot", p.pivot, 3, 0.001),
        ("MD", "midDetail", p.mid_detail, 2, 0.2),
    ];
    let cw = row1.width() / top.len() as f32;
    for (i, (label, id, v, d, step)) in top.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(row1.min.x + i as f32 * cw, row1.min.y), vec2(cw - 6.0, row1.height()));
        labelled(app, ui, st, t, cell, label, id, *v, *d, *step);
    }
    // bottom row: Col Boost, Shad, Hi/Light, Sat, Hue, L. Mix
    let row3 = Rect::from_min_max(pos2(r.min.x + 8.0, r.max.y - 26.0), pos2(r.max.x - 8.0, r.max.y - 4.0));
    let bottom: [(&str, &str, f32, f32); 6] = [
        ("Col Boost", "colorBoost", p.color_boost, 0.2),
        ("Shad", "shadows", p.shadows, 0.2),
        ("Hi/Light", "highlights", p.highlights, 0.2),
        ("Sat", "sat", p.sat, 0.2),
        ("Hue", "hue", p.hue, 0.2),
        ("L. Mix", "lumMix", p.lum_mix, 0.2),
    ];
    let cw = row3.width() / bottom.len() as f32;
    for (i, (label, id, v, step)) in bottom.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(row3.min.x + i as f32 * cw, row3.min.y), vec2(cw - 6.0, row3.height()));
        labelled(app, ui, st, t, cell, label, id, *v, 2, *step);
    }
    // the four groups
    let area = Rect::from_min_max(pos2(r.min.x + 6.0, row1.max.y + 6.0), pos2(r.max.x - 6.0, row3.min.y - 6.0));
    let gw = area.width() / 4.0;
    for (gi, g) in groups(&p).into_iter().enumerate() {
        let gr = Rect::from_min_size(pos2(area.min.x + gi as f32 * gw, area.min.y), vec2(gw - 6.0, area.height()));
        group(app, ui, gr, st, t, &g, wheels);
    }
    if t.is_none() {
        // no primaries node selected: a way to add one
        let b = Rect::from_center_size(area.center(), vec2(220.0, 30.0));
        ui.painter().rect_filled(b.expand(6.0), 6.0, c::PANEL.gamma_multiply(0.9));
        let resp = ui.interact(b, Id::new("cor-prim-add"), if clip.is_some() { Sense::click() } else { Sense::hover() });
        app.auto.add("cor.prim.add", b, tl!("Add a primaries node"));
        ui.painter().rect_filled(b, 4.0, if resp.hovered() { KODA_GREEN.gamma_multiply(0.45) } else { c::FIELD });
        ui.painter().rect_stroke(b, 4.0, Stroke::new(1.0, KODA_GREEN), StrokeKind::Inside);
        text(ui, b.center(), Align2::CENTER_CENTER, tl!("Add a primaries node"), 11.0, c::TEXT);
        if resp.clicked()
            && let Some(clip) = clip
        {
            super::cor::add_node(app, &ctx, st, clip, grade, "primaries");
        }
    }
}

/// One of Lift / Gamma / Gain / Offset: title, reset, bars or a wheel, fields, jog wheel.
fn group(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, t: Option<Target>, g: &Group, wheels: bool) {
    let ctx = ui.ctx().clone();
    let enabled = t.is_some();
    // title and reset
    text(ui, pos2(r.center().x, r.min.y + 8.0), Align2::CENTER_CENTER, g.title, 11.5, c::TEXT);
    let rb = Rect::from_center_size(pos2(r.max.x - 10.0, r.min.y + 8.0), vec2(16.0, 16.0));
    let resp = ui.interact(rb, Id::new(("cor-reset", g.id)), if enabled { Sense::click() } else { Sense::hover() });
    app.auto.add(&format!("cor.prim.reset.{}", g.id), rb, g.title);
    text(ui, rb.center(), Align2::CENTER_CENTER, "↺", 11.0, if resp.hovered() { c::TEXT } else { c::DIM });
    if resp.clicked()
        && let Some(t) = t
    {
        run(app, &ctx, "koda.color.resetParams", json!({ "clip": t.clip, "node": t.node, "what": g.id }));
    }
    let n = g.values.len();
    let fields_y = r.max.y - 40.0;
    let jog = Rect::from_min_max(pos2(r.min.x + 6.0, r.max.y - 14.0), pos2(r.max.x - 6.0, r.max.y - 2.0));
    let body = Rect::from_min_max(pos2(r.min.x, r.min.y + 20.0), pos2(r.max.x, fields_y - 6.0));
    if wheels {
        wheel(app, ui, body, st, t, g);
    } else {
        // vertical bars, one per channel
        let bw = 12.0;
        let gap = ((r.width() - n as f32 * bw) / (n as f32 + 1.0)).clamp(4.0, 22.0);
        let total = n as f32 * bw + (n as f32 - 1.0) * gap;
        let x0 = r.center().x - total / 2.0;
        for i in 0..n {
            let ch = channel(g, i);
            let track = Rect::from_min_max(pos2(x0 + i as f32 * (bw + gap), body.min.y + 4.0), pos2(x0 + i as f32 * (bw + gap) + bw, body.max.y - 2.0));
            let id = format!("bar.{}.{}", g.id, CH_NAME[ch]);
            let resp = ui.interact(track.expand2(vec2(4.0, 0.0)), Id::new(("cor-bar", g.id, ch)), if enabled { Sense::drag() } else { Sense::hover() });
            app.auto.add(&format!("cor.prim.{id}"), track, &id);
            let p = ui.painter();
            p.rect_filled(track, 3.0, c::BLACK);
            p.rect_stroke(track, 3.0, Stroke::new(1.0, Color32::from_gray(52)), StrokeKind::Inside);
            let v = g.values.get(i).copied().unwrap_or(g.neutral);
            let f = ((v - g.neutral) / g.span).clamp(-1.0, 1.0);
            let yv = track.center().y - f * (track.height() / 2.0 - 4.0);
            let col = if enabled { CH[ch] } else { c::FAINT };
            p.rect_filled(Rect::from_center_size(pos2(track.center().x, yv), vec2(bw + 2.0, 3.0)), 1.0, col);
            if (v - g.neutral).abs() > 1e-6 {
                p.line_segment([pos2(track.center().x, track.center().y), pos2(track.center().x, yv)], Stroke::new(2.0, col.gamma_multiply(0.6)));
            }
            if resp.drag_started() {
                merge_key(st, &id, true);
            }
            if resp.dragged() {
                let dy = resp.drag_delta().y;
                if dy != 0.0 {
                    let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
                    let nv = v - dy * g.span / (track.height() / 2.0) * fine;
                    let key = merge_key(st, &id, false);
                    set(app, &ctx, t, &format!("{}.{}", g.id, CH_NAME[ch]), json!(nv), Some(&key));
                }
            }
        }
    }
    // numeric fields under the bars / wheel
    let fw = ((r.width() - 8.0) / n as f32 - 4.0).clamp(26.0, 46.0);
    let total = n as f32 * (fw + 4.0) - 4.0;
    let fx0 = r.center().x - total / 2.0;
    for i in 0..n {
        let ch = channel(g, i);
        let fr = Rect::from_min_size(pos2(fx0 + i as f32 * (fw + 4.0), fields_y), vec2(fw, 18.0));
        let id = format!("{}.{}", g.id, CH_NAME[ch]);
        let v = g.values.get(i).copied().unwrap_or(g.neutral);
        if let Some((nv, merge)) = field(app, ui, fr, st, &id, v, g.decimals, g.field_step, Some(CH[ch]), enabled) {
            set(app, &ctx, t, &id, json!(nv), merge.as_deref());
        }
    }
    // jog wheel: the master (Y; Offset moves R, G and B together)
    let resp = ui.interact(jog, Id::new(("cor-jog", g.id)), if enabled { Sense::drag() } else { Sense::hover() });
    app.auto.add(&format!("cor.prim.jog.{}", g.id), jog, g.title);
    let p = ui.painter();
    p.rect_filled(jog, 3.0, c::BLACK);
    let phase = g.values.first().copied().unwrap_or(0.0) * 40.0;
    let ticks = (jog.width() / 4.0) as i32;
    for k in 0..ticks {
        let xx = jog.min.x + ((k as f32 * 4.0 + phase).rem_euclid(jog.width()));
        p.line_segment([pos2(xx, jog.min.y + 2.0), pos2(xx, jog.max.y - 2.0)], Stroke::new(1.0, Color32::from_gray(if k % 4 == 0 { 92 } else { 58 })));
    }
    if resp.drag_started() {
        merge_key(st, &format!("jog.{}", g.id), true);
    }
    if resp.dragged() {
        let dx = resp.drag_delta().x;
        if dx != 0.0 {
            let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
            let d = dx * g.field_step * fine;
            let key = merge_key(st, &format!("jog.{}", g.id), false);
            if g.id == "offset" {
                let nv: Vec<f32> = g.values.iter().map(|v| v + d).collect();
                set(app, &ctx, t, "offset", json!(nv), Some(&key));
            } else {
                let v = g.values.first().copied().unwrap_or(g.neutral);
                set(app, &ctx, t, &format!("{}.y", g.id), json!(v + d), Some(&key));
            }
        }
    }
}

/// Colour-wheel directions of R, G and B (120° apart, red towards the top left).
fn dirs() -> [egui::Vec2; 3] {
    [103.0f32, 223.0, 343.0].map(|deg| {
        let a = deg.to_radians();
        vec2(a.cos(), -a.sin())
    })
}

/// A colour wheel: the R, G, B balance as a point; dragging it moves R, G and B (Y kept).
fn wheel(app: &mut FilmcraftApp, ui: &mut egui::Ui, body: Rect, st: &mut CorState, t: Option<Target>, g: &Group) {
    let ctx = ui.ctx().clone();
    let rad = (body.width().min(body.height()) / 2.0 - 6.0).max(10.0);
    let cc = body.center();
    let enabled = t.is_some();
    let p = ui.painter();
    // a hue ring and the crosshair
    let steps = 72;
    for k in 0..steps {
        let a0 = k as f32 / steps as f32 * std::f32::consts::TAU;
        let a1 = (k + 1) as f32 / steps as f32 * std::f32::consts::TAU;
        let hue = egui::ecolor::Hsva::new(((103.0f32.to_radians() - a0) / std::f32::consts::TAU).rem_euclid(1.0), 0.55, 0.75, 1.0);
        p.line_segment(
            [cc + vec2(a0.cos(), -a0.sin()) * rad, cc + vec2(a1.cos(), -a1.sin()) * rad],
            Stroke::new(2.5, Color32::from(hue).gamma_multiply(if enabled { 1.0 } else { 0.35 })),
        );
    }
    p.circle_filled(cc, rad - 2.0, c::BLACK);
    p.line_segment([cc - vec2(rad * 0.15, 0.0), cc + vec2(rad * 0.15, 0.0)], Stroke::new(1.0, c::FAINT));
    p.line_segment([cc - vec2(0.0, rad * 0.15), cc + vec2(0.0, rad * 0.15)], Stroke::new(1.0, c::FAINT));
    // the balance point
    let rgb: Vec<f32> = if g.id == "offset" { g.values.clone() } else { g.values.iter().skip(1).copied().collect() };
    let mean = rgb.iter().sum::<f32>() / rgb.len().max(1) as f32;
    let d = dirs();
    let mut v = vec2(0.0, 0.0);
    for (i, x) in rgb.iter().enumerate() {
        if let Some(u) = d.get(i) {
            v += *u * ((x - mean) / g.span);
        }
    }
    let dot = cc + v * rad;
    let resp =
        ui.interact(Rect::from_center_size(cc, vec2(rad * 2.0, rad * 2.0)), Id::new(("cor-wheel", g.id)), if enabled { Sense::drag() } else { Sense::hover() });
    app.auto.add(&format!("cor.prim.wheel.{}", g.id), Rect::from_center_size(cc, vec2(rad * 2.0, rad * 2.0)), g.title);
    ui.painter().circle_stroke(dot, 4.0, Stroke::new(1.5, if enabled { c::TEXT } else { c::FAINT }));
    if resp.drag_started() {
        merge_key(st, &format!("wheel.{}", g.id), true);
    }
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let fine = if ui.input(|i| i.modifiers.shift) { 0.25 } else { 1.0 };
        let q = (pos - cc) / rad * fine;
        let q = if q.length() > 1.0 { q / q.length() } else { q };
        // R, G, B around their mean so that the point lands under the pointer
        let new_rgb: Vec<f32> = (0..3).map(|i| mean + g.span * (2.0 / 3.0) * d.get(i).map_or(0.0, |u| u.dot(q))).collect();
        let key = merge_key(st, &format!("wheel.{}", g.id), false);
        if g.id == "offset" {
            set(app, &ctx, t, "offset", json!(new_rgb), Some(&key));
        } else {
            let y = g.values.first().copied().unwrap_or(g.neutral);
            set(app, &ctx, t, g.id, json!([y, new_rgb[0], new_rgb[1], new_rgb[2]]), Some(&key));
        }
    }
}

// ---------------------------------------------------------------------------------- curves

pub fn curves(app: &mut FilmcraftApp, ui: &mut egui::Ui, r: Rect, st: &mut CorState, clip: Option<ClipId>, grade: &Grade) {
    super::cor::panel_bg(ui, r, c::PANEL);
    let ctx = ui.ctx().clone();
    let sel = selected_primaries(st, grade);
    let t = clip.zip(sel.as_ref()).map(|(clip, (node, _))| Target { clip, node: *node });
    let p = sel.map(|(_, p)| p).unwrap_or_default();
    let enabled = t.is_some();
    let head = Rect::from_min_size(r.min, vec2(r.width(), 24.0));
    text(ui, pos2(head.min.x + 10.0, head.center().y), Align2::LEFT_CENTER, tl!("Curves – Custom"), 11.5, c::TEXT);
    // the editor: a square on the left, controls on the right
    let side = (r.height() - 36.0).min(r.width() * 0.62).max(40.0);
    let ed = Rect::from_min_size(pos2(r.min.x + 12.0, head.max.y + 4.0), vec2(side, side));
    let ctrl = Rect::from_min_max(pos2(ed.max.x + 16.0, ed.min.y), pos2(r.max.x - 8.0, ed.max.y));
    // channel buttons, link, reset
    let chans = [("y", "Y"), ("r", "R"), ("g", "G"), ("b", "B")];
    for (i, (id, label)) in chans.iter().enumerate() {
        let b = Rect::from_min_size(pos2(ctrl.min.x + i as f32 * 30.0, ctrl.min.y), vec2(26.0, 20.0));
        let resp = ui.interact(b, Id::new(("cor-curve-ch", *id)), Sense::click());
        app.auto.add(&format!("cor.curve.{id}"), b, label);
        let on = st.curve == i;
        ui.painter().rect_filled(b, 3.0, if on { c::FIELD } else { c::PANEL });
        ui.painter().rect_stroke(b, 3.0, Stroke::new(1.0, if on { CH[i] } else { c::SEP }), StrokeKind::Inside);
        text(ui, b.center(), Align2::CENTER_CENTER, *label, 11.0, if on { CH[i] } else { c::DIM });
        if resp.clicked() {
            st.curve = i;
        }
    }
    let lb = Rect::from_min_size(pos2(ctrl.min.x, ctrl.min.y + 28.0), vec2(116.0, 20.0));
    let resp = ui.interact(lb, Id::new("cor-curve-link"), if enabled { Sense::click() } else { Sense::hover() });
    app.auto.add("cor.curve.link", lb, tl!("Join R, G, B"));
    ui.painter().rect_filled(lb, 3.0, c::FIELD);
    text(
        ui,
        lb.center(),
        Align2::CENTER_CENTER,
        if p.curves.linked { tl!("R G B joined") } else { tl!("R G B separate") },
        10.0,
        if p.curves.linked { KODA_GREEN } else { c::DIM },
    );
    if resp.clicked() {
        set(app, &ctx, t, "curvesLinked", json!(!p.curves.linked), None);
    }
    let rb = Rect::from_min_size(pos2(ctrl.min.x, ctrl.min.y + 54.0), vec2(116.0, 20.0));
    let resp = ui.interact(rb, Id::new("cor-curve-reset"), if enabled { Sense::click() } else { Sense::hover() });
    app.auto.add("cor.curve.reset", rb, tl!("Reset curve"));
    ui.painter().rect_filled(rb, 3.0, c::FIELD);
    text(ui, rb.center(), Align2::CENTER_CENTER, format!("↺ {}", tl!("Reset curve")), 10.0, c::DIM);
    if resp.clicked()
        && let Some(t) = t
    {
        run(app, &ctx, "koda.color.resetParams", json!({ "clip": t.clip, "node": t.node, "what": format!("curve.{}", CH_NAME[st.curve]) }));
    }
    text(ui, pos2(ctrl.min.x, ctrl.min.y + 90.0), Align2::LEFT_TOP, tl!("Click to add a point, drag to move, right-click to remove."), 10.0, c::FAINT);

    // the editor
    let pnt = ui.painter();
    pnt.rect_filled(ed, 2.0, c::BLACK);
    for k in 1..4 {
        let f = k as f32 / 4.0;
        pnt.line_segment([pos2(ed.min.x + ed.width() * f, ed.min.y), pos2(ed.min.x + ed.width() * f, ed.max.y)], Stroke::new(0.5, Color32::from_gray(40)));
        pnt.line_segment([pos2(ed.min.x, ed.min.y + ed.height() * f), pos2(ed.max.x, ed.min.y + ed.height() * f)], Stroke::new(0.5, Color32::from_gray(40)));
    }
    pnt.line_segment([ed.left_bottom(), ed.right_top()], Stroke::new(0.5, Color32::from_gray(60)));
    let to_screen = |x: f32, y: f32| pos2(ed.min.x + x * ed.width(), ed.max.y - y * ed.height());
    let curves = [&p.curves.y, &p.curves.r, &p.curves.g, &p.curves.b];
    for (i, cv) in curves.iter().enumerate() {
        if i == st.curve {
            continue;
        }
        let pts: Vec<egui::Pos2> = (0..=64).map(|k| k as f32 / 64.0).map(|x| to_screen(x, cv.eval(x))).collect();
        ui.painter().add(Shape::line(pts, Stroke::new(1.0, CH[i].gamma_multiply(0.35))));
    }
    let Some(cur) = curves.get(st.curve).map(|c| (*c).clone()) else { return };
    let pts: Vec<egui::Pos2> = (0..=128).map(|k| k as f32 / 128.0).map(|x| to_screen(x, cur.eval(x).clamp(-0.1, 1.1))).collect();
    ui.painter().add(Shape::line(pts, Stroke::new(1.6, if enabled { CH[st.curve] } else { c::FAINT })));
    let resp = ui.interact(ed, Id::new("cor-curve-editor"), if enabled { Sense::click_and_drag() } else { Sense::hover() });
    app.auto.add("cor.curve.editor", ed, tl!("Curve"));
    let drag_id = Id::new("cor-curve-drag");
    let dragging: Option<usize> = ctx.data(|d| d.get_temp(drag_id));
    let hit = |pos: egui::Pos2| cur.0.iter().position(|q| to_screen(q[0], q[1]).distance(pos) < 7.0);
    for (k, q) in cur.0.iter().enumerate() {
        let sp = to_screen(q[0], q[1]);
        let on = dragging == Some(k);
        ui.painter().rect_filled(Rect::from_center_size(sp, vec2(7.0, 7.0)), 1.0, if on { KODA_GREEN } else { c::TEXT });
        app.auto.add(&format!("cor.curve.point.{k}"), Rect::from_center_size(sp, vec2(10.0, 10.0)), "point");
    }
    let Some(t) = t else { return };
    let ch = CH_NAME[st.curve];
    let send = |app: &mut FilmcraftApp, pts: Vec<[f32; 2]>, merge: Option<&str>| {
        let mut prm = json!({ "clip": t.clip, "node": t.node, "channel": ch, "points": pts });
        if let Some(m) = merge {
            prm["merge"] = json!(m);
        }
        run(app, &ctx, "koda.color.setCurve", prm);
    };
    let from_screen = |pos: egui::Pos2| [((pos.x - ed.min.x) / ed.width()).clamp(0.0, 1.0), ((ed.max.y - pos.y) / ed.height()).clamp(-0.5, 1.5)];
    if resp.drag_started()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let k = match hit(pos) {
            Some(k) => Some(k),
            None => {
                // a new point where the drag starts
                let q = from_screen(pos);
                let mut v = cur.0.clone();
                v.push(q);
                v.sort_by(|a, b| a[0].total_cmp(&b[0]));
                let k = v.iter().position(|p| *p == q);
                merge_key(st, "curve", true);
                let key = merge_key(st, "curve", false);
                send(app, v, Some(&key));
                k
            }
        };
        if hit(pos).is_some() {
            merge_key(st, "curve", true);
        }
        ctx.data_mut(|d| d.insert_temp(drag_id, k));
    }
    if resp.dragged()
        && let (Some(Some(k)), Some(pos)) = (ctx.data(|d| d.get_temp::<Option<usize>>(drag_id)), resp.interact_pointer_pos())
    {
        let mut v = cur.0.clone();
        let n = v.len();
        let mut q = from_screen(pos);
        // the ends stay at x = 0 and x = 1; the others between their neighbours
        if k == 0 {
            q[0] = 0.0;
        } else if k + 1 == n {
            q[0] = 1.0;
        } else {
            let lo = v.get(k - 1).map_or(0.0, |p| p[0] + 0.01);
            let hi = v.get(k + 1).map_or(1.0, |p| p[0] - 0.01);
            q[0] = q[0].clamp(lo, hi.max(lo));
        }
        if let Some(p) = v.get_mut(k) {
            *p = q;
        }
        let key = merge_key(st, "curve", false);
        send(app, v, Some(&key));
    }
    if resp.drag_stopped() {
        ctx.data_mut(|d| d.remove::<Option<usize>>(drag_id));
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && hit(pos).is_none()
    {
        let mut v = cur.0.clone();
        let x = from_screen(pos)[0];
        v.push([x, cur.eval(x)]);
        send(app, v, None);
    }
    if resp.secondary_clicked()
        && let Some(pos) = resp.interact_pointer_pos()
        && let Some(k) = hit(pos)
        && k > 0
        && k + 1 < cur.0.len()
    {
        let mut v = cur.0.clone();
        v.remove(k);
        send(app, v, None);
    }
    let _ = CurvePoints::default;
}

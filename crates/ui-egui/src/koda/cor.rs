//! KODA Pro aba Cor: visual preview of the layout (`docs/koda/cor/layout-aba-cor.png`), drawn as
//! empty panels with their titles. Nothing here works yet; the panels are filled in milestone 5
//! of `docs/koda/cor/ESPEC_COR_V1.md`. The viewer takes the full height on the right (vertical
//! video); LUTs, the node editor and the effects library sit on top at the left, then the clip
//! strip, the palette bar, the primaries (bars mode) and the scopes.

use egui::{Align2, Color32, Rect, Stroke, StrokeKind, pos2, vec2};

use crate::FilmcraftApp;
use crate::theme::Tokens;

/// KODA green: the accent of the aba Cor (selected node, active controls).
pub const KODA_GREEN: Color32 = Color32::from_rgb(0x2E, 0xD5, 0x73);

const GAP: f32 = 8.0;

/// One panel: background, border and title (and an optional subtitle). Returns the content rect.
fn panel(app: &mut FilmcraftApp, ui: &egui::Ui, r: Rect, id: &str, title: &str, sub: &str) -> Rect {
    let t = app.tokens;
    let p = ui.painter();
    p.rect_filled(r, 6.0, t.panel_bg);
    p.rect_stroke(r, 6.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
    p.text(r.min + vec2(12.0, 16.0), Align2::LEFT_CENTER, title, Tokens::semibold(13.0), t.text);
    if !sub.is_empty() {
        p.text(r.min + vec2(12.0, 33.0), Align2::LEFT_CENTER, sub, Tokens::ui(11.0), t.text_dim);
    }
    app.auto.add(&format!("cor.{id}"), r, title);
    Rect::from_min_max(r.min + vec2(12.0, if sub.is_empty() { 30.0 } else { 46.0 }), r.max - vec2(12.0, 10.0))
}

/// Placeholder rows (list items).
fn rows(ui: &egui::Ui, t: &Tokens, c: Rect, labels: &[&str]) {
    let p = ui.painter();
    for (i, l) in labels.iter().enumerate() {
        let y = c.min.y + i as f32 * 28.0;
        if y + 22.0 > c.max.y {
            break;
        }
        let r = Rect::from_min_size(pos2(c.min.x, y), vec2(c.width(), 22.0));
        p.rect_filled(r, 4.0, t.field_bg);
        p.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, *l, Tokens::ui(11.5), t.text_dim);
    }
}

/// A dashed rectangle outline (a disabled node).
fn dashed(ui: &egui::Ui, r: Rect, col: Color32) {
    let s = Stroke::new(1.2, col);
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for w in pts.windows(2) {
        ui.painter().add(egui::Shape::dashed_line(&[w[0], w[1]], s, 4.0, 3.0));
    }
}

/// The node editor: the KODA powergrade's six serial nodes, 06 (spare LUT) disabled.
fn nodes(ui: &egui::Ui, t: &Tokens, c: Rect) {
    let names = ["global", "CST", "nitidez", "glow", "LUT", "LUT"];
    let n = names.len() as f32;
    let (w, h) = (64.0_f32, 40.0_f32);
    let dx = ((c.width() - w) / (n - 1.0)).clamp(0.0, 90.0);
    let dy = ((c.height() - h - 14.0) / (n - 1.0)).clamp(0.0, 46.0);
    let p = ui.painter();
    let mut prev: Option<Rect> = None;
    for (i, name) in names.iter().enumerate() {
        let r = Rect::from_min_size(c.min + vec2(i as f32 * dx, i as f32 * dy), vec2(w, h));
        if let Some(pr) = prev {
            // serial link: out of the previous node's right edge into this node's left edge
            let a = pr.right_center();
            let b = r.left_center();
            let mid = pos2(a.x + 8.0, a.y);
            p.line_segment([a, mid], Stroke::new(1.2, t.text_faint));
            p.line_segment([mid, pos2(mid.x, b.y)], Stroke::new(1.2, t.text_faint));
            p.line_segment([pos2(mid.x, b.y), b], Stroke::new(1.2, t.text_faint));
        }
        let disabled = i == 5;
        let selected = i == 0;
        if disabled {
            dashed(ui, r, t.text_faint);
        } else {
            p.rect_filled(r, 5.0, t.field_bg);
            p.rect_stroke(r, 5.0, Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { KODA_GREEN } else { t.field_border }), StrokeKind::Inside);
        }
        let col = if disabled { t.text_faint } else { t.text };
        p.text(r.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, format!("{:02}", i + 1), Tokens::semibold(12.0), col);
        p.text(r.center() + vec2(0.0, 9.0), Align2::CENTER_CENTER, *name, Tokens::ui(10.0), t.text_dim);
        prev = Some(r);
    }
}

/// Lift / Gamma / Gain / Offset in bars mode (Y R G B bars each), and the primary adjustments.
fn primaries(ui: &egui::Ui, t: &Tokens, c: Rect) {
    let p = ui.painter();
    // DaVinci's names, untranslated (colourists know them by these)
    let groups = ["Lift", "Gamma", "Gain", "Offset"];
    let gw = c.width() / 4.0;
    let bar_h = (c.height() - 46.0).clamp(20.0, 110.0);
    let colors = [t.text, Color32::from_rgb(0xE5, 0x5B, 0x5B), Color32::from_rgb(0x4C, 0xC7, 0x6B), Color32::from_rgb(0x5B, 0x8D, 0xEF)];
    for (g, label) in groups.iter().enumerate() {
        let x0 = c.min.x + g as f32 * gw + gw / 2.0 - 34.0;
        for (b, col) in colors.iter().enumerate() {
            let r = Rect::from_min_size(pos2(x0 + b as f32 * 18.0, c.min.y), vec2(12.0, bar_h));
            p.rect_stroke(r, 3.0, Stroke::new(1.0, t.field_border), StrokeKind::Inside);
            // neutral position: the handle in the middle
            let y = r.center().y;
            p.line_segment([pos2(r.min.x + 2.0, y), pos2(r.max.x - 2.0, y)], Stroke::new(2.0, *col));
        }
        p.text(pos2(c.min.x + g as f32 * gw + gw / 2.0, c.min.y + bar_h + 14.0), Align2::CENTER_CENTER, *label, Tokens::ui(12.0), t.text_dim);
    }
    p.text(
        pos2(c.center().x, c.max.y - 6.0),
        Align2::CENTER_CENTER,
        "Temp · Tint · Cont · Pivot · Mid Detail · Sat · Hue · Lum Mix",
        Tokens::ui(11.0),
        t.text_faint,
    );
}

/// Scopes: Parade (default), Waveform, Vectorscope placeholders.
fn scopes(ui: &egui::Ui, t: &Tokens, c: Rect) {
    let p = ui.painter();
    let labels = ["Parade RGB", "Waveform", "Vetorscópio"];
    let w = (c.width() - 2.0 * GAP) / 3.0;
    for (i, l) in labels.iter().enumerate() {
        let r = Rect::from_min_size(pos2(c.min.x + i as f32 * (w + GAP), c.min.y), vec2(w, (c.height() - 4.0).max(10.0)));
        p.rect_filled(r, 4.0, t.plot_bg);
        p.rect_stroke(r, 4.0, Stroke::new(1.0, if i == 0 { KODA_GREEN } else { t.separator }), StrokeKind::Inside);
        // 10-bit scale guides (0, 256, 512, 768, 1023)
        for k in 1..4 {
            let y = r.max.y - r.height() * k as f32 / 4.0;
            p.line_segment([pos2(r.min.x + 4.0, y), pos2(r.max.x - 4.0, y)], Stroke::new(0.5, t.plot_grid));
        }
        p.text(r.left_top() + vec2(6.0, 9.0), Align2::LEFT_CENTER, *l, Tokens::ui(10.0), t.text_dim);
    }
}

pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    ui.painter().rect_filled(rect, 0.0, t.app_bg);
    let area = rect.shrink(GAP);
    // Right: the viewer, full height, sized for vertical (9:16) video.
    let viewer_w = (area.height() * 9.0 / 16.0 + 40.0).clamp(240.0, area.width() * 0.42);
    let viewer = Rect::from_min_max(pos2(area.max.x - viewer_w, area.min.y), area.max);
    let left = Rect::from_min_max(area.min, pos2(viewer.min.x - GAP, area.max.y));

    // Left column, top to bottom: LUTs / nodes / effects, clips, palettes, primaries / scopes.
    let top_h = (left.height() * 0.44).max(120.0);
    let top = Rect::from_min_size(left.min, vec2(left.width(), top_h));
    let luts_w = top.width() * 0.2;
    let fx_w = top.width() * 0.26;
    let luts = Rect::from_min_size(top.min, vec2(luts_w, top.height()));
    let fx = Rect::from_min_max(pos2(top.max.x - fx_w, top.min.y), top.max);
    let node_r = Rect::from_min_max(pos2(luts.max.x + GAP, top.min.y), pos2(fx.min.x - GAP, top.max.y));
    let clips = Rect::from_min_size(pos2(left.min.x, top.max.y + GAP), vec2(left.width(), 64.0));
    let pal = Rect::from_min_size(pos2(left.min.x, clips.max.y + GAP), vec2(left.width(), 34.0));
    let bottom = Rect::from_min_max(pos2(left.min.x, pal.max.y + GAP), left.max);
    let prim = Rect::from_min_size(bottom.min, vec2((bottom.width() - GAP) / 2.0, bottom.height()));
    let scope_r = Rect::from_min_max(pos2(prim.max.x + GAP, bottom.min.y), bottom.max);

    let c = panel(app, ui, luts, "luts", tl!("LUTs"), tl!("Folder browser"));
    rows(ui, &t, c, &["Orson.cube", "Kodak 2383", "Fuji 3513", "Sony S-Log3", "Minhas LUTs", "Clientes"]);

    let c = panel(app, ui, node_r, "nodes", tl!("Node editor"), tl!("Serial · names · enable/disable"));
    nodes(ui, &t, c);

    let c = panel(app, ui, fx, "effects", tl!("Effects"), tl!("Drag onto a node"));
    rows(ui, &t, c, &["Desfoque", "Nitidez", "Glow", "CST", "LUT", "Vinheta"]);

    // Clip strip: thumbnails with codec and timecode.
    let c = panel(app, ui, clips, "clips", tl!("Clips"), "");
    let p = ui.painter();
    for i in 0..6 {
        let r = Rect::from_min_size(pos2(c.min.x + 90.0 + i as f32 * 128.0, clips.min.y + 8.0), vec2(120.0, 48.0));
        if r.max.x > c.max.x {
            break;
        }
        p.rect_filled(r, 4.0, t.field_bg);
        p.rect_stroke(r, 4.0, Stroke::new(if i == 0 { 1.6 } else { 1.0 }, if i == 0 { KODA_GREEN } else { t.field_border }), StrokeKind::Inside);
        p.text(r.left_top() + vec2(6.0, 10.0), Align2::LEFT_CENTER, format!("{:02}", i + 1), Tokens::semibold(10.0), t.text_dim);
        p.text(r.left_bottom() + vec2(6.0, -16.0), Align2::LEFT_CENTER, "H.264 4:2:2 10b", Tokens::ui(9.0), t.text_faint);
        p.text(r.left_bottom() + vec2(6.0, -6.0), Align2::LEFT_CENTER, format!("00:00:{:02}:00", i * 8), Tokens::ui(9.0), t.text_faint);
    }

    // Palette bar: the panels' toggles.
    let c = panel(app, ui, pal, "palettes", "", "");
    let p = ui.painter();
    p.text(pal.left_center() + vec2(12.0, 0.0), Align2::LEFT_CENTER, tl!("Palettes"), Tokens::ui(12.0), t.text_dim);
    for i in 0..11 {
        let ctr = pos2(c.max.x - 10.0 - i as f32 * 30.0, pal.center().y);
        p.circle_stroke(ctr, 8.0, Stroke::new(1.0, if i == 10 { KODA_GREEN } else { t.text_faint }));
    }

    let c = panel(app, ui, prim, "primaries", tl!("Primaries (bars)"), "");
    primaries(ui, &t, c);

    let c = panel(app, ui, scope_r, "scopes", tl!("Scopes"), "");
    scopes(ui, &t, c);

    // Viewer: the 9:16 picture, then before/after, split screen and transport.
    let c = panel(app, ui, viewer, "viewer", tl!("Viewer"), tl!("Vertical video"));
    let p = ui.painter();
    let ph = (c.height() - 70.0).max(40.0);
    let pw = (ph * 9.0 / 16.0).min(c.width());
    let pic = Rect::from_center_size(pos2(c.center().x, c.min.y + ph / 2.0), vec2(pw, ph));
    p.rect_filled(pic, 2.0, Color32::BLACK);
    p.rect_stroke(pic, 2.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
    p.text(pic.center(), Align2::CENTER_CENTER, "9:16", Tokens::ui(12.0), t.text_faint);
    let y = pic.max.y + 22.0;
    p.line_segment([pos2(pic.min.x, y), pos2(pic.max.x, y)], Stroke::new(1.0, t.text_faint));
    p.circle_filled(pos2(pic.min.x + pic.width() * 0.3, y), 5.0, KODA_GREEN);
    p.text(pos2(c.center().x, y + 22.0), Align2::CENTER_CENTER, tl!("Before/after · split · transport"), Tokens::ui(11.0), t.text_dim);
    p.text(pos2(c.center().x, c.max.y - 4.0), Align2::CENTER_CENTER, tl!("Preview: the Color tab is not working yet"), Tokens::ui(10.0), t.text_faint);
}

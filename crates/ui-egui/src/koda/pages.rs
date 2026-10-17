//! KODA Pro page bar: the app's pages switched from a bar along the bottom of the window, as in
//! DaVinci Resolve (`docs/koda/cor/LAYOUT_PAGINA_COR.md`, "Barra de páginas"). It replaces
//! FilmCraft's mode tabs in the header on every page: the app's name on the left, the pages in the
//! middle (Edit, Color, Motion, Export; the current one in KODA green with an underline), Home
//! and Settings on the right. Shortcuts as in DaVinci: Shift+4 Edit, Shift+5 Motion, Shift+6
//! Color, Shift+8 Export (`menus::UI_COMMANDS`). Motion has no page yet and shows disabled.
//!
//! Icons are drawn here, in KODA's thin-line style; none comes from another application.

use egui::{Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, pos2, vec2};

use crate::FilmcraftApp;
use crate::state::Mode;
use crate::theme::Tokens;

/// The bar's height.
pub const HEIGHT: f32 = 36.0;

/// Whether the pages are switched from this bar (the header then leaves its mode tabs out).
pub const IN_FOOTER: bool = true;

/// The bar's pages: (mode, command, label, automation id). `None` mode: not built yet.
const PAGES: [(Option<Mode>, &str, &str, &str); 4] = [
    (Some(Mode::Edit), "mode.edit", "Edit", "edit"),
    (Some(Mode::Color), "mode.color", "Color", "color"),
    (None, "", "Motion", "motion"),
    (Some(Mode::Export), "mode.export", "Export", "export"),
];

#[derive(Clone, Copy)]
enum Glyph {
    Edit,
    Color,
    Motion,
    Export,
}

/// A page glyph in a `r`-sized square: thin strokes, one colour.
fn glyph(p: &egui::Painter, r: Rect, g: Glyph, col: Color32) {
    let s = Stroke::new(1.4, col);
    let at = |x: f32, y: f32| -> Pos2 { pos2(r.min.x + x * r.width(), r.min.y + y * r.height()) };
    match g {
        // three clips on a timeline, staggered
        Glyph::Edit => {
            for (y, x0, x1) in [(0.28, 0.10, 0.62), (0.50, 0.30, 0.90), (0.72, 0.10, 0.48)] {
                p.rect_stroke(Rect::from_min_max(at(x0, y - 0.08), at(x1, y + 0.08)), 1.5, s, StrokeKind::Middle);
            }
        }
        // three overlapping circles (the additive primaries)
        Glyph::Color => {
            let rad = r.width() * 0.24;
            for (x, y) in [(0.50, 0.34), (0.34, 0.62), (0.66, 0.62)] {
                p.circle_stroke(at(x, y), rad, s);
            }
        }
        // a motion path ending in a keyframe diamond
        Glyph::Motion => {
            let pts: Vec<Pos2> = (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    at(0.10 + 0.62 * t, 0.78 - 0.55 * t + 0.18 * (t * std::f32::consts::PI).sin())
                })
                .collect();
            p.add(Shape::line(pts, s));
            let c = at(0.78, 0.26);
            let d = r.width() * 0.11;
            p.add(Shape::closed_line(vec![c + vec2(0.0, -d), c + vec2(d, 0.0), c + vec2(0.0, d), c + vec2(-d, 0.0)], s));
        }
        // an arrow leaving a tray
        Glyph::Export => {
            p.add(Shape::line(vec![at(0.18, 0.56), at(0.18, 0.84), at(0.82, 0.84), at(0.82, 0.56)], s));
            p.line_segment([at(0.50, 0.16), at(0.50, 0.64)], s);
            p.add(Shape::line(vec![at(0.32, 0.34), at(0.50, 0.16), at(0.68, 0.34)], s));
        }
    }
}

/// Draw the bar in `rect` and switch pages from it.
pub fn show(app: &mut FilmcraftApp, ui: &mut egui::Ui, rect: Rect) {
    let t = app.tokens;
    let green = super::cor::KODA_GREEN;
    ui.painter().rect_filled(rect, 0.0, t.header_bg);
    ui.painter().line_segment([rect.left_top(), rect.right_top()], Stroke::new(1.0, t.separator));
    ui.painter().text(rect.left_center() + vec2(14.0, 0.0), Align2::LEFT_CENTER, "Koda Pro", Tokens::semibold(13.0), t.text);

    // pages, centred
    let (iw, gap) = (44.0, 28.0);
    let total = PAGES.len() as f32 * iw + (PAGES.len() - 1) as f32 * gap;
    let mut x = rect.center().x - total / 2.0;
    let ctx = ui.ctx().clone();
    for (mode, cmd, label, id) in PAGES {
        let r = Rect::from_min_size(pos2(x, rect.min.y + 2.0), vec2(iw, rect.height() - 4.0));
        x += iw + gap;
        let enabled = mode.is_some();
        let current = mode.is_some_and(|m| app.ui.mode == m);
        let resp = ui.interact(r, egui::Id::new(("koda-page", id)), if enabled { Sense::click() } else { Sense::hover() });
        // "Motion" is the page's name (like DaVinci's Fusion), not the Motion effect's translation
        let shown = if id == "motion" { "Motion" } else { crate::i18n::t(label) };
        let resp = resp.on_hover_text(if enabled { shown.to_string() } else { format!("{shown} — {}", tl!("not available yet")) });
        app.auto.add(&format!("pages.{id}"), r, label);
        // FilmCraft's ids for the same switches (its automation and tests keep working)
        if enabled {
            app.auto.add(&format!("header.mode.{id}"), r, label);
        }
        let col = if current {
            green
        } else if !enabled {
            t.text_faint
        } else if resp.hovered() {
            t.text
        } else {
            t.text_dim
        };
        let icon = Rect::from_center_size(pos2(r.center().x, r.min.y + 13.0), vec2(18.0, 18.0));
        glyph(ui.painter(), icon, [Glyph::Edit, Glyph::Color, Glyph::Motion, Glyph::Export][PAGES.iter().position(|p| p.3 == id).unwrap_or(0)], col);
        ui.painter().text(pos2(r.center().x, r.max.y - 7.0), Align2::CENTER_CENTER, shown, Tokens::ui(10.0), col);
        if current {
            ui.painter().line_segment([pos2(r.min.x + 8.0, r.max.y), pos2(r.max.x - 8.0, r.max.y)], Stroke::new(2.0, green));
        }
        if enabled && resp.clicked() && !cmd.is_empty() {
            let _ = crate::menus::invoke(app, &ctx, cmd, serde_json::json!({}));
        }
    }

    // Home and Settings on the right
    let mut rx = rect.max.x - 14.0;
    for (icon, id, legacy, cmd, tip) in [
        (crate::icons::Icon::Gear, "pages.settings", "", "app.settings.general", tl!("Settings")),
        (crate::icons::Icon::Home, "pages.home", "header.home", "mode.import", tl!("Home")),
    ] {
        let r = Rect::from_center_size(pos2(rx - 13.0, rect.center().y), vec2(26.0, 26.0));
        rx -= 36.0;
        let resp = ui.interact(r, egui::Id::new(("koda-page-btn", id)), Sense::click()).on_hover_text(tip);
        app.auto.add(id, r, tip);
        if !legacy.is_empty() {
            app.auto.add(legacy, r, tip);
        }
        let current = id == "pages.home" && app.ui.mode == Mode::Import;
        if resp.hovered() {
            ui.painter().rect_filled(r, 4.0, t.hover);
        }
        crate::icons::paint(
            ui.painter(),
            r.shrink(5.0),
            icon,
            if current {
                green
            } else if resp.hovered() {
                t.text
            } else {
                t.text_dim
            },
        );
        if resp.clicked() {
            let _ = crate::menus::invoke(app, &ctx, cmd, serde_json::json!({}));
        }
    }
}

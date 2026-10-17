//! KODA Pro página Cor (aba Cor marco 5): the page drives the grade through `koda.color.*`.
//! Adding nodes from the effects library, selecting nodes, the primaries fields and bars (one
//! undo step per drag), the palettes, the clip strip and before / after.

use std::sync::mpsc::{Sender, channel};

use egui_kittest::Harness;
use filmcraft_engine::Session;
use filmcraft_ui_egui::FilmcraftApp;
use filmcraft_ui_egui::control::ControlRequest;
use serde_json::{Value, json};

struct Driver {
    harness: Harness<'static, FilmcraftApp>,
    tx: Sender<ControlRequest>,
}

impl Driver {
    fn demo() -> Self {
        koda_color::register();
        let mut session = Session::default();
        session.execute("file.openDemoProject", json!({})).expect("demo project");
        let (tx, rx) = channel();
        let app = FilmcraftApp::new(session).with_control(rx);
        let b = Harness::builder().with_size(egui::vec2(1915.0, 1049.0)).with_max_steps(10_000);
        let harness = b.build_eframe(move |_cc| app);
        let mut d = Driver { harness, tx };
        d.frames(4);
        d
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            let ctx = self.harness.ctx.clone();
            let mut raw = std::mem::take(self.harness.input_mut());
            eframe::App::raw_input_hook(self.harness.state_mut(), &ctx, &mut raw);
            *self.harness.input_mut() = raw;
            self.harness.step();
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let (req, reply) = ControlRequest::new(method, params.clone());
        self.tx.send(req).unwrap();
        for _ in 0..600 {
            self.frames(1);
            if let Ok(v) = reply.try_recv() {
                return v;
            }
        }
        panic!("no reply to {method} {params}");
    }

    fn ok(&mut self, method: &str, params: Value) -> Value {
        let v = self.call(method, params.clone());
        assert_eq!(v["ok"], json!(true), "{method} {params} failed: {v}");
        v["result"].clone()
    }

    fn exec(&mut self, command: &str, params: Value) -> Value {
        self.ok("engine.execute", json!({"command": command, "params": params}))
    }

    fn click(&mut self, id: &str) {
        self.ok("ui.click", json!({"id": id}));
        self.frames(2);
    }

    fn element(&mut self, id: &str) -> Option<[f32; 4]> {
        let v = self.ok("ui.elements", json!({"prefix": id}));
        v.as_array()?.iter().find(|e| e["id"] == id).map(|e| {
            let r = &e["rect"];
            [r[0].as_f64().unwrap() as f32, r[1].as_f64().unwrap() as f32, r[2].as_f64().unwrap() as f32, r[3].as_f64().unwrap() as f32]
        })
    }

    /// Press on `id`, move by (`dx`, `dy`) over `steps` frames, release.
    fn drag(&mut self, id: &str, dx: f32, dy: f32, steps: usize) {
        let r = self.element(id).unwrap_or_else(|| panic!("no element {id}"));
        let mut p = egui::pos2(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        let push = |d: &mut Self, e: egui::Event| d.harness.input_mut().events.push(e);
        push(self, egui::Event::PointerMoved(p));
        self.frames(1);
        push(self, egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
        self.frames(1);
        for _ in 0..steps {
            p.x += dx / steps as f32;
            p.y += dy / steps as f32;
            push(self, egui::Event::PointerMoved(p));
            self.frames(1);
        }
        push(self, egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
        self.frames(2);
    }
}

impl Driver {
    fn grade(&mut self) -> Value {
        self.exec("koda.color.inspect", json!({}))["grade"].clone()
    }
}

fn open() -> Driver {
    let mut d = Driver::demo();
    d.click("pages.color");
    d.frames(4);
    // the first clip of the strip becomes the selected one
    d.click("cor.clip.1");
    d.frames(2);
    d
}

#[test]
fn the_page_lays_out_like_the_reference() {
    let mut d = open();
    for id in [
        "cor.top.gallery",
        "cor.top.luts",
        "cor.top.nodes",
        "cor.top.effects",
        "cor.nodes.add",
        "cor.fx.primaries",
        "cor.clip.1",
        "cor.palette.primaries",
        "cor.palette.curves",
        "cor.scope.parade",
        "cor.viewer.bypass",
        "cor.prim.temp",
        "cor.prim.lift.y",
        "cor.prim.offset.b",
    ] {
        assert!(d.element(id).is_some(), "{id} is drawn");
    }
    // the viewer on the right from 71.3 %, the primaries bottom-left
    let v = d.element("cor.viewer.bypass").unwrap();
    assert!(v[0] > 1915.0 * 0.70, "{v:?}");
    let t = d.element("cor.prim.temp").unwrap();
    assert!(t[0] < 1915.0 * 0.355 && t[1] > 1049.0 * 0.6, "{t:?}");
    assert!(d.harness.state().ui_error.is_none());
}

#[test]
fn effects_library_adds_nodes_and_the_palette_edits_them() {
    let mut d = open();
    assert!(d.grade()["nodes"].as_array().unwrap().is_empty());
    d.click("cor.fx.primaries");
    d.click("cor.fx.glow");
    let g = d.grade();
    let nodes = g["nodes"].as_array().unwrap().clone();
    assert_eq!(nodes.iter().map(|n| n["type"].as_str().unwrap()).collect::<Vec<_>>(), ["primaries", "glow"]);
    // the glow is selected now: the primaries palette offers to add a node
    assert!(d.element("cor.prim.add").is_some());
    let prim = nodes[0]["id"].as_u64().unwrap();
    d.click(&format!("cor.node.{prim}"));
    d.frames(2);
    assert!(d.element("cor.prim.add").is_none(), "the primaries node is selected");
    // dragging Temp: one undo step for the whole drag
    let before = d.exec("history.list", json!({}));
    d.drag("cor.prim.temp", -60.0, 0.0, 6);
    let temp = d.grade()["nodes"][0]["temp"].as_f64().unwrap();
    assert!(temp < -10.0, "temp {temp}");
    let after = d.exec("history.list", json!({}));
    let n = |h: &Value| h["undo"].as_array().map_or(0, |a| a.len());
    assert_eq!(n(&after), n(&before) + 1, "one drag, one undo step: {after}");
    // a bar: Gain red up
    d.drag("cor.prim.bar.gain.r", 0.0, -30.0, 5);
    let gr = d.grade()["nodes"][0]["gain"][1].as_f64().unwrap();
    assert!(gr > 1.05, "gain r {gr}");
    // reset the group
    d.click("cor.prim.reset.gain");
    assert_eq!(d.grade()["nodes"][0]["gain"][1].as_f64().unwrap(), 1.0);
    // wheels mode
    d.click("cor.prim.mode.wheels");
    assert_eq!(d.grade()["nodes"][0]["mode"], "wheels");
    assert!(d.element("cor.prim.wheel.lift").is_some());
    d.drag("cor.prim.wheel.gamma", 20.0, 0.0, 4);
    let gamma = d.grade()["nodes"][0]["gamma"].clone();
    assert!(gamma.as_array().unwrap().iter().skip(1).any(|v| v.as_f64().unwrap().abs() > 0.01), "{gamma}");
    // curves palette
    d.click("cor.palette.curves");
    assert!(d.element("cor.curve.editor").is_some());
    d.click("cor.curve.editor");
    let pts = d.grade()["nodes"][0]["curves"]["y"].as_array().unwrap().len();
    assert_eq!(pts, 3, "a click adds a point");
    // before / after
    d.click("cor.viewer.bypass");
    assert_eq!(d.grade()["bypass"], true);
    assert!(d.harness.state().ui_error.is_none());
}

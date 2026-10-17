//! The aba Cor commands (`koda.color.*`): every change to a grade goes through one of these, so
//! the UI, the CLI, the control channel and MCP share one path, and each edit is one undo step.
//!
//! Target clip: `clip` (a timeline clip id in the active sequence), else the one selected clip.
//! It must be a media clip (a video file or subclip). Nodes are named by `node` (their stable id).

use filmcraft_engine::{CommandSpec, EngineError, Result, Session};
use filmcraft_project::{ClipId, ItemKind};
use serde_json::{Value, json};

use crate::grade::{Grade, NodeKind};
use crate::store;

type Run = fn(&mut Session, &Value) -> Result<Value>;

fn has_sequence(s: &Session) -> std::result::Result<(), String> {
    if s.active_sequence().is_some() { Ok(()) } else { Err("no sequence is open".into()) }
}

fn always(_: &Session) -> std::result::Result<(), String> {
    Ok(())
}

fn cmd(id: &'static str, label: &'static str, params: &'static str, run: Run) -> CommandSpec {
    CommandSpec { id, label, menu: &[], shortcut: None, params, enabled: has_sequence, run, journal: true }
}

/// The `koda.color.*` commands.
pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "koda.color.inspect",
            label: "Inspect Color Grade",
            menu: &[],
            shortcut: None,
            params: r#"{"clip":clipId?}"#,
            enabled: always,
            run: inspect,
            journal: false,
        },
        cmd(
            "koda.color.addNode",
            "Add Color Node",
            r#"{"clip":clipId?,"type":"gain"|"cst"|"lut"|"primaries"|"sharpen"|"glow","name":str?,"index":int?}"#,
            add_node,
        ),
        cmd("koda.color.removeNode", "Delete Color Node", r#"{"clip":clipId?,"node":nodeId}"#, remove_node),
        cmd("koda.color.moveNode", "Move Color Node", r#"{"clip":clipId?,"node":nodeId,"index":int}"#, move_node),
        cmd("koda.color.renameNode", "Rename Color Node", r#"{"clip":clipId?,"node":nodeId,"name":str}"#, rename_node),
        cmd("koda.color.setNodeEnabled", "Enable/Disable Color Node", r#"{"clip":clipId?,"node":nodeId,"enabled":bool?}"#, set_node_enabled),
        cmd("koda.color.setParam", "Set Color Node Parameter", r#"{"clip":clipId?,"node":nodeId,"param":str,"value":number|[number]|str}"#, set_param),
        cmd("koda.color.setBypass", "Bypass Color Grade", r#"{"clip":clipId?,"bypass":bool?}"#, set_bypass),
        cmd("koda.color.setGrade", "Set Color Grade", r#"{"clip":clipId?,"grade":object}"#, set_grade_cmd),
        cmd("koda.color.setCurve", "Set Color Curve", r#"{"clip":clipId?,"node":nodeId,"channel":"y"|"r"|"g"|"b","points":[[x,y]]}"#, set_curve),
        cmd(
            "koda.color.resetParams",
            "Reset Color Node",
            r#"{"clip":clipId?,"node":nodeId,"what":"all"|"lift"|"gamma"|"gain"|"offset"|"curves"|"curve.y"?}"#,
            reset_params,
        ),
        cmd("koda.color.loadLut", "Load LUT into Color Node", r#"{"clip":clipId?,"node":nodeId,"path":str}"#, load_lut),
    ]
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.to_string(), msg: msg.into() }
}

/// The target clip: `clip`, else the single selected clip; it must be a media clip in the active
/// sequence.
fn target(s: &Session, cmd: &str, p: &Value) -> Result<ClipId> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let id = match p.get("clip") {
        Some(v) => serde_json::from_value::<ClipId>(v.clone()).map_err(|_| bad(cmd, "clip: expected a clip id"))?,
        None => match s.state.selection.as_slice() {
            [one] => *one,
            [] => return Err(EngineError::Disabled(cmd.to_string(), "select a clip, or name it with \"clip\"".into())),
            _ => return Err(EngineError::Disabled(cmd.to_string(), "select only one clip, or name it with \"clip\"".into())),
        },
    };
    let (_, item) = seq.find_item(id).ok_or_else(|| bad(cmd, format!("clip {id:?} is not in the open sequence")))?;
    let is_media = s.project.item(item.item).is_some_and(|i| matches!(i.kind, ItemKind::Media(_) | ItemKind::Subclip { .. }));
    if !is_media {
        return Err(bad(cmd, "only media clips (video files) have a color grade"));
    }
    Ok(id)
}

fn node_id(cmd: &str, p: &Value) -> Result<u32> {
    p.get("node").and_then(Value::as_u64).and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad(cmd, "node: expected a node id"))
}

/// Change the target clip's grade with `f`, as one undo step; returns `f`'s value and the grade.
fn edit_grade(s: &mut Session, cmd: &'static str, label: &str, p: &Value, f: impl FnOnce(&mut Grade) -> std::result::Result<Value, String>) -> Result<Value> {
    let id = target(s, cmd, p)?;
    s.edit_sequence(label, move |seq, _, _| {
        let (_, item) = seq.find_item_mut(id).ok_or_else(|| EngineError::Other("the clip went away".into()))?;
        let mut g = store::grade_of(item).map_err(|e| bad(cmd, e))?;
        let out = f(&mut g).map_err(|e| bad(cmd, e))?;
        store::set_grade(item, &g);
        Ok(json!({ "result": out, "grade": g }))
    })
}

fn inspect(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.inspect";
    let id = target(s, CMD, p)?;
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let (_, item) = seq.find_item(id).ok_or_else(|| bad(CMD, "clip not found"))?;
    let g = store::grade_of(item).map_err(|e| bad(CMD, e))?;
    Ok(json!({ "clip": id, "grade": g }))
}

fn add_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.addNode";
    let t = p.get("type").and_then(Value::as_str).unwrap_or("gain");
    let kind = NodeKind::from_type(t).ok_or_else(|| bad(CMD, format!("unknown node type \"{t}\" (gain, cst, lut, primaries, sharpen, glow)")))?;
    let name = match p.get("name") {
        None => None,
        Some(v) => Some(v.as_str().ok_or_else(|| bad(CMD, "name: expected text"))?.to_string()),
    };
    let index = match p.get("index") {
        None => None,
        Some(v) => Some(v.as_u64().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| bad(CMD, "index: expected a position from 0"))?),
    };
    edit_grade(s, CMD, "Add Color Node", p, move |g| g.add(kind, name.as_deref(), index).map(|id| json!({ "node": id })))
}

fn remove_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.removeNode";
    let n = node_id(CMD, p)?;
    edit_grade(s, CMD, "Delete Color Node", p, move |g| g.remove(n).map(|_| json!({ "node": n })))
}

fn move_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.moveNode";
    let n = node_id(CMD, p)?;
    let to = p.get("index").and_then(Value::as_u64).and_then(|i| usize::try_from(i).ok()).ok_or_else(|| bad(CMD, "index: expected a position from 0"))?;
    edit_grade(s, CMD, "Move Color Node", p, move |g| g.move_to(n, to).map(|_| json!({ "node": n })))
}

fn rename_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.renameNode";
    let n = node_id(CMD, p)?;
    let name: String = p.get("name").and_then(Value::as_str).ok_or_else(|| bad(CMD, "name: expected text"))?.chars().take(crate::grade::MAX_NAME).collect();
    edit_grade(s, CMD, "Rename Color Node", p, move |g| {
        g.node_mut(n)?.name = name;
        Ok(json!({ "node": n }))
    })
}

fn set_node_enabled(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.setNodeEnabled";
    let n = node_id(CMD, p)?;
    let want = match p.get("enabled") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| bad(CMD, "enabled: expected true or false"))?),
    };
    edit_grade(s, CMD, "Enable/Disable Color Node", p, move |g| {
        let node = g.node_mut(n)?;
        node.enabled = want.unwrap_or(!node.enabled);
        Ok(json!({ "node": n, "enabled": node.enabled }))
    })
}

fn set_param(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.setParam";
    let n = node_id(CMD, p)?;
    let param = p.get("param").and_then(Value::as_str).ok_or_else(|| bad(CMD, "param: expected a parameter name"))?.to_string();
    let value = p.get("value").cloned().ok_or_else(|| bad(CMD, "value: missing"))?;
    edit_grade(s, CMD, "Set Color Node Parameter", p, move |g| {
        g.node_mut(n)?.kind.set_param(&param, &value)?;
        Ok(json!({ "node": n }))
    })
}

fn set_bypass(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.setBypass";
    let want = match p.get("bypass") {
        None => None,
        Some(v) => Some(v.as_bool().ok_or_else(|| bad(CMD, "bypass: expected true or false"))?),
    };
    edit_grade(s, CMD, "Bypass Color Grade", p, move |g| {
        g.bypass = want.unwrap_or(!g.bypass);
        Ok(json!({ "bypass": g.bypass }))
    })
}

fn set_grade_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.setGrade";
    let v = p.get("grade").ok_or_else(|| bad(CMD, "grade: missing"))?;
    let new = Grade::from_json(&v.to_string()).map_err(|e| bad(CMD, e))?;
    edit_grade(s, CMD, "Set Color Grade", p, move |g| {
        *g = new;
        Ok(json!({ "nodes": g.nodes.len() }))
    })
}

/// Import a `.cube` / `.3dl` file into the project's LUT library (`lut.import`, which reuses an
/// identical earlier import) and point a LUT node at it.
fn load_lut(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.loadLut";
    let n = node_id(CMD, p)?;
    let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad(CMD, "path: expected a .cube or .3dl file"))?.to_string();
    // the node must exist and be a LUT node before anything is imported
    let id = target(s, CMD, p)?;
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let (_, item) = seq.find_item(id).ok_or_else(|| bad(CMD, "clip not found"))?;
    let g = store::grade_of(item).map_err(|e| bad(CMD, e))?;
    match g.nodes.iter().find(|x| x.id == n).map(|x| &x.kind) {
        Some(NodeKind::Lut { .. }) => {}
        Some(_) => return Err(bad(CMD, format!("node {n} is not a LUT node"))),
        None => return Err(bad(CMD, format!("no node {n} in this grade"))),
    }
    let imported = s.execute("lut.import", json!({ "path": path }))?;
    let reference = imported.get("ref").and_then(Value::as_str).ok_or_else(|| bad(CMD, "the LUT library did not return a reference"))?.to_string();
    let r2 = reference.clone();
    edit_grade(s, CMD, "Load LUT into Color Node", p, move |g| {
        g.node_mut(n)?.kind.set_param("lut", &Value::String(r2))?;
        Ok(json!({ "node": n }))
    })?;
    Ok(json!({ "node": n, "lut": reference }))
}

/// The primaries node `n` of `g`, for changes.
fn primaries_mut(g: &mut Grade, n: u32) -> std::result::Result<&mut crate::primaries::Primaries, String> {
    match &mut g.node_mut(n)?.kind {
        NodeKind::Primaries(p) => Ok(p),
        _ => Err(format!("node {n} is not a primaries node")),
    }
}

/// Replace one custom curve's points (linked R / G / B move together).
fn set_curve(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.setCurve";
    let n = node_id(CMD, p)?;
    let ch = p.get("channel").and_then(Value::as_str).ok_or_else(|| bad(CMD, "channel: expected y, r, g or b"))?.to_string();
    let pts = p.get("points").and_then(Value::as_array).ok_or_else(|| bad(CMD, "points: expected [[x, y], …]"))?;
    if pts.len() > crate::primaries::range::CURVE_POINTS * 4 {
        return Err(bad(CMD, "points: too many"));
    }
    let mut v = Vec::with_capacity(pts.len());
    for q in pts {
        let xy = q.as_array().filter(|a| a.len() == 2).and_then(|a| Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32]));
        v.push(xy.ok_or_else(|| bad(CMD, "points: each point is [x, y] with numbers"))?);
    }
    edit_grade(s, CMD, "Set Color Curve", p, move |g| {
        let prim = primaries_mut(g, n)?;
        prim.curves.set(&ch, crate::primaries::CurvePoints(v))?;
        Ok(json!({ "node": n }))
    })
}

/// Reset a primaries node's controls (or one group, or a curve) to DaVinci's defaults.
fn reset_params(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.resetParams";
    let n = node_id(CMD, p)?;
    let what = match p.get("what") {
        None => "all".to_string(),
        Some(v) => v.as_str().ok_or_else(|| bad(CMD, "what: expected text"))?.to_string(),
    };
    edit_grade(s, CMD, "Reset Color Node", p, move |g| {
        primaries_mut(g, n)?.reset(&what)?;
        Ok(json!({ "node": n }))
    })
}

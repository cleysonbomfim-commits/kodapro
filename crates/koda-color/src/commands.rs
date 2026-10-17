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
        CommandSpec {
            id: "koda.color.copyGrade",
            label: "Copy Color Grade",
            menu: &[],
            shortcut: None,
            params: r#"{"clip":clipId?}"#,
            enabled: has_sequence,
            run: copy_grade,
            journal: false,
        },
        cmd("koda.color.pasteGrade", "Paste Color Grade", r#"{"clip":clipId?,"clips":[clipId]?,"mode":"replace"|"append"?}"#, paste_grade),
        cmd(
            "koda.color.applyGradeFrom",
            "Apply Grade from Clip",
            r#"{"from":clipId,"clip":clipId?,"clips":[clipId]?,"mode":"replace"|"append"?}"#,
            apply_grade_from,
        ),
        CommandSpec {
            id: "koda.color.copyNode",
            label: "Copy Color Node",
            menu: &[],
            shortcut: None,
            params: r#"{"clip":clipId?,"node":nodeId}"#,
            enabled: has_sequence,
            run: copy_node,
            journal: false,
        },
        cmd("koda.color.pasteNode", "Paste Color Node", r#"{"clip":clipId?,"index":int?,"replace":nodeId?}"#, paste_node),
        CommandSpec {
            id: "koda.color.savePowergrade",
            label: "Save Powergrade",
            menu: &[],
            shortcut: None,
            params: r#"{"clip":clipId?,"name":str,"path":str?}"#,
            enabled: has_sequence,
            run: save_powergrade,
            journal: false,
        },
        cmd("koda.color.applyPowergrade", "Apply Powergrade", r#"{"path":str,"clip":clipId?,"clips":[clipId]?,"mode":"replace"|"append"?}"#, apply_powergrade),
        CommandSpec {
            id: "koda.color.listPowergrades",
            label: "List Powergrades",
            menu: &[],
            shortcut: None,
            params: r#"{"dir":str?}"#,
            enabled: always,
            run: list_powergrades,
            journal: false,
        },
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

// ---- copy / paste and powergrades (marco 6) ----

/// Several target clips: `clips`, else `clip`, else every selected clip. Each must be a media clip
/// in the open sequence.
fn targets(s: &Session, cmd: &str, p: &Value) -> Result<Vec<ClipId>> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let ids: Vec<ClipId> = match (p.get("clips"), p.get("clip")) {
        (Some(v), _) => serde_json::from_value(v.clone()).map_err(|_| bad(cmd, "clips: expected a list of clip ids"))?,
        (None, Some(v)) => vec![serde_json::from_value(v.clone()).map_err(|_| bad(cmd, "clip: expected a clip id"))?],
        (None, None) => s.state.selection.clone(),
    };
    if ids.is_empty() {
        return Err(EngineError::Disabled(cmd.to_string(), "select clips, or name them with \"clips\"".into()));
    }
    if ids.len() > 10_000 {
        return Err(bad(cmd, "clips: too many"));
    }
    let mut out: Vec<ClipId> = Vec::with_capacity(ids.len());
    for id in ids {
        let (_, item) = seq.find_item(id).ok_or_else(|| bad(cmd, format!("clip {id:?} is not in the open sequence")))?;
        let is_media = s.project.item(item.item).is_some_and(|i| matches!(i.kind, ItemKind::Media(_) | ItemKind::Subclip { .. }));
        if !is_media {
            return Err(bad(cmd, format!("clip {id:?} is not a media clip (only video clips have a color grade)")));
        }
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// How a pasted grade meets the clip's own: replace it (DaVinci's default) or go after its nodes.
fn paste_mode(cmd: &str, p: &Value) -> Result<bool> {
    match p.get("mode").map(|v| v.as_str()) {
        None | Some(Some("replace")) => Ok(false),
        Some(Some("append")) => Ok(true),
        _ => Err(bad(cmd, "mode: \"replace\" or \"append\"")),
    }
}

/// The grade of clip `id` in the open sequence.
fn grade_at(s: &Session, cmd: &str, id: ClipId) -> Result<Grade> {
    let seq = s.active_sequence().ok_or(EngineError::NoSequence)?;
    let (_, item) = seq.find_item(id).ok_or_else(|| bad(cmd, "clip not found"))?;
    store::grade_of(item).map_err(|e| bad(cmd, e))
}

/// Put `pg` on every clip of `ids` as one undo step: its LUTs join the project's library, then
/// each clip's grade is replaced by it (or gets its nodes appended, with new ids).
fn put_grade(s: &mut Session, cmd: &'static str, label: &str, ids: Vec<ClipId>, pg: crate::library::Powergrade, append: bool) -> Result<Value> {
    let seq_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    let n = ids.len();
    s.edit(label, move |project, _| {
        let mut grade = pg.grade.clone();
        crate::library::import_luts(&pg.luts, grade.nodes.iter_mut(), project);
        let seq = project.sequence_mut(seq_id).ok_or(EngineError::NoSequence)?;
        for id in &ids {
            let (_, item) = seq.find_item_mut(*id).ok_or_else(|| EngineError::Other("the clip went away".into()))?;
            let new = if append {
                let mut g = store::grade_of(item).map_err(|e| bad(cmd, e))?;
                for node in &grade.nodes {
                    g.add(node.kind.clone(), Some(&node.name), None).map_err(|e| bad(cmd, e))?;
                    if let Some(last) = g.nodes.last_mut() {
                        last.enabled = node.enabled;
                    }
                }
                g
            } else {
                grade.clone()
            };
            store::set_grade(item, &new);
        }
        Ok(())
    })?;
    Ok(json!({ "clips": n }))
}

fn copy_grade(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.copyGrade";
    let id = target(s, CMD, p)?;
    let g = grade_at(s, CMD, id)?;
    let nodes = g.nodes.len();
    crate::library::copy(crate::library::Copied::Grade(crate::library::Powergrade::new("", g, &s.project)));
    Ok(json!({ "clip": id, "nodes": nodes }))
}

fn paste_grade(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.pasteGrade";
    let append = paste_mode(CMD, p)?;
    let Some(crate::library::Copied::Grade(pg)) = crate::library::clipboard() else {
        return Err(EngineError::Disabled(CMD.into(), "no grade was copied (koda.color.copyGrade)".into()));
    };
    let ids = targets(s, CMD, p)?;
    put_grade(s, CMD, "Paste Color Grade", ids, pg, append)
}

fn apply_grade_from(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.applyGradeFrom";
    let append = paste_mode(CMD, p)?;
    let from: ClipId = p.get("from").cloned().and_then(|v| serde_json::from_value(v).ok()).ok_or_else(|| bad(CMD, "from: expected a clip id"))?;
    let g = grade_at(s, CMD, from)?;
    let ids: Vec<ClipId> = targets(s, CMD, p)?.into_iter().filter(|c| *c != from).collect();
    if ids.is_empty() {
        return Err(bad(CMD, "no other clip to apply the grade to"));
    }
    let pg = crate::library::Powergrade::new("", g, &s.project);
    put_grade(s, CMD, "Apply Grade from Clip", ids, pg, append)
}

fn copy_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.copyNode";
    let n = node_id(CMD, p)?;
    let id = target(s, CMD, p)?;
    let g = grade_at(s, CMD, id)?;
    let node = g.nodes.iter().find(|x| x.id == n).ok_or_else(|| bad(CMD, format!("no node {n} in this grade")))?.clone();
    let luts = crate::library::embedded_luts(std::iter::once(&node), &s.project);
    let ty = node.kind.type_name();
    crate::library::copy(crate::library::Copied::Node(node, luts));
    Ok(json!({ "node": n, "type": ty }))
}

/// Paste the copied node: inserted as a new node (at `index`, else the end), or onto the node
/// `replace` (its settings replaced; its id, name and on/off kept, like DaVinci's paste
/// attributes).
fn paste_node(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.pasteNode";
    let Some(crate::library::Copied::Node(node, luts)) = crate::library::clipboard() else {
        return Err(EngineError::Disabled(CMD.into(), "no node was copied (koda.color.copyNode)".into()));
    };
    let index = match p.get("index") {
        None => None,
        Some(v) => Some(v.as_u64().and_then(|i| usize::try_from(i).ok()).ok_or_else(|| bad(CMD, "index: expected a position from 0"))?),
    };
    let replace = match p.get("replace") {
        None => None,
        Some(v) => Some(v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or_else(|| bad(CMD, "replace: expected a node id"))?),
    };
    let id = target(s, CMD, p)?;
    let seq_id = s.state.active_sequence.ok_or(EngineError::NoSequence)?;
    s.edit("Paste Color Node", move |project, _| {
        let mut node = node;
        crate::library::import_luts(&luts, std::iter::once(&mut node), project);
        let seq = project.sequence_mut(seq_id).ok_or(EngineError::NoSequence)?;
        let (_, item) = seq.find_item_mut(id).ok_or_else(|| EngineError::Other("the clip went away".into()))?;
        let mut g = store::grade_of(item).map_err(|e| bad(CMD, e))?;
        let out = match replace {
            Some(r) => {
                g.node_mut(r).map_err(|e| bad(CMD, e))?.kind = node.kind;
                r
            }
            None => {
                let new = g.add(node.kind, Some(&node.name), index).map_err(|e| bad(CMD, e))?;
                if let Ok(x) = g.node_mut(new) {
                    x.enabled = node.enabled;
                }
                new
            }
        };
        store::set_grade(item, &g);
        Ok(json!({ "node": out }))
    })
}

fn powergrade_path(cmd: &str, p: &Value, name: &str) -> Result<std::path::PathBuf> {
    match p.get("path") {
        Some(v) => Ok(std::path::PathBuf::from(v.as_str().ok_or_else(|| bad(cmd, "path: expected a file path"))?)),
        None => crate::library::default_dir()
            .map(|d| d.join(crate::library::file_name(name)))
            .ok_or_else(|| bad(cmd, "no library folder on this system: pass \"path\"")),
    }
}

fn save_powergrade(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.savePowergrade";
    let name = p.get("name").and_then(Value::as_str).ok_or_else(|| bad(CMD, "name: expected text"))?.to_string();
    let id = target(s, CMD, p)?;
    let g = grade_at(s, CMD, id)?;
    if g.nodes.is_empty() {
        return Err(bad(CMD, "the clip has no grade to save"));
    }
    let path = powergrade_path(CMD, p, &name)?;
    let pg = crate::library::Powergrade::new(&name, g, &s.project);
    crate::library::save(&path, &pg).map_err(|e| bad(CMD, e))?;
    Ok(json!({ "path": path.to_string_lossy(), "name": pg.name, "nodes": pg.grade.nodes.len(), "luts": pg.luts.len() }))
}

fn apply_powergrade(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.applyPowergrade";
    let append = paste_mode(CMD, p)?;
    let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad(CMD, "path: expected a .kpg file"))?;
    let pg = crate::library::load(std::path::Path::new(path)).map_err(|e| bad(CMD, e))?;
    let ids = targets(s, CMD, p)?;
    let name = pg.name.clone();
    let mut out = put_grade(s, CMD, "Apply Powergrade", ids, pg, append)?;
    out["name"] = json!(name);
    Ok(out)
}

fn list_powergrades(_: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "koda.color.listPowergrades";
    let dir = match p.get("dir") {
        Some(v) => std::path::PathBuf::from(v.as_str().ok_or_else(|| bad(CMD, "dir: expected a folder"))?),
        None => crate::library::default_dir().ok_or_else(|| bad(CMD, "no library folder on this system: pass \"dir\""))?,
    };
    Ok(json!({ "dir": dir.to_string_lossy(), "powergrades": crate::library::list(&dir) }))
}

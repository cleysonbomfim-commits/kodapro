//! Powergrades and the grade clipboard (aba Cor marco 6, the engine side of DaVinci's Gallery and
//! copy / paste grade).
//!
//! A **powergrade** is a grade saved to a file (`*.kpg`, JSON) with the LUTs its nodes use
//! embedded, so it works in any project: applying it imports the LUTs into the project's LUT
//! library (reusing identical ones) and points the nodes at them. The default library folder is
//! `<FilmCraft data dir>/koda/powergrades` ([`default_dir`]).
//!
//! The **clipboard** holds a copied grade or node, with its LUTs, for this run of the app.
//!
//! Files are untrusted: size-capped before reading, parsed, then sanitized like a project grade.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::grade::{Grade, Node, NodeKind};

/// File extension of a powergrade.
pub const EXT: &str = "kpg";
/// Format tag written into every file.
pub const FORMAT: &str = "koda-powergrade";
pub const VERSION: u32 = 1;
/// Largest powergrade file read (LUT text included).
pub const MAX_FILE: u64 = 64 << 20;
/// Most LUTs a powergrade carries, and the largest one.
pub const MAX_LUTS: usize = 16;
pub const MAX_LUT_TEXT: usize = 16 << 20;
/// Most files [`list`] returns.
pub const MAX_LIST: usize = 1000;

/// A LUT carried with a grade: the reference its nodes use and the file's text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedLut {
    /// The reference in `grade` (`lib:<id>`).
    pub reference: String,
    pub name: String,
    /// `cube` or `3dl`.
    pub format: String,
    pub text: String,
}

/// A grade with what it needs to travel: a powergrade file, or the clipboard.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Powergrade {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub grade: Grade,
    #[serde(default)]
    pub luts: Vec<EmbeddedLut>,
}

impl Powergrade {
    /// `grade` named `name`, with the LUTs of `project` its nodes reference.
    pub fn new(name: &str, grade: Grade, project: &filmcraft_project::Project) -> Self {
        let luts = embedded_luts(grade.nodes.iter(), project);
        Powergrade { format: FORMAT.into(), version: VERSION, name: name.chars().take(crate::grade::MAX_NAME * 2).collect(), grade, luts }
    }

    /// Parse and sanitize a powergrade's JSON text.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let p: Powergrade = serde_json::from_str(text).map_err(|e| format!("not a KODA powergrade: {e}"))?;
        if p.format != FORMAT {
            return Err(format!("not a KODA powergrade (format \"{}\")", p.format.chars().take(40).collect::<String>()));
        }
        if p.version > VERSION {
            return Err(format!("powergrade version {} is newer than this KODA Pro reads ({VERSION})", p.version));
        }
        if p.luts.len() > MAX_LUTS || p.luts.iter().any(|l| l.text.len() > MAX_LUT_TEXT) {
            return Err("the powergrade's LUTs are too many or too large".into());
        }
        let luts = p
            .luts
            .into_iter()
            .map(|l| EmbeddedLut {
                reference: l.reference.chars().take(crate::grade::MAX_LUT_REF).collect(),
                name: l.name.chars().take(256).collect(),
                format: if l.format == "3dl" { "3dl".into() } else { "cube".into() },
                text: l.text,
            })
            .collect();
        Ok(Powergrade {
            format: FORMAT.into(),
            version: VERSION,
            name: p.name.chars().take(crate::grade::MAX_NAME * 2).collect(),
            grade: p.grade.sanitized(),
            luts,
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// The LUTs (from `project`'s library) the LUT nodes among `nodes` reference, once each.
pub fn embedded_luts<'a>(nodes: impl Iterator<Item = &'a Node>, project: &filmcraft_project::Project) -> Vec<EmbeddedLut> {
    let mut out: Vec<EmbeddedLut> = Vec::new();
    for n in nodes {
        let NodeKind::Lut { lut, .. } = &n.kind else { continue };
        if out.iter().any(|e| &e.reference == lut) || out.len() >= MAX_LUTS {
            continue;
        }
        let Some(id) = lut.strip_prefix("lib:") else { continue };
        if let Some(l) = project.luts.iter().find(|l| l.id == id) {
            out.push(EmbeddedLut { reference: lut.clone(), name: l.name.clone(), format: l.format.clone(), text: l.text.to_string() });
        }
    }
    out
}

/// Bring `luts` into `project`'s LUT library (an identical text is reused, never duplicated) and
/// point the LUT nodes among `nodes` at the library entries.
pub fn import_luts<'a>(luts: &[EmbeddedLut], nodes: impl Iterator<Item = &'a mut Node>, project: &mut filmcraft_project::Project) {
    let mut map: Vec<(String, String)> = Vec::new();
    for l in luts {
        let id = match project.luts.iter().find(|p| *p.text == *l.text) {
            Some(p) => p.id.clone(),
            None => {
                let id = format!("lut{}", project.alloc_id());
                project.luts.push(filmcraft_project::ProjectLut {
                    id: id.clone(),
                    name: l.name.clone(),
                    source_path: None,
                    format: l.format.clone(),
                    text: l.text.as_str().into(),
                });
                id
            }
        };
        map.push((l.reference.clone(), format!("lib:{id}")));
    }
    for n in nodes {
        if let NodeKind::Lut { lut, .. } = &mut n.kind
            && let Some((_, to)) = map.iter().find(|(from, _)| from == lut)
        {
            *lut = to.clone();
        }
    }
}

/// The default powergrade folder: `<FilmCraft data dir>/koda/powergrades`.
pub fn default_dir() -> Option<PathBuf> {
    filmcraft_engine::autosave::default_data_dir().map(|d| d.join("koda").join("powergrades"))
}

/// A file name for a powergrade called `name` (characters that file systems refuse removed).
pub fn file_name(name: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).take(80).collect();
    let clean = clean.trim().trim_start_matches('.');
    format!("{}.{EXT}", if clean.is_empty() { "powergrade" } else { clean })
}

/// Write `pg` to `path` (folders made as needed).
pub fn save(path: &Path, pg: &Powergrade) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, pg.to_json()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Read a powergrade file (at most [`MAX_FILE`] bytes).
pub fn load(path: &Path) -> Result<Powergrade, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{}: not a file", path.display()));
    }
    if meta.len() > MAX_FILE {
        return Err(format!("{}: larger than {} MB", path.display(), MAX_FILE >> 20));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Powergrade::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The powergrades in `dir` (name, file, node count), sorted by name; unreadable files are
/// listed with their error.
pub fn list(dir: &Path) -> Vec<serde_json::Value> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(String, serde_json::Value)> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(EXT))
        .take(MAX_LIST)
        .map(|p| match load(&p) {
            Ok(pg) => (
                pg.name.to_lowercase(),
                serde_json::json!({ "name": pg.name, "path": p.to_string_lossy(), "nodes": pg.grade.nodes.len(), "luts": pg.luts.len() }),
            ),
            Err(e) => (String::from("\u{10ffff}"), serde_json::json!({ "path": p.to_string_lossy(), "error": e })),
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.into_iter().map(|(_, v)| v).collect()
}

/// What was copied.
#[derive(Clone, Debug, PartialEq)]
pub enum Copied {
    /// A whole grade (with its LUTs).
    Grade(Powergrade),
    /// One node (with its LUT, if it is a LUT node).
    Node(Node, Vec<EmbeddedLut>),
}

static CLIPBOARD: Mutex<Option<Copied>> = Mutex::new(None);

/// Put `c` on the grade clipboard.
pub fn copy(c: Copied) {
    *CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner) = Some(c);
}

/// What the grade clipboard holds.
pub fn clipboard() -> Option<Copied> {
    CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name("KODA global"), "KODA global.kpg");
        assert_eq!(file_name("../../etc/passwd"), "_.._etc_passwd.kpg");
        assert_eq!(file_name(""), "powergrade.kpg");
        assert_eq!(file_name("..."), "powergrade.kpg");
        assert!(file_name(&"x".repeat(500)).len() < 100);
    }

    #[test]
    fn hostile_powergrades_are_refused() {
        assert!(Powergrade::from_json("{}").is_err());
        assert!(Powergrade::from_json(r#"{"format":"x","version":1,"name":"a","grade":{}}"#).is_err());
        assert!(Powergrade::from_json(r#"{"format":"koda-powergrade","version":99,"name":"a","grade":{}}"#).is_err());
        let many: Vec<serde_json::Value> =
            (0..40).map(|i| serde_json::json!({"reference": format!("lib:{i}"), "name": "x", "format": "cube", "text": ""})).collect();
        let j = serde_json::json!({"format": FORMAT, "version": 1, "name": "a", "grade": {}, "luts": many});
        assert!(Powergrade::from_json(&j.to_string()).is_err());
        // a grade inside is sanitized like a project's
        let j = serde_json::json!({"format": FORMAT, "version": 1, "name": "n".repeat(10_000), "grade": {"nodes": [{"id": 0, "name": "g", "enabled": true, "type": "gain", "master": 1e30}]}});
        let p = Powergrade::from_json(&j.to_string()).unwrap();
        assert_eq!(p.grade.nodes[0].kind, NodeKind::Gain { master: crate::grade::GAIN_RANGE.1, rgb: [1.0; 3] });
        assert!(p.name.chars().count() <= crate::grade::MAX_NAME * 2);
    }
}

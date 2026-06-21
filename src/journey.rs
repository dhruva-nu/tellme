//! Cross-layer data journey (#40, #41).
//!
//! Starting from an endpoint function, follow its call chain across the
//! project's Python files and present the data's path through architectural
//! layers (DB → repository → service → controller → API), noting where the
//! shape changes. Layer detection and transformation hints are heuristic
//! (path naming + return-expression patterns).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::analysis::{self, FunctionDef};
use crate::error::{Error, Result};

/// One layer box in the journey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Layer name (db / repository / service / controller / api).
    pub layer: String,
    /// Function (or table) at this layer.
    pub label: String,
    /// Repo-relative file, if any.
    pub file: Option<String>,
    /// A short note about what the data looks like here.
    pub note: String,
}

/// The full journey for an endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Journey {
    /// Endpoint function name.
    pub endpoint: String,
    /// Stages ordered from data origin (DB) to the API response.
    pub stages: Vec<Stage>,
    /// Shape changes detected along the way.
    pub transformations: Vec<String>,
}

struct Indexed {
    file: String,
    def: FunctionDef,
}

/// Classify a file path into a `(rank, layer)`; lower rank is closer to the DB.
fn layer_of(path: &str) -> (u8, &'static str) {
    let p = path.to_lowercase();
    if p.contains("model") || p.contains("/db") || p.contains("database") || p.contains("schema") {
        (1, "db")
    } else if p.contains("repositor") || p.contains("/repo") || p.contains("dao") {
        (2, "repository")
    } else if p.contains("service") {
        (3, "service")
    } else if p.contains("controller")
        || p.contains("/api")
        || p.contains("route")
        || p.contains("view")
    {
        (4, "controller")
    } else {
        (3, "other")
    }
}

/// Whether a function appears to touch the database.
fn touches_db(def: &FunctionDef) -> bool {
    let hay = |s: &str| {
        let l = s.to_lowercase();
        l.contains("query") || l.contains("select ") || l.contains("db.") || l.contains("execute")
    };
    def.returns.iter().any(|r| hay(r)) || def.callees.iter().any(|c| hay(&c.name))
}

/// A transformation hint from a return expression, if recognizable.
fn transformation(ret: &str) -> Option<&'static str> {
    let l = ret.to_lowercase();
    if l.contains("jsonify") || l.contains("json(") {
        Some("→ JSON response")
    } else if l.contains("to_dto") || l.contains(".dto") {
        Some("model → DTO")
    } else if l.contains("(**") || l.contains("[item(") || l.contains("model(") {
        Some("rows → models")
    } else if l.contains("query") || l.contains("select ") {
        Some("DB → rows")
    } else {
        None
    }
}

/// Collect `.py` files under `root`, skipping vcs/build/venv noise.
fn python_files(root: &Path) -> Vec<PathBuf> {
    fn skip(name: &str) -> bool {
        matches!(
            name,
            ".git" | ".tellme" | "node_modules" | "__pycache__" | "target" | ".venv" | "venv"
        )
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !skip(&name) {
                    stack.push(path);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("py") {
                out.push(path);
            }
        }
    }
    out
}

/// Index every function in the project by name (first definition wins).
fn build_index(root: &Path) -> HashMap<String, Indexed> {
    let mut index = HashMap::new();
    for path in python_files(root) {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(analyzer) = analysis::for_path(&path) else {
            continue;
        };
        let Ok(defs) = analyzer.functions(&src) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        for def in defs {
            index.entry(def.name.clone()).or_insert_with(|| Indexed {
                file: rel.clone(),
                def: def.clone(),
            });
        }
    }
    index
}

/// Build the journey for `endpoint`, starting search at `repo_root`.
pub fn journey(repo_root: &Path, endpoint: &str) -> Result<Journey> {
    let index = build_index(repo_root);
    if !index.contains_key(endpoint) {
        return Err(Error::Other(format!(
            "endpoint `{endpoint}` not found in any Python file under the repo"
        )));
    }

    // Follow the call chain from the endpoint through project functions.
    let mut order: Vec<String> = Vec::new();
    let mut stack = vec![endpoint.to_string()];
    while let Some(name) = stack.pop() {
        if order.contains(&name) {
            continue;
        }
        order.push(name.clone());
        if let Some(item) = index.get(&name) {
            // Push callees that resolve to project functions (preserve order).
            for c in &item.def.callees {
                let short = c.name.rsplit('.').next().unwrap_or(&c.name).to_string();
                if index.contains_key(&short) && !order.contains(&short) {
                    stack.push(short);
                }
            }
        }
    }

    // Build stages from visited functions, sorted DB-first by layer.
    let mut stages: Vec<(u8, Stage)> = Vec::new();
    let mut transformations = Vec::new();
    let mut db_seen = false;
    for name in &order {
        let Some(item) = index.get(name) else {
            continue;
        };
        let (rank, layer) = layer_of(&item.file);
        let note = item
            .def
            .returns
            .first()
            .map(|r| truncate(r, 48))
            .unwrap_or_else(|| "—".into());
        stages.push((
            rank,
            Stage {
                layer: layer.to_string(),
                label: format!("{}()", name),
                file: Some(item.file.clone()),
                note,
            },
        ));
        for r in &item.def.returns {
            if let Some(t) = transformation(r) {
                let line = format!("{name}: {t}");
                if !transformations.contains(&line) {
                    transformations.push(line);
                }
            }
        }
        if touches_db(&item.def) {
            db_seen = true;
        }
    }
    stages.sort_by_key(|(rank, _)| *rank);
    let mut stages: Vec<Stage> = stages.into_iter().map(|(_, s)| s).collect();

    if db_seen {
        stages.insert(
            0,
            Stage {
                layer: "db".into(),
                label: "database".into(),
                file: None,
                note: "raw rows".into(),
            },
        );
    }
    stages.push(Stage {
        layer: "api".into(),
        label: "API response".into(),
        file: None,
        note: "serialized output".into(),
    });

    Ok(Journey {
        endpoint: endpoint.to_string(),
        stages,
        transformations,
    })
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > max {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn traces_endpoint_across_layers() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(
            root,
            "repository/items.py",
            "def fetch_all():\n    return db.query(\"SELECT * FROM items\")\n",
        );
        write(
            root,
            "service/items.py",
            "def list_items():\n    rows = fetch_all()\n    return [Item(**r) for r in rows]\n",
        );
        write(root, "controller/items.py", "def items():\n    models = list_items()\n    return jsonify([i.to_dto() for i in models])\n");

        let j = journey(root, "items").unwrap();
        let layers: Vec<&str> = j.stages.iter().map(|s| s.layer.as_str()).collect();
        // DB origin first, API response last.
        assert_eq!(layers.first(), Some(&"db"));
        assert_eq!(layers.last(), Some(&"api"));
        assert!(layers.contains(&"repository"));
        assert!(layers.contains(&"service"));
        assert!(layers.contains(&"controller"));
        // Transformations detected.
        assert!(j.transformations.iter().any(|t| t.contains("JSON")));
        assert!(j
            .transformations
            .iter()
            .any(|t| t.contains("rows → models")));
    }

    #[test]
    fn missing_endpoint_errors() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.py", "def x():\n    return 1\n");
        assert!(journey(dir.path(), "nope").is_err());
    }
}

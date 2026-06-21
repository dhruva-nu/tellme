//! Source analysis behind a language-agnostic trait (#33).
//!
//! v1 ships a Python implementation; other languages slot in by adding an
//! [`Analyzer`] and wiring [`for_path`]. The flow types are language-neutral.

mod python;

use std::path::Path;

use crate::error::{Error, Result};

/// What happened to a variable at a point in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarEventKind {
    /// First assignment.
    Initialized,
    /// A later (re)assignment.
    Modified,
    /// A read.
    Used,
    /// The last point the variable is referenced.
    LifecycleEnd,
}

impl VarEventKind {
    /// A short uppercase label for display.
    pub fn label(self) -> &'static str {
        match self {
            VarEventKind::Initialized => "INITIALIZED",
            VarEventKind::Modified => "MODIFIED",
            VarEventKind::Used => "USED",
            VarEventKind::LifecycleEnd => "LIFECYCLE END",
        }
    }
}

/// One event in a variable's life.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarEvent {
    /// What happened.
    pub kind: VarEventKind,
    /// One-based line.
    pub line: usize,
    /// The source line, trimmed.
    pub text: String,
}

/// A variable's lifecycle within its function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarFlow {
    /// Variable name.
    pub name: String,
    /// Enclosing function, if any.
    pub function: Option<String>,
    /// Events, ordered by line.
    pub events: Vec<VarEvent>,
}

/// A function call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// Callee name (may be dotted, e.g. `cart.items`).
    pub name: String,
    /// One-based line.
    pub line: usize,
}

/// A function's connections and signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFlow {
    /// Function name.
    pub name: String,
    /// One-based line of the definition.
    pub line: usize,
    /// Parameter declarations (text as written).
    pub params: Vec<String>,
    /// Return annotation, if present.
    pub returns: Option<String>,
    /// Call sites of this function within the file.
    pub callers: Vec<Call>,
    /// Calls made within this function's body.
    pub callees: Vec<Call>,
}

/// A language-specific source analyzer.
pub trait Analyzer {
    /// Trace a variable's lifecycle.
    fn var_flow(&self, src: &str, var: &str) -> Result<VarFlow>;
    /// Trace a function's callers, callees, and signature.
    fn function_flow(&self, src: &str, func: &str) -> Result<FunctionFlow>;
}

/// Pick an analyzer for a file by extension. v1 supports Python.
pub fn for_path(path: &Path) -> Result<Box<dyn Analyzer>> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py") => Ok(Box::new(python::Python::new()?)),
        other => Err(Error::Other(format!(
            "no analyzer for {} files yet (v1 supports Python)",
            other.unwrap_or("(unknown)")
        ))),
    }
}

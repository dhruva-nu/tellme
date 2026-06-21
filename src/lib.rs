//! tellme — git blame, but for prompts and decisions.
//!
//! Library crate exposing the building blocks the `tellme` binary wires
//! together: the CLI surface, command handlers, the `.tellme/` store, git
//! integration, and configuration. Many items here are the foundation that
//! later roadmap phases (Prompt Blame, Data Flow, …) build on.

pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod git;
pub mod paths;
pub mod store;

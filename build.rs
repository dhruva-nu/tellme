//! Build-time shell-completion generation.
//!
//! `include!`s the real CLI definition from `src/cli.rs` (inside a module, so
//! its inner doc comments and imports stay local) — the completion scripts can
//! therefore never drift from the command surface. `cli.rs`'s only
//! crate-internal dependency is `config::OutputFormat`, which we mirror in a
//! tiny shim below (completion only needs its `ValueEnum` shape), keeping the
//! library source untouched.

use std::path::PathBuf;

use clap::{CommandFactory, ValueEnum};
use clap_complete::{generate_to, Shell};

// Shim mirroring `src/config.rs::OutputFormat`. Keep the variants in sync.
mod config {
    #[derive(Debug, Clone, clap::ValueEnum)]
    #[value(rename_all = "lowercase")]
    pub enum OutputFormat {
        Text,
        Json,
        Dot,
    }
}

// Shim mirroring `src/completion.rs`. The aot generator never invokes the
// completer (dynamic candidates are produced at runtime), so a stub with the
// matching signature is enough to satisfy the `add = ...` arg attributes.
mod completion {
    use std::ffi::OsStr;

    use clap_complete::CompletionCandidate;

    pub fn complete_repo_file(_current: &OsStr) -> Vec<CompletionCandidate> {
        Vec::new()
    }
}

#[path = "src/cli.rs"]
mod cli;

fn main() {
    println!("cargo:rerun-if-changed=src/cli.rs");
    println!("cargo:rerun-if-changed=build.rs");

    // Emit into `completions/` at the crate root so packagers and users can
    // find the scripts without digging through `target/`.
    let outdir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("completions");
    if let Err(e) = std::fs::create_dir_all(&outdir) {
        println!("cargo:warning=could not create {}: {e}", outdir.display());
        return;
    }

    let mut cmd = cli::Cli::command();
    for &shell in Shell::value_variants() {
        if let Err(e) = generate_to(shell, &mut cmd, "tellme", &outdir) {
            println!("cargo:warning=failed to generate {shell} completion: {e}");
        }
    }
}

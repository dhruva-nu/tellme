#!/usr/bin/env bash
#
# install.sh — build tellme in release mode and install it as a system CLI.
#
# Usage:
#   ./install.sh                # build + install to the default bin dir
#   BINDIR=~/.local/bin ./install.sh   # install somewhere specific
#   ./install.sh --uninstall    # remove an installed tellme
#   ./install.sh --no-completions      # skip shell-completion setup
#
# Default install dir:
#   - $BINDIR if set
#   - /usr/local/bin if writable or sudo is available
#   - otherwise ~/.local/bin
#
set -euo pipefail

BIN_NAME="tellme"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- helpers ---------------------------------------------------------------

info()  { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
warn()  { printf '\033[1;33mwarning:\033[0m %s\n' "$*" >&2; }
die()   { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

# Pick an install directory and decide whether sudo is needed.
choose_bindir() {
    if [ -n "${BINDIR:-}" ]; then
        echo "$BINDIR"
        return
    fi
    if [ -d /usr/local/bin ] && { [ -w /usr/local/bin ] || command -v sudo >/dev/null 2>&1; }; then
        echo /usr/local/bin
        return
    fi
    echo "$HOME/.local/bin"
}

# Whether we can write into $1 — checking the nearest existing ancestor, since
# the dir itself may not exist yet (we may be about to mkdir it).
can_write() {
    local d="$1"
    while [ ! -e "$d" ]; do d="$(dirname "$d")"; done
    [ -w "$d" ]
}

# Run a command, using sudo only when the target dir is not writable by us.
maybe_sudo() {
    local dir="$1"; shift
    if can_write "$dir"; then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo "$@"
    else
        die "no write permission for $dir and sudo is unavailable"
    fi
}

# ---- shell completion ------------------------------------------------------
#
# Completion is *dynamic*: each shell sources a tiny registration that calls
# `tellme` at startup, so subcommands, flags, and — crucially — basename file
# completion (e.g. `errors.py` → `backend/dls/errors.py`) all stay in sync with
# the binary. We add a marked block to the user's rc so it's idempotent and
# cleanly removable on --uninstall.

COMPLETION_BEGIN="# >>> tellme completion >>>"
COMPLETION_END="# <<< tellme completion <<<"

# The rc file for the current shell, or empty if we don't manage it (e.g. fish,
# which is handled separately via a completions file).
completion_rc() {
    case "$(basename "${SHELL:-}")" in
        bash) echo "$HOME/.bashrc" ;;
        zsh)  echo "${ZDOTDIR:-$HOME}/.zshrc" ;;
        *)    echo "" ;;
    esac
}

# Strip any existing tellme completion block from a file (in place).
strip_completion_block() {
    local file="$1"
    [ -f "$file" ] || return 0
    # Delete everything between the markers, inclusive.
    sed -i.bak "\|^${COMPLETION_BEGIN}\$|,\|^${COMPLETION_END}\$|d" "$file"
    rm -f "$file.bak"
}

setup_completions() {
    local shell_name; shell_name="$(basename "${SHELL:-}")"

    # fish: drop a self-sourcing completion file; no rc edits needed.
    if [ "$shell_name" = "fish" ]; then
        local dir="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions"
        mkdir -p "$dir"
        printf 'COMPLETE=fish %s | source\n' "$BIN_NAME" > "$dir/$BIN_NAME.fish"
        info "Installed fish completion → $dir/$BIN_NAME.fish"
        return
    fi

    local rc; rc="$(completion_rc)"
    if [ -z "$rc" ]; then
        warn "Unknown shell '${shell_name:-?}'; skipping completion setup."
        echo "  To enable it manually, add to your shell rc:"
        echo "    source <(COMPLETE=<shell> $BIN_NAME)   # <shell> = bash | zsh"
        return
    fi

    # Re-write our block: strip any prior copy, then append a fresh one.
    strip_completion_block "$rc"
    touch "$rc"
    {
        echo "$COMPLETION_BEGIN"
        echo "source <(COMPLETE=$shell_name $BIN_NAME)"
        if [ "$shell_name" = "zsh" ]; then
            # Show an interactive, arrow-navigable menu when several files match
            # (e.g. `graph` → graph.py / graph-web.json) instead of inserting the
            # first. Global so it applies to all completions, like most setups.
            echo "zstyle ':completion:*' menu select"
        fi
        echo "$COMPLETION_END"
    } >> "$rc"
    info "Enabled $shell_name completion in $rc"
    echo "  Restart your shell or run:  source \"$rc\""
}

remove_completions() {
    local shell_name; shell_name="$(basename "${SHELL:-}")"
    if [ "$shell_name" = "fish" ]; then
        local f="${XDG_CONFIG_HOME:-$HOME/.config}/fish/completions/$BIN_NAME.fish"
        [ -e "$f" ] && { rm -f "$f"; info "Removed $f"; }
        return
    fi
    local rc; rc="$(completion_rc)"
    if [ -n "$rc" ] && [ -f "$rc" ] && grep -qF "$COMPLETION_BEGIN" "$rc"; then
        strip_completion_block "$rc"
        info "Removed tellme completion from $rc"
    fi
}

# ---- argument parsing ------------------------------------------------------

DO_UNINSTALL=0
WITH_COMPLETIONS=1
for arg in "$@"; do
    case "$arg" in
        --uninstall)      DO_UNINSTALL=1 ;;
        --no-completions) WITH_COMPLETIONS=0 ;;
        *) die "unknown option: $arg" ;;
    esac
done

if [ "$DO_UNINSTALL" -eq 1 ]; then
    BINDIR="$(choose_bindir)"
    target="$BINDIR/$BIN_NAME"
    if [ -e "$target" ]; then
        info "Removing $target"
        maybe_sudo "$BINDIR" rm -f "$target"
        info "Uninstalled."
    else
        warn "$target not found; nothing to do."
    fi
    [ "$WITH_COMPLETIONS" -eq 1 ] && remove_completions
    exit 0
fi

# ---- build -----------------------------------------------------------------

command -v cargo >/dev/null 2>&1 || die "cargo not found — install Rust from https://rustup.rs"

info "Building $BIN_NAME (release)…"
( cd "$SCRIPT_DIR" && cargo build --release --locked )

artifact="$SCRIPT_DIR/target/release/$BIN_NAME"
[ -x "$artifact" ] || die "build succeeded but $artifact is missing"

# ---- install ---------------------------------------------------------------

BINDIR="$(choose_bindir)"
info "Installing to $BINDIR"
maybe_sudo "$BINDIR" mkdir -p "$BINDIR"
maybe_sudo "$BINDIR" install -m 0755 "$artifact" "$BINDIR/$BIN_NAME"

installed="$BINDIR/$BIN_NAME"
info "Installed $("$installed" --version) → $installed"

# ---- PATH check ------------------------------------------------------------

case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *)
        warn "$BINDIR is not on your PATH."
        echo "  Add it by appending this to your shell rc (~/.bashrc, ~/.zshrc):"
        echo "    export PATH=\"$BINDIR:\$PATH\""
        ;;
esac

# ---- shell completion ------------------------------------------------------

if [ "$WITH_COMPLETIONS" -eq 1 ]; then
    setup_completions
else
    info "Skipping shell-completion setup (--no-completions)."
fi

info "Done. Try:  $BIN_NAME --help"

#!/usr/bin/env bash
#
# install.sh — build tellme in release mode and install it as a system CLI.
#
# Usage:
#   ./install.sh                # build + install to the default bin dir
#   BINDIR=~/.local/bin ./install.sh   # install somewhere specific
#   ./install.sh --uninstall    # remove an installed tellme
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

# ---- uninstall -------------------------------------------------------------

if [ "${1:-}" = "--uninstall" ]; then
    BINDIR="$(choose_bindir)"
    target="$BINDIR/$BIN_NAME"
    if [ -e "$target" ]; then
        info "Removing $target"
        maybe_sudo "$BINDIR" rm -f "$target"
        info "Uninstalled."
    else
        warn "$target not found; nothing to do."
    fi
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

info "Done. Try:  $BIN_NAME --help"

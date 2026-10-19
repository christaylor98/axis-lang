#!/usr/bin/env bash
# Install the Axis toolchain: one `axis` command that builds .m1 / .ai3 / .pyax
# source into native executables.
#
#   scripts/install.sh [--bin-dir DIR] [--no-link] [--update] [--offline] [--no-smoke]
#
# Layout. `axis` finds everything it needs relative to the checkout it was
# built in, so the three repos sit side by side:
#
#   <parent>/axis-lang-lab-working     this repo: compiler + surface configs
#   <parent>/axRegistry        the registry files
#   <parent>/axis-codegen-bridge-rs    CoreIR -> executable (runs rustc)
#
# Missing repos are cloned. Existing checkouts are never touched unless you
# pass --update, which fast-forwards them (and refuses if one has local
# changes). Both tools are built in release mode, and `axis` is symlinked into
# --bin-dir (default ~/.local/bin). The bridge is NOT copied anywhere: it finds
# its runtime library beside its own binary, so it has to stay where it was
# built. Re-running the script is safe; it rebuilds whatever changed.
#
# Sandboxes. Nothing outside the three checkouts and --bin-dir is written.
#   - No network: put the three repos in place first (clone or copy) and pass
#     --offline; cargo then builds from its local cache (~/.cargo/registry), and
#     says which crate is missing if the cache is incomplete. Without
#     --offline, a failed online build is retried offline automatically.
#   - Read-only home: --bin-dir somewhere writable, or --no-link to skip the
#     symlink and call target/release/axis by its full path.
#   - Unwritable /tmp: TMPDIR is pointed at <lab>/target/tmp, and exported so
#     `axis build` (which links in a temp dir) uses it too — set TMPDIR the
#     same way when you run axis later in that sandbox.
#   - Rust installed by rustup but not on PATH: ~/.cargo/bin is added.

set -euo pipefail

GITHUB="https://github.com/christaylor98"
BIN_DIR="${HOME}/.local/bin"
UPDATE=0
SMOKE=1
LINK=1
OFFLINE=0

usage() { sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit "${1:-0}"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --bin-dir)  BIN_DIR="${2:?--bin-dir needs a directory}"; shift 2 ;;
    --update)   UPDATE=1; shift ;;
    --offline)  OFFLINE=1; shift ;;
    --no-link)  LINK=0; shift ;;
    --no-smoke) SMOKE=0; shift ;;
    -h|--help)  usage 0 ;;
    *) echo "unknown argument: $1" >&2; usage 1 ;;
  esac
done

step() { printf '\n==> %s\n' "$*"; }
ok()   { printf '    ok  %s\n' "$*"; }
die()  { printf '\nerror: %s\n' "$*" >&2; exit 1; }

LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PARENT="$(dirname "$LAB")"
REGISTRY="$PARENT/axRegistry"
BRIDGE="$PARENT/axis-codegen-bridge-rs"

# ── 1. prerequisites ─────────────────────────────────────────────────────────
step "Checking prerequisites"
# rustup installs here; a sandbox's minimal PATH often leaves it out.
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
case ":$PATH:" in *":$cargo_bin:"*) ;; *) [ -d "$cargo_bin" ] && PATH="$cargo_bin:$PATH" ;; esac
export PATH

# A writable temp dir, for this script and for `axis build` itself.
writable_tmp() { local t; t="$(mktemp -d "${1}/axis-probe.XXXXXX" 2>/dev/null)" && rm -rf "$t"; }
if ! writable_tmp "${TMPDIR:-/tmp}"; then
  export TMPDIR="$LAB/target/tmp"
  mkdir -p "$TMPDIR" 2>/dev/null && writable_tmp "$TMPDIR" \
    || die "no writable temp dir: /tmp and $TMPDIR are both read-only. Set TMPDIR to a writable directory and re-run"
  ok "temp dir not writable; using TMPDIR=$TMPDIR"
fi
missing=()
need() {
  if command -v "$1" >/dev/null 2>&1; then ok "$1"; else missing+=("$1  ($2)"); fi
}
need git    "any recent version"
need cargo  "Rust toolchain: https://rustup.rs"
need rustc  "Rust toolchain: https://rustup.rs — also needed at build time, the bridge links with it"
need cc     "a C compiler: apt install build-essential"
need ar     "binutils: apt install binutils"
if [ ${#missing[@]} -gt 0 ]; then
  printf '\nMissing:\n' >&2
  printf '    %s\n' "${missing[@]}" >&2
  die "install the tools above and re-run"
fi
case "$(uname -m)" in
  x86_64|amd64) ;;
  *) die "the bridge runtime is x86_64-only (it compiles with -msha -msse4.1); this machine is $(uname -m)" ;;
esac

# ── 2. repositories ──────────────────────────────────────────────────────────
step "Repositories (in $PARENT)"
repo() {
  local dir="$1" name; name="$(basename "$1")"
  if [ ! -e "$dir" ]; then
    [ "$OFFLINE" = 1 ] && die "$dir is missing and --offline was given; clone or copy $GITHUB/$name.git there first"
    git clone --quiet "$GITHUB/$name.git" "$dir" \
      || die "cannot clone $GITHUB/$name.git (no network?). Clone or copy it to $dir, then re-run with --offline"
    ok "$name  cloned"
  elif [ ! -d "$dir/.git" ]; then
    die "$dir exists but is not a git checkout; move it aside and re-run"
  elif [ "$UPDATE" = 1 ]; then
    if [ -n "$(git -C "$dir" status --porcelain --untracked-files=no)" ]; then
      die "$name has uncommitted changes; commit or stash them, or re-run without --update"
    fi
    git -C "$dir" pull --quiet --ff-only || die "$name: cannot pull (no network, or diverged from its upstream)"
    ok "$name  updated to $(git -C "$dir" rev-parse --short HEAD)"
  else
    ok "$name  present at $(git -C "$dir" rev-parse --short HEAD) (not updated; pass --update)"
  fi
}
[ "$UPDATE" = 1 ] && repo "$LAB"
repo "$REGISTRY"
repo "$BRIDGE"
for f in axis-types axis axis-bridge m1-builtins gen-working; do
  [ -f "$REGISTRY/$f.axreg" ] || die "registry file missing: $REGISTRY/$f.axreg"
done

# ── 3. build ─────────────────────────────────────────────────────────────────
# Online first unless --offline; an online failure is retried offline, which
# succeeds in a network-less sandbox whenever ~/.cargo/registry has the crates.
# Compiler output goes to a log, shown only when a build fails.
build() {
  local dir="$1" bin="$2" log="${TMPDIR:-/tmp}/axis-install-$2.log"
  if [ "$OFFLINE" = 0 ] && (cd "$dir" && cargo build --release --bin "$bin") >"$log" 2>&1; then
    return 0
  fi
  [ "$OFFLINE" = 0 ] && printf '    online build failed; retrying offline from the local crate cache\n'
  if (cd "$dir" && cargo build --release --offline --bin "$bin") >"$log" 2>&1; then
    return 0
  fi
  tail -n 25 "$log" >&2
  die "cannot build $bin. If this sandbox has no network, the crates it needs must already be in ${CARGO_HOME:-$HOME/.cargo}/registry: build once with network access (or 'cargo fetch' in $dir), then re-run (full log: $log)"
}

step "Building the bridge (release)"
build "$BRIDGE" axis-codegen-bridge
ok "$BRIDGE/target/release/axis-codegen-bridge"

step "Building axis (release)"
build "$LAB" axis
ok "$LAB/target/release/axis"

# ── 4. install ───────────────────────────────────────────────────────────────
target="$LAB/target/release/axis"
link="$target"
if [ "$LINK" = 1 ]; then
  step "Installing axis into $BIN_DIR"
  mkdir -p "$BIN_DIR" 2>/dev/null && [ -w "$BIN_DIR" ] \
    || die "$BIN_DIR is not writable (sandbox?). Pass --bin-dir <writable dir>, or --no-link to use $target directly"
  link="$BIN_DIR/axis"
  # An older axis installed as a plain copy: kept, not deleted.
  if [ -e "$link" ] && [ ! -L "$link" ]; then
    mv "$link" "$link.bak"
    ok "moved the existing $link aside to $link.bak"
  fi
  # A symlink, not a copy: a later `cargo build --release` is picked up as is.
  ln -sfn "$target" "$link"
  ok "$link -> $target"
fi

# ── 5. smoke test ────────────────────────────────────────────────────────────
if [ "$SMOKE" = 1 ]; then
  step "Smoke test: build and run one program per surface"
  tmp="$(mktemp -d "${TMPDIR:-/tmp}/axis-smoke.XXXXXX")"
  trap 'rm -rf "$tmp"' EXIT
  cat > "$tmp/hello.m1" <<'EOF'
fn greet(name: Text) -> Text {
  str_concat(Text("hello "), name)
}

fn hello(args: ValueList(Text)) -> Text {
  greet(text_list_head(value_list_to_text_list(args)))
}
EOF
  cat > "$tmp/hello.ai3" <<'EOF'
fn greet(name: Text) -> Text {
  return str_concat(Text("hello "), name);
}

fn hello(args: ValueList(Text)) -> Text {
  return greet(text_list_head(value_list_to_text_list(args)));
}
EOF
  cat > "$tmp/hello.pyax" <<'EOF'
def greet(name: str) -> str:
    return "hello " + name

def main(args: list[str]) -> None:
    print(greet(args[0]))
EOF
  for s in m1 ai3 pyax; do
    got="$("$link" run "$tmp/hello.$s" "$s" 2>&1)" || die "axis run hello.$s failed:
$got"
    [ "$got" = "hello $s" ] || die "axis run hello.$s printed '$got', expected 'hello $s'"
    ok "hello.$s  ->  $got"
  done
fi

# ── done ─────────────────────────────────────────────────────────────────────
step "Installed"
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) [ "$LINK" = 1 ] && printf '\n    %s is not on your PATH. Add this to your shell profile:\n\n        export PATH="%s:$PATH"\n' "$BIN_DIR" "$BIN_DIR" ;;
esac
cat <<EOF

    $link build prog.ai3 -o prog     # .m1 / .ai3 / .pyax -> native executable
    $link run   prog.pyax arg ...    # build to a temp dir and run it

    Language guides: $LAB/docs/AI3_AUTHORING_PRIMER.md
    Re-run this script with --update to pull and rebuild all three repos.
EOF

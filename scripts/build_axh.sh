#!/usr/bin/env sh
set -e

# Get repository root (parent of scripts directory)
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

SRC=$1
OUT=${2:-build/h1/app}

mkdir -p "$REPO_ROOT/build/h1"

AXIS="${AXIS:-$REPO_ROOT/target/debug/axis}"

"$AXIS" \
  --lexer "$REPO_ROOT/axis-surface-h1-config/surface-h1-lexer.yaml" \
  --parser "$REPO_ROOT/axis-surface-h1-config/surface-h1-parse.yaml" \
  --schema "$REPO_ROOT/axis-surface-h1-config/surface-h1-ast.yaml" \
  --normalize "$REPO_ROOT/axis-surface-h1-config/surface-h1-normalize.yaml" \
  --lowering "$REPO_ROOT/axis-surface-h1-config/surface-h1-lowering.yaml" \
  --file "$SRC" \
  --reg "$REPO_ROOT/axis-surface-h1-config/surface-h1-registry.axreg" \
  --out "$REPO_ROOT/build/h1/program.coreir"

BRIDGE_BINARY="${BRIDGE_BINARY:-~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge}"

# Build with optional external library linking and entry flags.
# Usage examples:
#   ./scripts/build_axh.sh input.axh
#   ./scripts/build_axh.sh input.axh build/output --link-lib libaxis_std.a
#   ./scripts/build_axh.sh input.axh build/output --entries test_fn --entry-stack-size 65536
# "$@" forwards all args (including SRC/OUT) to the bridge; the bridge tolerates extra positional args.
"$BRIDGE_BINARY" build \
  "$REPO_ROOT/build/h1/program.coreir" \
  --out "$OUT" \
  "$@"

echo "Executable emitted at $OUT"

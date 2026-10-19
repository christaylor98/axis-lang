#!/usr/bin/env sh
set -e

# Get repository root (parent of scripts directory)
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

SRC=$1
OUT=${2:-build/surface0/app}

mkdir -p "$REPO_ROOT/build/surface0"

AXIS="${AXIS:-$REPO_ROOT/target/debug/axis}"

"$AXIS" \
  --lexer "$REPO_ROOT/axis-surface-0-config/semantic-surface-0-lexer.yaml" \
  --parser "$REPO_ROOT/axis-surface-0-config/semantic-surface-0-parse.yaml" \
  --schema "$REPO_ROOT/axis-surface-0-config/semantic-surface-0-ast.yaml" \
  --normalize "$REPO_ROOT/axis-surface-0-config/semantic-surface-0-normalize.yaml" \
  --lowering "$REPO_ROOT/axis-surface-0-config/semantic-surface-0-lowering.yaml" \
  --file "$SRC" \
  --reg "$REPO_ROOT/axis-surface-0-config/surface-0-registry.axreg" \
  --out "$REPO_ROOT/build/surface0/program.coreir"

BRIDGE_BINARY="${BRIDGE_BINARY:-~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge}"

# Build with optional entry flags and link args.
# Usage examples:
#   ./scripts/build_surface0.sh input.ax0
#   ./scripts/build_surface0.sh input.ax0 build/output --entries test_fn --entry-stack-size 65536
# "$@" forwards all args (including SRC/OUT) to the bridge; the bridge tolerates extra positional args.
"$BRIDGE_BINARY" build \
  "$REPO_ROOT/build/surface0/program.coreir" \
  --out "$OUT" \
  "$@"

echo "Executable emitted at $OUT"

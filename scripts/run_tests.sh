#!/usr/bin/env sh
# Axis test runner — Stage 0 (single-entry-per-exe mode).
#
# A test is a surface-m1 .m1 file containing an entry function.
# PASS: axis compiles it, bridge builds it, exe exits 0.
# FAIL: any step exits non-zero (compile error, bridge build error, or runtime panic).
#
# Stage-0 today: one test file -> one exe (default root entry).
# Stage-1 (once bridge ships --entries): flip to multi-entry by passing
#   --entries <fn1,fn2,...> --entry-stack-size <N> to the bridge call below.
#
# No build_m1.sh exists; this script handles M1 compilation directly.
# Stage-1 refactor: extract the compile/build block into scripts/build_m1.sh.
#
# Usage:
#   ./scripts/run_tests.sh                  # runs all tests/axtest/*.m1
#   ./scripts/run_tests.sh tests/axtest/test_pass.m1   # single file

set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

AXIS="${AXIS:-$REPO_ROOT/target/debug/axis}"
BRIDGE_BINARY="${BRIDGE_BINARY:-~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge}"

# M1 surface configuration
M1_CFG="$REPO_ROOT/axis-surface-m1-config"
LEXER="$M1_CFG/m1-lexer.yaml"
PARSER="$M1_CFG/m1-parse.yaml"
SCHEMA="$M1_CFG/m1-ast.yaml"
NORMALIZE="$M1_CFG/m1-normalize.yaml"
LOWERING="$M1_CFG/m1-lowering.yaml"
REG_M1="$M1_CFG/m1-registry.axreg"
REG_AXTEST="$REPO_ROOT/registries/axtest.axreg"

BUILD_DIR="$REPO_ROOT/build/axtest"
mkdir -p "$BUILD_DIR"

# Determine which files to run
if [ $# -gt 0 ]; then
  TEST_FILES="$@"
else
  TEST_FILES="$REPO_ROOT/tests/axtest"/*.m1
fi

pass=0
fail=0
total=0

for src in $TEST_FILES; do
  [ -f "$src" ] || { echo "  SKIP  $src (not a file)"; continue; }
  name=$(basename "$src" .m1)
  coreir="$BUILD_DIR/${name}.coreir"
  exe="$BUILD_DIR/${name}"
  total=$((total + 1))

  # ── Step 1: compile to Core IR ──────────────────────────────────
  compile_err="$BUILD_DIR/${name}.compile_err"
  if ! "$AXIS" \
    --file "$src" \
    --lexer "$LEXER" --parser "$PARSER" --schema "$SCHEMA" \
    --normalize "$NORMALIZE" --lowering "$LOWERING" \
    --reg "$REG_M1" --reg "$REG_AXTEST" \
    --out "$coreir" >"$compile_err" 2>&1; then
    echo "  FAIL  $name  (compile error)"
    sed 's/^/        /' "$compile_err"
    fail=$((fail + 1))
    continue
  fi

  # ── Step 2: bridge build → executable ───────────────────────────
  bridge_err="$BUILD_DIR/${name}.bridge_err"
  if ! "$BRIDGE_BINARY" build "$coreir" \
    --out "$exe" --exe \
    --reg "$REG_M1" --reg "$REG_AXTEST" \
    >"$bridge_err" 2>&1; then
    echo "  FAIL  $name  (bridge build error)"
    grep -E "^error:" "$bridge_err" | head -3 | sed 's/^/        /'
    fail=$((fail + 1))
    continue
  fi

  # ── Step 3: run ─────────────────────────────────────────────────
  run_out="$BUILD_DIR/${name}.run_out"
  if "$exe" >"$run_out" 2>&1; then
    echo "  PASS  $name"
    pass=$((pass + 1))
  else
    rc=$?
    echo "  FAIL  $name  (exit $rc)"
    sed 's/^/        /' "$run_out"
    fail=$((fail + 1))
  fi
done

echo ""
echo "────────────────────────────────"
echo "  $pass passed  /  $total total"
[ "$fail" -gt 0 ] && echo "  $fail FAILED"
echo "────────────────────────────────"

[ "$fail" -eq 0 ]

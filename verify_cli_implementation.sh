#!/bin/bash
# CLI Implementation Verification
# Verifies that the unified CLI implementation is complete and correct

echo "=============================================================="
echo "CLI Unified Semantics - Implementation Verification"
echo "=============================================================="
echo ""

PASS=0
FAIL=0

check() {
    if [ $? -eq 0 ]; then
        echo "  ✓ $1"
        ((PASS++))
    else
        echo "  ✗ $1"
        ((FAIL++))
    fi
}

# Check file existence
echo "Checking implementation files..."
test -f src/cli_util.rs && check "src/cli_util.rs exists"
test -f rust-bridge/src/cli_util.rs && check "rust-bridge/src/cli_util.rs exists"
test -f src/main.rs && check "src/main.rs exists"
test -f rust-bridge/src/main.rs && check "rust-bridge/src/main.rs exists"
echo ""

# Check documentation
echo "Checking documentation..."
test -f CLI_UNIFIED_SEMANTICS.md && check "CLI_UNIFIED_SEMANTICS.md exists"
test -f CLI_QUICKREF.md && check "CLI_QUICKREF.md exists"
test -f CLI_IMPLEMENTATION_SUMMARY.md && check "CLI_IMPLEMENTATION_SUMMARY.md exists"
test -f CLI_README.md && check "CLI_README.md exists"
echo ""

# Check compilation
echo "Checking compilation..."
cd ~/dev/axis-lang-lab-working

cargo build --bin axis --quiet 2>&1 > /dev/null
check "axis binary compiles"

cd rust-bridge
cargo build --bin axis-rust-bridge --quiet 2>&1 > /dev/null
check "rust-bridge binary compiles"
cd ..
echo ""

# Check tests
echo "Running unit tests..."
cargo test --lib cli_util --quiet 2>&1 > /dev/null
check "axis cli_util tests pass"

cd rust-bridge
cargo test --lib cli_util --quiet 2>&1 > /dev/null
check "rust-bridge cli_util tests pass"
cd ..
echo ""

# Check CLI behavior
echo "Checking CLI behavior..."

# Axis should reject missing --src
./target/debug/axis --reg test.yaml 2>&1 | grep -q "At least one --src is required"
check "axis rejects missing --src"

# Axis should reject missing --reg
./target/debug/axis --src test.ax 2>&1 | grep -q "At least one --reg is required"
check "axis rejects missing --reg"

# Rust bridge should reject missing --coreir (in non-inspect mode)
./rust-bridge/target/debug/axis-rust-bridge --name test 2>&1 | grep -q "At least one --coreir is required"
check "rust-bridge rejects missing --coreir"

# Both should show help
./target/debug/axis --help | grep -q "Usage: axis"
check "axis shows help"

./rust-bridge/target/debug/axis-rust-bridge --help | grep -q "Usage: axis-rust-bridge"
check "rust-bridge shows help"
echo ""

# Summary
echo "=============================================================="
echo "Verification Summary"
echo "=============================================================="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [ $FAIL -eq 0 ]; then
    echo "✓ All checks passed - implementation is complete!"
    exit 0
else
    echo "✗ Some checks failed - review output above"
    exit 1
fi

#!/bin/bash
# CLI Unified Semantics - Demonstration Script
# This script demonstrates the new CLI behavior

set -e

echo "==================================================================="
echo "CLI Unified Semantics - Demonstration"
echo "==================================================================="
echo ""

# Build binaries
echo "Building binaries..."
cd ~/dev/axis-lang-lab-working
cargo build --bin axis --quiet
cd rust-bridge
cargo build --bin axis-rust-bridge --quiet
cd ..
echo "✓ Build complete"
echo ""

# Axis CLI Tests
echo "-------------------------------------------------------------------"
echo "AXIS COMPILER CLI"
echo "-------------------------------------------------------------------"
echo ""

echo "1. Show help:"
echo "$ axis --help"
./target/debug/axis --help
echo ""

echo "2. Test error: no --src provided"
echo "$ axis --reg test.yaml"
./target/debug/axis --reg test.yaml 2>&1 || echo "Exit code: $?"
echo ""

echo "3. Test error: no --reg provided"
echo "$ axis --src test.ax"
./target/debug/axis --src test.ax 2>&1 || echo "Exit code: $?"
echo ""

echo "4. Test error: file doesn't exist"
echo "$ axis --src nonexistent.ax --reg nonexistent.yaml"
./target/debug/axis --src nonexistent.ax --reg nonexistent.yaml 2>&1 || echo "Exit code: $?"
echo ""

# Rust Bridge CLI Tests
echo "-------------------------------------------------------------------"
echo "RUST BRIDGE CLI"
echo "-------------------------------------------------------------------"
echo ""

echo "1. Show help:"
echo "$ axis-rust-bridge --help"
./rust-bridge/target/debug/axis-rust-bridge --help
echo ""

echo "2. Test error: no --coreir provided"
echo "$ axis-rust-bridge --name test"
./rust-bridge/target/debug/axis-rust-bridge --name test 2>&1 || echo "Exit code: $?"
echo ""

echo "3. Test error: file doesn't exist"
echo "$ axis-rust-bridge --coreir nonexistent.coreir"
./rust-bridge/target/debug/axis-rust-bridge --coreir nonexistent.coreir 2>&1 || echo "Exit code: $?"
echo ""

echo "==================================================================="
echo "Demonstration Complete"
echo "==================================================================="
echo ""
echo "Key Features Demonstrated:"
echo "  ✓ Clear help text"
echo "  ✓ Required flag validation"
echo "  ✓ Path existence checking"
echo "  ✓ Informative error messages"
echo ""
echo "Note: Pipeline integration is pending - actual compilation/linking"
echo "      will fail with a clear message indicating integration needed."

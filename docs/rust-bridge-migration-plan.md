# Bridge Migration — `axis-codegen-bridge` (standalone, Core IR 0.5)

**Status: COMPLETE as of Phase 6.**

The legacy in-tree `rust-bridge/` (crate `axis-rust-bridge`) has been deleted.
All consumers now point at the external standalone crate
`~/dev/axis-codegen-bridge-rs` (`axis-codegen-bridge`),
which has been aligned to Core IR 0.5.

## What changed (summary)

| Consumer | Before | After |
|---|---|---|
| `scripts/build_ai1.sh` | `cargo build` in `rust-bridge/` + `axis-rust-bridge` binary | `$BRIDGE_BINARY` (default: `~/dev/axis-codegen-bridge-rs/target/release/axis-codegen-bridge`) |
| `scripts/build_ai2.sh` | same | same |
| `scripts/build_surface0.sh` | same | same |
| `scripts/build_axh.sh` | same | same |
| `build/ai2/generated.rs` | `extern crate axis_rust_bridge; runtime::shim` | `extern crate axis_codegen_bridge;` (no shim) |
| `oper/discover_reg_script.sh` | `BRIDGE_DIR="$ROOT_DIR/rust-bridge/src"` | `$BRIDGE_SRC_DIR` (default: `~/dev/axis-codegen-bridge-rs/src`) |

## Core IR version

The project now targets **Core IR 0.5** end-to-end:

- `axis` compiler emits 0.5 bundles (flat indexed table: constant_pool + node list).
- `axis-codegen-bridge` loads 0.5 bundles via its `core_ir_05` module (Cap'n Proto schema `@0xa4e7f1b8c93d0521`).
- 0.4 bundle support is retained in the bridge as a fallback; the bridge auto-detects the version.

## How to use the bridge

**Override the binary path** (e.g., after installing or cross-compiling):

```sh
BRIDGE_BINARY=/path/to/axis-codegen-bridge ./scripts/build_ai2.sh input.ax
```

**Override the bridge source path** for the registry discovery script:

```sh
BRIDGE_SRC_DIR=/path/to/axis-codegen-bridge-rs/src ./oper/discover_reg_script.sh
```

**Build the external bridge** (needed before running any script):

```sh
cd ~/dev/axis-codegen-bridge-rs
cargo build --release
```

## What was removed / superseded

- `rust-bridge/` directory — deleted (Phase 3d).
- `runtime::shim` — never existed in `axis-codegen-bridge`; removed from generated code.
- `registry_shim_coverage.json` — "shim coverage" concept superseded by `--reg` registry validation.
- The 0.4-era migration plan phases (0–6 in the old plan) — all complete.

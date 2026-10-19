// axis_mcp: read-only MCP shell over the `axis` binary.
//
// Bootstrapped under AXIS_M1_MCP_V1 with a single hand-authored tool,
// m1_validate. AXIS_M1_MCP_V2 explicitly governed four further additions —
// m1_verify_diag, m1_type_resolve, m1_symbols, m1_find — each grounded
// against the running binary before being added (see conversation record).
// Any further tool beyond these five requires revisiting the
// MANIFEST_REFLECTED_ONLY exception again; the fix for "we need another
// tool" is a manifest format + reflection layer, not an ungoverned #[tool] fn.
//
// This shell authors no pipeline logic of its own: every tool shells out to
// the already-verified `axis` binary (subcommands: default pipeline, `verify`,
// `types`, `symbols`, `find`) and reports bounded, structured text. No LLM
// calls in the response path. m1_verify_diag writes a Core IR bundle to an
// ephemeral scratch directory outside the repo (required to obtain
// full-hash diagnostics — see below) and removes it before returning; no
// repo file, registry, or pipeline state is ever touched.

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct M1ValidateArgs {
    /// Path to an M1 source file (.m1), relative to the repo root.
    file: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct M1VerifyDiagArgs {
    /// Path to an M1 source file (.m1), relative to the repo root.
    file: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct M1TypeResolveArgs {
    /// 64-hex-char (optionally 0x-prefixed) Hash256 type identity, as it
    /// appears (truncated) in an m1_validate/m1_verify_diag diagnostic.
    hash: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct M1SymbolsArgs {
    /// Case-insensitive substring to match against every projected type/fn
    /// name in the effective flat registry set. Omit (or pass "") to list
    /// all names, capped at 50 with a truncation notice.
    #[serde(default)]
    query: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct M1FindArgs {
    /// Output type identity (64-hex-char Hash256, as from m1_type_resolve).
    out: String,
    /// Ordered input type identities (64-hex-char Hash256 each), comma
    /// separated. Omit (or pass "") for a nullary signature.
    #[serde(default)]
    r#in: Option<String>,
}

/// The three registries the M1 baseline (CLAUDE.md) declares as the
/// effective flat set. Every tool here searches only this set.
const M1_REGISTRIES: [&str; 3] = [
    "axis-surface-m1-config/m1-registry.axreg",
    "registries/pilot_lowering.axreg",
    "registries/fnref_test.axreg",
];

#[derive(Clone)]
struct AxisMcp {
    tool_router: ToolRouter<AxisMcp>,
}

impl AxisMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve the sibling `axis` binary and repo root from this binary's own
    /// path. Cargo always places both binaries in the same target/<profile>/
    /// directory, so this never depends on the caller's working directory.
    fn locate() -> Result<(PathBuf, PathBuf), McpError> {
        let exe = std::env::current_exe().map_err(|e| {
            McpError::internal_error(format!("cannot resolve own executable path: {e}"), None)
        })?;
        let bin_dir = exe
            .parent()
            .ok_or_else(|| McpError::internal_error("executable has no parent directory", None))?;
        let axis_bin = bin_dir.join("axis");
        if !axis_bin.is_file() {
            return Err(McpError::internal_error(
                format!(
                    "axis binary not found at {}; run `cargo build --bin axis` first",
                    axis_bin.display()
                ),
                None,
            ));
        }
        let repo_root = bin_dir
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| {
                McpError::internal_error("cannot resolve repo root from executable path", None)
            })?
            .to_path_buf();
        if !repo_root.join("axis-surface-m1-config").is_dir() {
            return Err(McpError::internal_error(
                format!(
                    "resolved repo root {} does not contain axis-surface-m1-config; refusing to guess",
                    repo_root.display()
                ),
                None,
            ));
        }
        Ok((axis_bin, repo_root))
    }

    /// Append the effective flat registry set to a command, as repo-root-relative
    /// paths (matches `locate()`'s `current_dir(&repo_root)`).
    fn add_reg_args(cmd: &mut Command) {
        for reg in M1_REGISTRIES {
            cmd.arg("--reg").arg(reg);
        }
    }

    /// A scratch directory outside the repo, named without a `.` (the `axis`
    /// CLI's `--out` flag treats any path with a dot as a file, not a
    /// directory, so a dotted mktemp-style name would silently misroute).
    /// Caller is responsible for removing it.
    fn make_scratch_dir() -> Result<PathBuf, McpError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("axis_mcp_{}_{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).map_err(|e| {
            McpError::internal_error(format!("cannot create scratch dir: {e}"), None)
        })?;
        Ok(dir)
    }
}

#[tool_router]
impl AxisMcp {
    #[tool(
        description = "Validate an M1 source file through the full Axis pipeline (lex, parse, normalize, lower, verify). Returns PASS or FAIL and the first diagnostic line only. Read-only: runs the existing `axis` binary as a subprocess against the fixed M1 surface config; does not modify any files and does not invoke an LLM."
    )]
    fn m1_validate(
        &self,
        Parameters(M1ValidateArgs { file }): Parameters<M1ValidateArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (axis_bin, repo_root) = Self::locate()?;

        let source = repo_root.join(&file);
        if !source.is_file() {
            return Err(McpError::invalid_params(
                format!("no such file: {}", source.display()),
                None,
            ));
        }

        let mut cmd = Command::new(&axis_bin);
        cmd.current_dir(&repo_root)
            .arg("--file")
            .arg(&file)
            .arg("--lexer")
            .arg("axis-surface-m1-config/m1-lexer.yaml")
            .arg("--parser")
            .arg("axis-surface-m1-config/m1-parse.yaml")
            .arg("--schema")
            .arg("axis-surface-m1-config/m1-ast.yaml")
            .arg("--normalize")
            .arg("axis-surface-m1-config/m1-normalize.yaml");
        Self::add_reg_args(&mut cmd);
        let output = cmd.output().map_err(|e| {
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}");
        // "→" is the PASS marker used by CLAUDE.md's mandatory M1 baseline check.
        let verdict = if combined.contains('\u{2192}') {
            "PASS"
        } else {
            "FAIL"
        };
        let detail = combined.lines().next().unwrap_or("").trim();

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{verdict}  {file}: {detail}"
        ))]))
    }

    #[tool(
        description = "Structured verify diagnostic for an M1 source file. Like m1_validate, but on a verification FAIL the diagnostic contains FULL 64-hex-char type identity hashes (not truncated), suitable for feeding directly to m1_type_resolve. Read-only over the repo: internally writes the intermediate Core IR bundle to an OS scratch directory outside the repo (required to obtain full hashes — the axis binary only prints full hashes for a bundle already on disk) and deletes it before returning. No repo file, registry, or pipeline state is modified. No LLM calls."
    )]
    fn m1_verify_diag(
        &self,
        Parameters(M1VerifyDiagArgs { file }): Parameters<M1VerifyDiagArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (axis_bin, repo_root) = Self::locate()?;

        let source = repo_root.join(&file);
        if !source.is_file() {
            return Err(McpError::invalid_params(
                format!("no such file: {}", source.display()),
                None,
            ));
        }

        let scratch = Self::make_scratch_dir()?;
        let cleanup = |dir: &Path| {
            let _ = std::fs::remove_dir_all(dir);
        };

        // Step 1: compile with --no-verify so a bundle is written to the
        // scratch dir even for programs that would fail verification (the
        // normal pipeline exits before writing on verify failure).
        let mut compile_cmd = Command::new(&axis_bin);
        compile_cmd
            .current_dir(&repo_root)
            .arg("--file")
            .arg(&file)
            .arg("--lexer")
            .arg("axis-surface-m1-config/m1-lexer.yaml")
            .arg("--parser")
            .arg("axis-surface-m1-config/m1-parse.yaml")
            .arg("--schema")
            .arg("axis-surface-m1-config/m1-ast.yaml")
            .arg("--normalize")
            .arg("axis-surface-m1-config/m1-normalize.yaml")
            .arg("--no-verify")
            .arg("--out")
            .arg(&scratch);
        Self::add_reg_args(&mut compile_cmd);
        let compile_out = compile_cmd.output().map_err(|e| {
            cleanup(&scratch);
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        if !compile_out.status.success() {
            // Lex/parse/normalize/lowering-stage failure — same failure class
            // m1_validate would report; no bundle exists to verify further.
            let stdout = String::from_utf8_lossy(&compile_out.stdout);
            let stderr = String::from_utf8_lossy(&compile_out.stderr);
            let combined = format!("{stdout}{stderr}");
            let detail = combined.lines().next().unwrap_or("").trim().to_string();
            cleanup(&scratch);
            return Ok(CallToolResult::success(vec![ContentBlock::text(format!(
                "FAIL  {file}: {detail}"
            ))]));
        }

        let bundle_path = scratch.join(format!(
            "{}.coreir",
            Path::new(&file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("out")
        ));
        if !bundle_path.is_file() {
            cleanup(&scratch);
            return Err(McpError::internal_error(
                format!("expected compiled bundle at {} was not found", bundle_path.display()),
                None,
            ));
        }

        // Step 2: verify the bundle standalone, where VerifyError's Display
        // impl (unlike the main pipeline's log line) prints full hashes.
        let mut verify_cmd = Command::new(&axis_bin);
        verify_cmd
            .current_dir(&repo_root)
            .arg("verify")
            .arg(&bundle_path);
        Self::add_reg_args(&mut verify_cmd);
        let verify_out = verify_cmd.output().map_err(|e| {
            cleanup(&scratch);
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        cleanup(&scratch);

        let stdout = String::from_utf8_lossy(&verify_out.stdout);
        let stderr = String::from_utf8_lossy(&verify_out.stderr);
        let combined = format!("{stdout}{stderr}");
        let detail = combined.lines().next().unwrap_or("").trim();
        let verdict = if verify_out.status.success() { "PASS" } else { "FAIL" };

        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "{verdict}  {file}: {detail}"
        ))]))
    }

    #[tool(
        description = "Resolve a Hash256 type identity (as seen, full-length, in an m1_verify_diag diagnostic) to its structural TypeDef shape, one level deep. Nested Product/Sum/List element identities are returned as hex — call again on a child hash to descend. Also reports the projected name if the registries assign one (e.g. 'Bool', 'Unit'). Returns UNKNOWN, never a fabricated shape, if the hash is not present in the effective flat registry set. Read-only: only reads the given registry files via the axis binary; no LLM calls."
    )]
    fn m1_type_resolve(
        &self,
        Parameters(M1TypeResolveArgs { hash }): Parameters<M1TypeResolveArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (axis_bin, repo_root) = Self::locate()?;

        let mut cmd = Command::new(&axis_bin);
        cmd.current_dir(&repo_root).arg("types").arg(&hash);
        Self::add_reg_args(&mut cmd);
        let output = cmd.output().map_err(|e| {
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}");
        let detail = combined.lines().next().unwrap_or("").trim();

        Ok(CallToolResult::success(vec![ContentBlock::text(
            detail.to_string(),
        )]))
    }

    #[tool(
        description = "Case-insensitive substring search over every projected type/fn name in the effective flat registry set. Each match reports its kind (TYPE/FN), full hash identity, and — for fns — the input/output type-identity signature. Bounded to 50 matches; if more exist, a final line reports the drop count so truncation is never silent. Use this before authoring a new fn or type to check whether the name is already taken. Read-only: only reads the given registry files via the axis binary; no LLM calls."
    )]
    fn m1_symbols(
        &self,
        Parameters(M1SymbolsArgs { query }): Parameters<M1SymbolsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (axis_bin, repo_root) = Self::locate()?;

        let mut cmd = Command::new(&axis_bin);
        cmd.current_dir(&repo_root).arg("symbols");
        if let Some(q) = query.filter(|q| !q.is_empty()) {
            cmd.arg(q);
        }
        Self::add_reg_args(&mut cmd);
        let output = cmd.output().map_err(|e| {
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}").trim().to_string();
        let text = if combined.is_empty() {
            "NO_MATCH: no name in the effective flat registry set matches this query".to_string()
        } else {
            combined
        };

        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }

    #[tool(
        description = "Exact-match contract search (the dedup gate — consult before authoring a new fn under any name): finds every fn in the effective flat registry set whose (inputs, output) signature exactly equals the one given, regardless of name. Both `out` and each entry of `in` are full 64-hex-char Hash256 type identities, as returned by m1_type_resolve or m1_symbols. Omit `in` for a nullary signature. Returns NO_MATCH, never a fabricated result, if nothing matches. Read-only: only reads the given registry files via the axis binary; no LLM calls."
    )]
    fn m1_find(
        &self,
        Parameters(M1FindArgs { out, r#in }): Parameters<M1FindArgs>,
    ) -> Result<CallToolResult, McpError> {
        let (axis_bin, repo_root) = Self::locate()?;

        let mut cmd = Command::new(&axis_bin);
        cmd.current_dir(&repo_root).arg("find").arg("--out").arg(&out);
        if let Some(ins) = r#in.filter(|s| !s.is_empty()) {
            cmd.arg("--in").arg(ins);
        }
        Self::add_reg_args(&mut cmd);
        let output = cmd.output().map_err(|e| {
            McpError::internal_error(format!("failed to run axis binary: {e}"), None)
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}{stderr}").trim().to_string();
        let text = if combined.is_empty() {
            "NO_MATCH: no fn with this exact signature in given registries".to_string()
        } else {
            combined
        };

        Ok(CallToolResult::success(vec![ContentBlock::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for AxisMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_instructions(
                "Read-only mechanical oracle over the Axis-M1 pipeline. Tools: \
                 m1_validate(file) - PASS/FAIL + first diagnostic line; \
                 m1_verify_diag(file) - like m1_validate but with FULL type-hash \
                 diagnostics on FAIL; m1_type_resolve(hash) - resolve a full \
                 type-identity hash to its structural shape and name; \
                 m1_symbols(query?) - substring search over type/fn names \
                 (name-collision check before authoring); m1_find(out, in?) - \
                 exact fn-signature search regardless of name (the dedup gate \
                 before authoring). No writes, no LLM calls, bounded output."
                    .to_string(),
            )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let service = AxisMcp::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

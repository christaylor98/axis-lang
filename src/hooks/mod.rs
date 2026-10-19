// WAVE 4: Compiler Hook Framework
//
// This module implements the compiler hook system as specified in:
// - AXIS_COMPILER_HOOKS_AND_PLUGINS_SPECIFICATION_0.1.md
// - ARCHITECTURE.md (Section 7)
//
// SCOPE:
// - Hook registration and resolution
// - Stage enumeration (pre-lex, post-parse, normalisation, post-normalisation)
// - Deterministic ordering
// - Conflict detection
// - Permission enforcement (read-only vs rewrite)
// - Pipeline integration (structural only)
// - Inspection support
//
// OUT OF SCOPE (HARD BOUNDARY):
// - Semantics
// - Lowering hooks
// - Core IR hooks
// - Registry access
// - User-facing DSL
// - Desugaring changes
//
// ARCHITECTURAL INVARIANTS:
// - Hooks must not affect meaning
// - Hooks must not bypass normalisation or NF validation
// - Lowering remains fixed and sealed
// - All hook effects must be observable

pub mod execution;
pub mod framework;
pub mod registry;

pub use execution::{HookExecution, HookExecutionTrace, HookExecutor};
pub use framework::{
    Hook, HookContext, HookError, HookExecutionResult, HookInput, HookOutput, HookPermission,
    HookResult, HookStage,
};
pub use registry::{HookConflict, HookRegistry, OrderingHint};

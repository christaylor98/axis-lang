// Hook Execution - Tracing and Inspection
//
// Provides execution tracing for hooks, enabling full observability
// of hook invocations during compilation.

use super::framework::{Hook, HookContext, HookExecutionResult, HookInput, HookOutput, HookStage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// HOOK EXECUTION RECORD
// ═══════════════════════════════════════════════════════════════════════════

/// Record of a single hook execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecution {
    /// Hook name
    pub hook_name: String,

    /// Pipeline stage
    pub stage: String,

    /// Normalisation pass (if applicable)
    pub normalisation_pass: Option<String>,

    /// Permission level
    pub permission: String,

    /// Execution duration
    #[serde(skip)]
    pub duration: Duration,

    /// Duration in milliseconds (for serialization)
    pub duration_ms: f64,

    /// Whether the hook modified its input
    pub modified: bool,

    /// Error message (if execution failed)
    pub error: Option<String>,

    /// Optional metadata from the hook
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl HookExecution {
    /// Create a new execution record
    pub fn new(
        hook_name: String,
        stage: HookStage,
        normalisation_pass: Option<String>,
        permission: String,
        duration: Duration,
        modified: bool,
        error: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            hook_name,
            stage: format!("{}", stage),
            normalisation_pass,
            permission,
            duration,
            duration_ms: duration.as_secs_f64() * 1000.0,
            modified,
            error,
            metadata,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// EXECUTION TRACE
// ═══════════════════════════════════════════════════════════════════════════

/// Complete trace of all hook executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookExecutionTrace {
    /// All hook executions in order
    pub executions: Vec<HookExecution>,

    /// Total time spent in hooks
    #[serde(skip)]
    pub total_duration: Duration,

    /// Total duration in milliseconds
    pub total_duration_ms: f64,

    /// Number of hooks that modified their input
    pub modifications: usize,

    /// Number of failed hooks
    pub failures: usize,
}

impl HookExecutionTrace {
    /// Create a new empty trace
    pub fn new() -> Self {
        Self {
            executions: Vec::new(),
            total_duration: Duration::ZERO,
            total_duration_ms: 0.0,
            modifications: 0,
            failures: 0,
        }
    }

    /// Add an execution record
    pub fn add_execution(&mut self, execution: HookExecution) {
        self.total_duration += execution.duration;
        self.total_duration_ms = self.total_duration.as_secs_f64() * 1000.0;

        if execution.modified {
            self.modifications += 1;
        }

        if execution.error.is_some() {
            self.failures += 1;
        }

        self.executions.push(execution);
    }

}

impl Default for HookExecutionTrace {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// HOOK EXECUTOR
// ═══════════════════════════════════════════════════════════════════════════

/// Executes a hook and records execution metadata
pub struct HookExecutor;

impl HookExecutor {
    /// Execute a hook with full tracing
    ///
    /// Records:
    /// - Execution time
    /// - Whether input was modified
    /// - Any errors
    /// - Hook metadata
    pub fn execute(
        hook: &Arc<dyn Hook>,
        input: HookInput,
        context: &HookContext,
    ) -> (HookExecutionResult, HookExecution) {
        let start = Instant::now();

        // Clone input for modification detection
        let input_clone = input.clone();

        // Execute hook
        let result = hook.execute(input, context);

        let duration = start.elapsed();

        // Determine if modified
        let modified = match (&result, &input_clone) {
            (Ok(hook_result), HookInput::Text(original_text)) => match &hook_result.output {
                HookOutput::Text(new_text) => new_text != original_text,
                _ => false,
            },
            (Ok(hook_result), HookInput::Ast(_original_ast)) => {
                match &hook_result.output {
                    HookOutput::Ast(_new_ast) => {
                        // For AST, we consider it modified if the hook ran
                        // (precise equality check would require deep comparison)
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        };

        // Extract metadata and error
        let (error, metadata) = match &result {
            Ok(hook_result) => (None, hook_result.metadata.clone()),
            Err(e) => (Some(e.message.clone()), None),
        };

        // Create execution record
        let execution = HookExecution::new(
            hook.name().to_string(),
            context.stage,
            context.normalisation_pass.clone(),
            format!("{}", context.permission),
            duration,
            modified,
            error,
            metadata,
        );

        (result, execution)
    }

    /// Execute multiple hooks in sequence
    ///
    /// Returns:
    /// - Final output (or error from first failed hook)
    /// - Complete execution trace
    pub fn execute_sequence(
        hooks: Vec<Arc<dyn Hook>>,
        mut input: HookInput,
        context: &HookContext,
    ) -> (Result<HookOutput, String>, HookExecutionTrace) {
        let mut trace = HookExecutionTrace::new();

        for hook in hooks {
            let (result, execution) = Self::execute(&hook, input.clone(), context);
            trace.add_execution(execution.clone());

            match result {
                Ok(hook_result) => {
                    // Update input for next hook
                    input = match hook_result.output {
                        HookOutput::Text(text) => HookInput::Text(text),
                        HookOutput::Ast(ast) => HookInput::Ast(ast),
                        HookOutput::Metadata(_) => {
                            // Read-only hook, keep original input
                            input
                        }
                    };
                }
                Err(e) => {
                    // Hook failed, stop execution
                    return (Err(e.message), trace);
                }
            }
        }

        // Extract final output
        let final_output = match input {
            HookInput::Text(text) => HookOutput::Text(text),
            HookInput::Ast(ast) => HookOutput::Ast(ast),
        };

        (Ok(final_output), trace)
    }
}


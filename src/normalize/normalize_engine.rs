// Normalization Engine
//
// PURPOSE:
// Execute normalization rewrites according to loaded rules
//
// SCOPE:
// - Bottom-up AST traversal
// - Rule pattern matching
// - AST rewriting
// - Deterministic rule application
//
// INVARIANTS:
// - First matching rule wins
// - No match → HARD ERROR
// - Bottom-up traversal (children before parents)
// - Single rule application per node

use crate::frontend::schema_ast::{SchemaAstNode, SchemaValue};
use crate::frontend::token::Span;
use crate::normalize::rules_loader::{
    FieldMapping, NormalizationRules, ReplacementFields, RulePattern,
};
use std::collections::HashMap;

pub struct NormalizationEngine {
    rules: NormalizationRules,
}

#[derive(Debug)]
pub struct RewriteError {
    pub message: String,
    pub node_kind: String,
    pub span: Span,
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rewrite error on '{}' at {}..{}: {}",
            self.node_kind, self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for RewriteError {}

impl NormalizationEngine {
    pub fn new(rules: NormalizationRules) -> Self {
        NormalizationEngine { rules }
    }

    /// Apply normalization rewrites to AST (bottom-up)
    pub fn normalize(&self, ast: SchemaAstNode) -> Result<SchemaAstNode, RewriteError> {
        self.normalize_node(ast)
    }

    fn normalize_node(&self, mut node: SchemaAstNode) -> Result<SchemaAstNode, RewriteError> {
        // BOTTOM-UP: Normalize children first
        node.fields = self.normalize_fields(node.fields, &node.span)?;

        // Find matching rule (first match wins)
        let matching_rule = self.find_matching_rule(&node);

        match matching_rule {
            Some(rule_name) => {
                let rule = &self.rules.rules[&rule_name];
                self.apply_rule(&node, rule)
            }
            None => {
                // NO MATCH → HARD ERROR
                Err(RewriteError {
                    message: format!(
                        "No normalization rule matches node kind '{}'. Available rules: [{}]",
                        node.kind,
                        self.rules.rule_order.join(", ")
                    ),
                    node_kind: node.kind.clone(),
                    span: node.span.clone(),
                })
            }
        }
    }

    fn normalize_fields(
        &self,
        fields: HashMap<String, SchemaValue>,
        _span: &Span,
    ) -> Result<HashMap<String, SchemaValue>, RewriteError> {
        let mut normalized = HashMap::new();

        for (name, value) in fields {
            let normalized_value = match value {
                SchemaValue::Node(boxed_node) => {
                    let normalized_node = self.normalize_node(*boxed_node)?;
                    SchemaValue::Node(Box::new(normalized_node))
                }
                SchemaValue::Nodes(nodes) => {
                    let mut normalized_nodes = Vec::new();
                    for node in nodes {
                        normalized_nodes.push(self.normalize_node(node)?);
                    }
                    SchemaValue::Nodes(normalized_nodes)
                }
                SchemaValue::Token(t) => SchemaValue::Token(t),
                SchemaValue::Tokens(ts) => SchemaValue::Tokens(ts),
            };
            normalized.insert(name, normalized_value);
        }

        Ok(normalized)
    }

    fn find_matching_rule(&self, node: &SchemaAstNode) -> Option<String> {
        // Iterate through rules in declared order
        for rule_name in &self.rules.rule_order {
            if let Some(rule) = self.rules.rules.get(rule_name) {
                if self.pattern_matches(&rule.pattern, node) {
                    return Some(rule_name.clone());
                }
            }
        }
        None
    }

    fn pattern_matches(&self, pattern: &RulePattern, node: &SchemaAstNode) -> bool {
        match pattern {
            RulePattern::SingleKind {
                node_kind,
                fields: field_constraints,
            } => {
                // Check node kind match
                if &node.kind != node_kind {
                    return false;
                }

                // Check field constraints
                for (field_name, constraint) in field_constraints {
                    if constraint.exists {
                        if !node.fields.contains_key(field_name) {
                            return false;
                        }

                        let field_value = &node.fields[field_name];

                        if constraint.is_token {
                            if !matches!(field_value, SchemaValue::Token(_)) {
                                return false;
                            }
                        }

                        if constraint.is_node {
                            if !matches!(field_value, SchemaValue::Node(_)) {
                                return false;
                            }
                        }

                        if constraint.is_list {
                            if !matches!(
                                field_value,
                                SchemaValue::Nodes(_) | SchemaValue::Tokens(_)
                            ) {
                                return false;
                            }
                        }
                    }
                }

                true
            }
            RulePattern::MultipleKinds {
                node_kind_matches, ..
            } => node_kind_matches.contains(&node.kind),
        }
    }

    fn apply_rule(
        &self,
        node: &SchemaAstNode,
        rule: &crate::normalize::rules_loader::RewriteRule,
    ) -> Result<SchemaAstNode, RewriteError> {
        let nf_kind = match rule.replacement.nf_node.as_str() {
            "preserve_kind" | "identity" => node.kind.clone(),
            "transparent" => {
                // Transparent nodes unwrap to their inner content
                return self.unwrap_transparent_node(node);
            }
            "unwrap_program_items" | "extract_first_item" | "extract_first_decl" => {
                // Single item → unwrap; multi-item → Program{items}
                return self.unwrap_program_items(node);
            }
            "extract_first_element" => {
                // Extract first element from a single-element list field
                return self.extract_first_element(node);
            }
            "fold_body_to_let_chain" => {
                // Body{bindings, result} → nested LetExpr (or just result if 0 bindings)
                return self.fold_body_to_let_chain(node);
            }
            "fold_stmts_to_let_chain" => {
                // BlockContent{items: [LetBinding*, result_expr]} → nested LetExpr chain
                return self.fold_stmts_to_let_chain(node);
            }
            other => other.to_string(),
        };
        let nf_fields = self.build_nf_fields(node, &rule.replacement.nf_fields)?;

        Ok(SchemaAstNode {
            kind: nf_kind,
            fields: nf_fields,
            annotations: node.annotations.clone(),
            span: node.span.clone(),
        })
    }

    fn unwrap_transparent_node(&self, node: &SchemaAstNode) -> Result<SchemaAstNode, RewriteError> {
        // Transparent nodes have a single field that is the unwrapped content
        // Common patterns: variant, content, expr, value, inner, etc.

        // Try common unwrap field names
        for field_name in &["variant", "content", "expr", "value", "inner", "body", "left", "target"] {
            if let Some(SchemaValue::Node(inner)) = node.fields.get(*field_name) {
                return Ok((**inner).clone());
            }
        }

        Err(RewriteError {
            message: format!(
                "Transparent node '{}' has no unwrappable field (tried: variant, content, expr, value, inner, body, left, target)",
                node.kind
            ),
            node_kind: node.kind.clone(),
            span: node.span.clone(),
        })
    }

    fn unwrap_program_items(
        &self,
        node: &SchemaAstNode,
    ) -> Result<SchemaAstNode, RewriteError> {
        // Extract items/decls array (Program nodes contain items or decls)
        let items_field = node
            .fields
            .get("items")
            .or_else(|| node.fields.get("decls"));

        let items = match items_field {
            Some(SchemaValue::Nodes(nodes)) => nodes,
            _ => {
                return Err(RewriteError {
                    message: format!(
                        "Program node '{}' missing 'items' or 'decls' field",
                        node.kind
                    ),
                    node_kind: node.kind.clone(),
                    span: node.span.clone(),
                })
            }
        };

        if items.is_empty() {
            return Err(RewriteError {
                message: "Program must contain at least one item (empty programs not supported)"
                    .to_string(),
                node_kind: node.kind.clone(),
                span: node.span.clone(),
            });
        }

        // Single item: unwrap to the item directly (backward-compatible)
        if items.len() == 1 {
            return Ok(items[0].clone());
        }

        // Multi-item: pipeline splits Program before normalization; reaching here is a bug
        Err(RewriteError {
            message: format!(
                "Program with {} items reached normalizer — pipeline must split before normalization",
                items.len()
            ),
            node_kind: node.kind.clone(),
            span: node.span.clone(),
        })
    }

    fn fold_body_to_let_chain(
        &self,
        body_node: &SchemaAstNode,
    ) -> Result<SchemaAstNode, RewriteError> {
        // Extract bindings (already-normalized LetBinding list) and result
        let bindings: &[SchemaAstNode] = match body_node.fields.get("bindings") {
            Some(SchemaValue::Nodes(nodes)) => nodes.as_slice(),
            None => &[],
            _ => &[],
        };

        let result: SchemaAstNode = match body_node.fields.get("result") {
            Some(SchemaValue::Node(n)) => (**n).clone(),
            _ => {
                return Err(RewriteError {
                    message: "Body missing 'result' node field".to_string(),
                    node_kind: body_node.kind.clone(),
                    span: body_node.span.clone(),
                })
            }
        };

        // Zero bindings: fold to just the result (no LetExpr wrapper)
        if bindings.is_empty() {
            return Ok(result);
        }

        // n bindings → n nested LetExpr, right-to-left (innermost body = result)
        let mut current = result;
        for binding in bindings.iter().rev() {
            let name_tok = match binding.fields.get("name") {
                Some(SchemaValue::Token(t)) => t.clone(),
                _ => {
                    return Err(RewriteError {
                        message: "LetBinding missing 'name' token field".to_string(),
                        node_kind: binding.kind.clone(),
                        span: binding.span.clone(),
                    })
                }
            };
            let value_node = match binding.fields.get("value") {
                Some(SchemaValue::Node(n)) => n.clone(),
                _ => {
                    return Err(RewriteError {
                        message: "LetBinding missing 'value' node field".to_string(),
                        node_kind: binding.kind.clone(),
                        span: binding.span.clone(),
                    })
                }
            };

            let mut let_fields = std::collections::HashMap::new();
            let_fields.insert("name".to_string(), SchemaValue::Token(name_tok));
            let_fields.insert("value".to_string(), SchemaValue::Node(value_node));
            let_fields.insert("body".to_string(), SchemaValue::Node(Box::new(current)));

            current = SchemaAstNode {
                kind: "LetExpr".to_string(),
                fields: let_fields,
                annotations: binding.annotations.clone(),
                span: binding.span.clone(),
            };
        }

        Ok(current)
    }

    fn fold_stmts_to_let_chain(
        &self,
        block_content: &SchemaAstNode,
    ) -> Result<SchemaAstNode, RewriteError> {
        // BlockContent{items: [LetBinding*, result_expr]} → nested LetExpr chain.
        // Bottom-up: children are already normalized, so AssignStmts are LetBindings.
        let items: &[SchemaAstNode] = match block_content.fields.get("items") {
            Some(SchemaValue::Nodes(nodes)) => nodes.as_slice(),
            _ => {
                return Err(RewriteError {
                    message: "BlockContent missing 'items' list field".to_string(),
                    node_kind: block_content.kind.clone(),
                    span: block_content.span.clone(),
                })
            }
        };

        if items.is_empty() {
            return Err(RewriteError {
                message: "BlockContent is empty — a function body must have at least one expression".to_string(),
                node_kind: block_content.kind.clone(),
                span: block_content.span.clone(),
            });
        }

        // Last item is the result expression.
        let result = items.last().unwrap().clone();

        // All preceding items must be LetBinding nodes.
        let bindings = &items[..items.len() - 1];
        for b in bindings {
            if b.kind != "LetBinding" {
                return Err(RewriteError {
                    message: format!(
                        "expected LetBinding (from AssignStmt) before result expression, found '{}'",
                        b.kind
                    ),
                    node_kind: b.kind.clone(),
                    span: b.span.clone(),
                });
            }
        }

        if bindings.is_empty() {
            return Ok(result);
        }

        // Fold right-to-left: innermost body = result.
        let mut current = result;
        for binding in bindings.iter().rev() {
            let name_tok = match binding.fields.get("name") {
                Some(SchemaValue::Token(t)) => t.clone(),
                _ => {
                    return Err(RewriteError {
                        message: "LetBinding missing 'name' token".to_string(),
                        node_kind: binding.kind.clone(),
                        span: binding.span.clone(),
                    })
                }
            };
            let value_node = match binding.fields.get("value") {
                Some(SchemaValue::Node(n)) => n.clone(),
                _ => {
                    return Err(RewriteError {
                        message: "LetBinding missing 'value' node".to_string(),
                        node_kind: binding.kind.clone(),
                        span: binding.span.clone(),
                    })
                }
            };
            let mut let_fields = std::collections::HashMap::new();
            let_fields.insert("name".to_string(), SchemaValue::Token(name_tok));
            let_fields.insert("value".to_string(), SchemaValue::Node(value_node));
            let_fields.insert("body".to_string(), SchemaValue::Node(Box::new(current)));
            current = SchemaAstNode {
                kind: "LetExpr".to_string(),
                fields: let_fields,
                annotations: binding.annotations.clone(),
                span: binding.span.clone(),
            };
        }

        Ok(current)
    }

    fn extract_first_element(&self, node: &SchemaAstNode) -> Result<SchemaAstNode, RewriteError> {
        // Extract first element from a list field (e.g., AppExpr with single element)
        // This handles cases like: AppExpr { parts: [expr] } → expr

        // Try to find a nodes field (most common: "parts", "items", "elements")
        let nodes_field = node
            .fields
            .get("parts")
            .or_else(|| node.fields.get("items"))
            .or_else(|| node.fields.get("elements"))
            .or_else(|| node.fields.get("children"));

        let nodes = match nodes_field {
            Some(SchemaValue::Nodes(nodes)) => nodes,
            _ => {
                return Err(RewriteError {
                    message: format!(
                        "Cannot extract_first_element from '{}': no nodes field found (tried: parts, items, elements, children)",
                        node.kind
                    ),
                    node_kind: node.kind.clone(),
                    span: node.span.clone(),
                })
            }
        };

        if nodes.is_empty() {
            return Err(RewriteError {
                message: format!(
                    "Cannot extract_first_element from empty list in '{}'",
                    node.kind
                ),
                node_kind: node.kind.clone(),
                span: node.span.clone(),
            });
        }

        if nodes.len() != 1 {
            return Err(RewriteError {
                message: format!(
                    "extract_first_element requires exactly 1 element, found {} in '{}'",
                    nodes.len(),
                    node.kind
                ),
                node_kind: node.kind.clone(),
                span: node.span.clone(),
            });
        }

        // Extract and return the single element (already normalized by bottom-up traversal)
        Ok(nodes[0].clone())
    }

    fn build_nf_fields(
        &self,
        source_node: &SchemaAstNode,
        replacement_fields: &ReplacementFields,
    ) -> Result<HashMap<String, SchemaValue>, RewriteError> {
        match replacement_fields {
            ReplacementFields::Literal(strategy) => match strategy.as_str() {
                "preserve_all" => Ok(source_node.fields.clone()),
                "nest_statements_as_let" => {
                    // H1 BlockContent: { s1; s2; expr } → nested LetExpr
                    self.nest_statements_as_let(source_node)
                }
                "extract_from_block_context"
                | "synthesize_for_each_call"
                | "curry_lambda_from_params"
                | "build_curried_lambda_chain"
                | "right_to_left_nesting"
                | "nest_if_chain"
                | "use_registry_for_each" => {
                    // Complex synthesis not yet implemented
                    Ok(source_node.fields.clone())
                }
                _ => Err(RewriteError {
                    message: format!("Unknown field replacement strategy: {}", strategy),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
            },
            ReplacementFields::Fields(field_mappings) => {
                let mut result = HashMap::new();

                for (target_field, mapping) in field_mappings {
                    let value = self.extract_field_value(source_node, mapping)?;
                    result.insert(target_field.clone(), value);
                }

                Ok(result)
            }
        }
    }

    fn nest_statements_as_let(
        &self,
        block_content: &SchemaAstNode,
    ) -> Result<HashMap<String, SchemaValue>, RewriteError> {
        // Extract stmts array
        let stmts = match block_content.fields.get("stmts") {
            Some(SchemaValue::Nodes(nodes)) => nodes,
            _ => {
                return Err(RewriteError {
                    message: "BlockContent missing 'stmts' nodes field".to_string(),
                    node_kind: block_content.kind.clone(),
                    span: block_content.span.clone(),
                })
            }
        };

        if stmts.is_empty() {
            return Err(RewriteError {
                message: "BlockContent must have at least one statement".to_string(),
                node_kind: block_content.kind.clone(),
                span: block_content.span.clone(),
            });
        }

        // Last statement becomes the body
        // TODO: Implement proper BlockContent nesting strategy
        // Previous statements should become nested let bindings (if they are LetStmt)
        // Right-to-left nesting: last let is innermost
        // For now, return error indicating this needs implementation

        Err(RewriteError {
            message: format!(
                "BlockContent nesting not yet implemented (has {} statements)",
                stmts.len()
            ),
            node_kind: block_content.kind.clone(),
            span: block_content.span.clone(),
        })
    }

    fn extract_field_value(
        &self,
        source_node: &SchemaAstNode,
        mapping: &FieldMapping,
    ) -> Result<SchemaValue, RewriteError> {
        let from_spec = &mapping.from;

        // Parse "from" specification
        if let Some(rest) = from_spec.strip_prefix("token:") {
            // Extract token field
            let field_name = rest;
            match source_node.fields.get(field_name) {
                Some(SchemaValue::Token(t)) => Ok(SchemaValue::Token(t.clone())),
                Some(_) => Err(RewriteError {
                    message: format!("Field '{}' is not a token", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
                None => Err(RewriteError {
                    message: format!("Token field '{}' not found", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
            }
        } else if let Some(rest) = from_spec.strip_prefix("node:") {
            // Extract node field (already normalized if recurse was set)
            let field_name = rest;
            match source_node.fields.get(field_name) {
                Some(SchemaValue::Node(n)) => Ok(SchemaValue::Node(n.clone())),
                Some(_) => Err(RewriteError {
                    message: format!("Field '{}' is not a node", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
                None => Err(RewriteError {
                    message: format!("Node field '{}' not found", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
            }
        } else if let Some(rest) = from_spec.strip_prefix("nodes:") {
            // Extract nodes array field
            let field_name = rest;
            match source_node.fields.get(field_name) {
                Some(v @ SchemaValue::Nodes(_)) => Ok(v.clone()),
                Some(_) => Err(RewriteError {
                    message: format!("Field '{}' is not a nodes array", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
                None => Err(RewriteError {
                    message: format!("Nodes field '{}' not found", field_name),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                }),
            }
        } else if let Some(rest) = from_spec.strip_prefix("children:") {
            // Extract from children array
            // Format: "children:parts[0]" or "children:parts"
            if let Some((field_name, index_part)) = rest.split_once('[') {
                // Extract specific index
                let index_str = index_part.trim_end_matches(']');
                let index: usize = index_str.parse().map_err(|_| RewriteError {
                    message: format!("Invalid index in children extraction: {}", index_str),
                    node_kind: source_node.kind.clone(),
                    span: source_node.span.clone(),
                })?;

                match source_node.fields.get(field_name) {
                    Some(SchemaValue::Nodes(nodes)) => {
                        if index < nodes.len() {
                            Ok(SchemaValue::Node(Box::new(nodes[index].clone())))
                        } else {
                            Err(RewriteError {
                                message: format!(
                                    "Children index {} out of bounds (len={})",
                                    index,
                                    nodes.len()
                                ),
                                node_kind: source_node.kind.clone(),
                                span: source_node.span.clone(),
                            })
                        }
                    }
                    Some(_) => Err(RewriteError {
                        message: format!("Field '{}' is not a node array", field_name),
                        node_kind: source_node.kind.clone(),
                        span: source_node.span.clone(),
                    }),
                    None => Err(RewriteError {
                        message: format!("Children field '{}' not found", field_name),
                        node_kind: source_node.kind.clone(),
                        span: source_node.span.clone(),
                    }),
                }
            } else {
                // Extract entire children array
                match source_node.fields.get(rest) {
                    Some(v @ SchemaValue::Nodes(_)) => Ok(v.clone()),
                    Some(_) => Err(RewriteError {
                        message: format!("Field '{}' is not a node array", rest),
                        node_kind: source_node.kind.clone(),
                        span: source_node.span.clone(),
                    }),
                    None => Err(RewriteError {
                        message: format!("Children field '{}' not found", rest),
                        node_kind: source_node.kind.clone(),
                        span: source_node.span.clone(),
                    }),
                }
            }
        } else {
            Err(RewriteError {
                message: format!(
                    "Invalid field mapping format: '{}'. Expected 'token:', 'node:', 'nodes:', or 'children:'",
                    from_spec
                ),
                node_kind: source_node.kind.clone(),
                span: source_node.span.clone(),
            })
        }
    }
}

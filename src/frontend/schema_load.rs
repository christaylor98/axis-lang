// Wave 4: AST Schema Loading and Validation
//
// This module loads and validates ast_schema.yaml files.
//
// The schema is the ONLY authority for AST projection.
// No defaults, no inference, no implicit behavior.

use crate::frontend::schema_ast::{
    AnnotationExtraction, AstSchema, FieldExtraction, SchemaNodeDef, SpanRule,
    UnhandledNodeBehavior,
};
use crate::frontend::token::TokenKind;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Schema load error
#[derive(Debug)]
pub struct SchemaLoadError {
    pub message: String,
}

impl std::fmt::Display for SchemaLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Schema load error: {}", self.message)
    }
}

impl std::error::Error for SchemaLoadError {}

/// Load AST schema from YAML file
pub fn load_schema_from_file<P: AsRef<Path>>(path: P) -> Result<AstSchema, SchemaLoadError> {
    let content = fs::read_to_string(path.as_ref()).map_err(|e| SchemaLoadError {
        message: format!("failed to read schema file: {}", e),
    })?;

    load_schema_from_string(&content)
}

/// Load AST schema from YAML string
pub fn load_schema_from_string(yaml: &str) -> Result<AstSchema, SchemaLoadError> {
    let value: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| SchemaLoadError {
        message: format!("failed to parse YAML: {}", e),
    })?;

    parse_schema(&value)
}

/// Parse schema from YAML value
fn parse_schema(value: &serde_yaml::Value) -> Result<AstSchema, SchemaLoadError> {
    let root = value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: "schema root must be a mapping".to_string(),
    })?;

    // Extract nodes mapping
    let nodes_value = root
        .get(&serde_yaml::Value::String("nodes".to_string()))
        .ok_or_else(|| SchemaLoadError {
            message: "schema missing 'nodes' field".to_string(),
        })?;

    let nodes_map = nodes_value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: "'nodes' must be a mapping".to_string(),
    })?;

    let mut nodes = HashMap::new();
    for (key, node_def_value) in nodes_map {
        let kind = key.as_str().ok_or_else(|| SchemaLoadError {
            message: "node kind must be a string".to_string(),
        })?;

        let node_def = parse_node_def(kind, node_def_value)?;
        nodes.insert(kind.to_string(), node_def);
    }

    // Default behavior: reject unhandled nodes (strict)
    let unhandled_behavior = UnhandledNodeBehavior::Reject;

    Ok(AstSchema {
        nodes,
        unhandled_behavior,
    })
}

/// Parse a schema node definition
fn parse_node_def(kind: &str, value: &serde_yaml::Value) -> Result<SchemaNodeDef, SchemaLoadError> {
    let node_map = value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: format!("node '{}' definition must be a mapping", kind),
    })?;

    // Extract 'match' field (which Generic AST node kind to match)
    let match_kind = node_map
        .get(&serde_yaml::Value::String("match".to_string()))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| kind.to_string()); // Default: same as schema kind

    // Extract 'fields' mapping
    let fields_value = node_map
        .get(&serde_yaml::Value::String("fields".to_string()))
        .ok_or_else(|| SchemaLoadError {
            message: format!("node '{}' missing 'fields'", kind),
        })?;

    let fields_map = fields_value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: format!("node '{}' fields must be a mapping", kind),
    })?;

    let mut fields = HashMap::new();
    for (field_key, field_value) in fields_map {
        let field_name = field_key.as_str().ok_or_else(|| SchemaLoadError {
            message: format!("field name in node '{}' must be a string", kind),
        })?;

        let extraction = parse_field_extraction(kind, field_name, field_value)?;
        fields.insert(field_name.to_string(), extraction);
    }

    // Optional annotation extraction
    let annotations = node_map
        .get(&serde_yaml::Value::String("annotations".to_string()))
        .map(|ann_value| parse_annotation_extraction(kind, ann_value))
        .transpose()?;

    // Optional span rule (defaults to Derived)
    let span_rule = Some(SpanRule::Derived);

    Ok(SchemaNodeDef {
        match_kind,
        fields,
        annotations,
        span_rule,
    })
}

/// Parse a field extraction rule
fn parse_field_extraction(
    node_kind: &str,
    field_name: &str,
    value: &serde_yaml::Value,
) -> Result<FieldExtraction, SchemaLoadError> {
    let field_map = value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: format!(
            "field '{}' in node '{}' must be a mapping with 'from' key",
            field_name, node_kind
        ),
    })?;

    let from_value = field_map
        .get(&serde_yaml::Value::String("from".to_string()))
        .ok_or_else(|| SchemaLoadError {
            message: format!(
                "field '{}' in node '{}' missing 'from' specifier",
                field_name, node_kind
            ),
        })?;

    let from_str = from_value.as_str().ok_or_else(|| SchemaLoadError {
        message: format!(
            "field '{}' 'from' in node '{}' must be a string",
            field_name, node_kind
        ),
    })?;

    parse_from_specifier(node_kind, field_name, from_str)
}

/// Parse a 'from' specifier (e.g., "child(1)", "token(INT_LIT)", "child_token(0)")
fn parse_from_specifier(
    node_kind: &str,
    field_name: &str,
    spec: &str,
) -> Result<FieldExtraction, SchemaLoadError> {
    let spec = spec.trim();

    // child(n)
    if spec.starts_with("child(") && spec.ends_with(')') {
        let inner = &spec[6..spec.len() - 1];
        let index = inner.parse::<usize>().map_err(|_| SchemaLoadError {
            message: format!(
                "invalid child index in field '{}' of node '{}': '{}'",
                field_name, node_kind, inner
            ),
        })?;
        return Ok(FieldExtraction::Child(index));
    }

    // child_token(n) - extract token from nth child node
    if spec.starts_with("child_token(") && spec.ends_with(')') {
        let inner = &spec[12..spec.len() - 1];
        let index = inner.parse::<usize>().map_err(|_| SchemaLoadError {
            message: format!(
                "invalid child index in child_token for field '{}' of node '{}': '{}'",
                field_name, node_kind, inner
            ),
        })?;
        return Ok(FieldExtraction::ChildToken(index));
    }

    // children(start..end) or children(start..)
    if spec.starts_with("children(") && spec.ends_with(')') {
        let inner = &spec[9..spec.len() - 1];
        let parts: Vec<&str> = inner.split("..").collect();
        if parts.len() != 2 {
            return Err(SchemaLoadError {
                message: format!(
                    "invalid children range in field '{}' of node '{}': '{}'",
                    field_name, node_kind, inner
                ),
            });
        }

        let start = parts[0].parse::<usize>().map_err(|_| SchemaLoadError {
            message: format!("invalid start index in children range: '{}'", parts[0]),
        })?;

        // Handle open-ended range: children(N..)
        let end = if parts[1].is_empty() {
            None // Open-ended: extract to end of children
        } else {
            Some(parts[1].parse::<usize>().map_err(|_| SchemaLoadError {
                message: format!("invalid end index in children range: '{}'", parts[1]),
            })?)
        };

        return Ok(FieldExtraction::Children(start, end));
    }

    // children_excluding_last(N)
    if spec.starts_with("children_excluding_last(") && spec.ends_with(')') {
        let inner = &spec[24..spec.len() - 1];
        let start = inner.parse::<usize>().map_err(|_| SchemaLoadError {
            message: format!(
                "invalid start index in children_excluding_last: '{}'",
                inner
            ),
        })?;
        return Ok(FieldExtraction::ChildrenExcludingLast(start));
    }

    // last_child
    if spec == "last_child" {
        return Ok(FieldExtraction::LastChild);
    }

    // first_token — first terminal of any kind (kind varies by grammar alternative)
    if spec == "first_token" {
        return Ok(FieldExtraction::FirstToken);
    }

    // token(TOKEN_KIND)
    if spec.starts_with("token(") && spec.ends_with(')') {
        let inner = &spec[6..spec.len() - 1];
        let kind = parse_token_kind(inner)?;
        return Ok(FieldExtraction::Token(kind));
    }

    // tokens(TOKEN_KIND)
    if spec.starts_with("tokens(") && spec.ends_with(')') {
        let inner = &spec[7..spec.len() - 1];
        let kind = parse_token_kind(inner)?;
        return Ok(FieldExtraction::Tokens(kind));
    }

    Err(SchemaLoadError {
        message: format!(
            "unknown 'from' specifier in field '{}' of node '{}': '{}'",
            field_name, node_kind, spec
        ),
    })
}

/// Parse a token kind string
fn parse_token_kind(s: &str) -> Result<TokenKind, SchemaLoadError> {
    match s {
        "IDENT" => Ok(TokenKind::Ident),
        "INT_LIT" => Ok(TokenKind::IntLit),
        "FLOAT_LIT" => Ok(TokenKind::FloatLit),
        "BOOL_LIT" => Ok(TokenKind::BoolLit),
        "STRING_LIT" => Ok(TokenKind::StringLit),
        "UNIT_LIT" => Ok(TokenKind::UnitLit),
        "EOF" => Ok(TokenKind::Eof),
        _ => {
            // Check for keyword or punct
            if s.starts_with("KW_") {
                let kw = &s[3..];
                Ok(TokenKind::Keyword(kw.to_string()))
            } else if s.starts_with("PUNCT_") {
                let p = &s[6..];
                Ok(TokenKind::Punct(p.to_string()))
            } else {
                Err(SchemaLoadError {
                    message: format!("unknown token kind: '{}'", s),
                })
            }
        }
    }
}

/// Parse annotation extraction rule
fn parse_annotation_extraction(
    node_kind: &str,
    value: &serde_yaml::Value,
) -> Result<AnnotationExtraction, SchemaLoadError> {
    let ann_map = value.as_mapping().ok_or_else(|| SchemaLoadError {
        message: format!(
            "annotations in node '{}' must be a mapping with 'from' key",
            node_kind
        ),
    })?;

    let from_value = ann_map
        .get(&serde_yaml::Value::String("from".to_string()))
        .ok_or_else(|| SchemaLoadError {
            message: format!(
                "annotations in node '{}' missing 'from' specifier",
                node_kind
            ),
        })?;

    let from_str = from_value.as_str().ok_or_else(|| SchemaLoadError {
        message: format!(
            "annotations 'from' in node '{}' must be a string",
            node_kind
        ),
    })?;

    // Parse tokens(TOKEN_KIND) format
    let spec = from_str.trim();
    if spec.starts_with("tokens(") && spec.ends_with(')') {
        let inner = &spec[7..spec.len() - 1];
        let token_kind = parse_token_kind(inner)?;
        return Ok(AnnotationExtraction::FromTokens(token_kind));
    }

    Err(SchemaLoadError {
        message: format!(
            "unsupported annotation extraction format in node '{}': '{}' (expected 'tokens(TOKEN_KIND)')",
            node_kind, spec
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_simple_schema() {
        let yaml = r#"
nodes:
  IntLiteral:
    match: Atom
    fields:
      value:
        from: token(INT_LIT)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        assert_eq!(schema.nodes.len(), 1);
        assert!(schema.nodes.contains_key("IntLiteral"));

        let int_lit = &schema.nodes["IntLiteral"];
        assert_eq!(int_lit.match_kind, "Atom");
        assert_eq!(int_lit.fields.len(), 1);
        assert!(int_lit.fields.contains_key("value"));
    }

    #[test]
    fn test_load_if_expr_schema() {
        let yaml = r#"
nodes:
  IfExpr:
    match: If
    fields:
      condition:
        from: child(1)
      then_branch:
        from: child(2)
      else_branch:
        from: child(4)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        assert_eq!(schema.nodes.len(), 1);

        let if_expr = &schema.nodes["IfExpr"];
        assert_eq!(if_expr.match_kind, "If");
        assert_eq!(if_expr.fields.len(), 3);
        assert!(if_expr.fields.contains_key("condition"));
        assert!(if_expr.fields.contains_key("then_branch"));
        assert!(if_expr.fields.contains_key("else_branch"));
    }

    #[test]
    fn test_parse_child_extraction() {
        let yaml = r#"
nodes:
  Test:
    match: Test
    fields:
      child0:
        from: child(0)
      child5:
        from: child(5)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        let test_node = &schema.nodes["Test"];

        match &test_node.fields["child0"] {
            FieldExtraction::Child(0) => (),
            _ => panic!("expected Child(0)"),
        }

        match &test_node.fields["child5"] {
            FieldExtraction::Child(5) => (),
            _ => panic!("expected Child(5)"),
        }
    }

    #[test]
    fn test_parse_children_extraction() {
        let yaml = r#"
nodes:
  Test:
    match: Test
    fields:
      items:
        from: children(1..4)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        let test_node = &schema.nodes["Test"];

        match &test_node.fields["items"] {
            FieldExtraction::Children(1, Some(4)) => (),
            _ => panic!("expected Children(1, Some(4))"),
        }
    }

    #[test]
    fn test_parse_children_open_ended() {
        let yaml = r#"
nodes:
  Test:
    match: Test
    fields:
      tail:
        from: children(2..)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        let test_node = &schema.nodes["Test"];

        match &test_node.fields["tail"] {
            FieldExtraction::Children(2, None) => (),
            _ => panic!("expected Children(2, None)"),
        }
    }

    #[test]
    fn test_parse_token_extraction() {
        let yaml = r#"
nodes:
  Test:
    match: Test
    fields:
      name:
        from: token(IDENT)
      value:
        from: token(INT_LIT)
"#;

        let schema = load_schema_from_string(yaml).expect("failed to load schema");
        let test_node = &schema.nodes["Test"];

        match &test_node.fields["name"] {
            FieldExtraction::Token(TokenKind::Ident) => (),
            _ => panic!("expected Token(Ident)"),
        }

        match &test_node.fields["value"] {
            FieldExtraction::Token(TokenKind::IntLit) => (),
            _ => panic!("expected Token(IntLit)"),
        }
    }

    #[test]
    fn test_invalid_schema_missing_nodes() {
        let yaml = r#"
other_field: value
"#;

        let result = load_schema_from_string(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("missing 'nodes' field"));
    }

    #[test]
    fn test_invalid_schema_missing_fields() {
        let yaml = r#"
nodes:
  Test:
    match: Test
"#;

        let result = load_schema_from_string(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("missing 'fields'"));
    }

    #[test]
    fn test_invalid_from_specifier() {
        let yaml = r#"
nodes:
  Test:
    match: Test
    fields:
      bad:
        from: invalid_spec
"#;

        let result = load_schema_from_string(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("unknown 'from' specifier"));
    }
}

// Wave 3 Integration Tests: Generic AST Builder
//
// Tests verify that ParseTree → Generic AST projection is:
// 1. Structurally faithful (isomorphic)
// 2. Span-correct (derived properly from children)
// 3. Deterministic (same input → same output)
// 4. Happy-path projection works for various grammar patterns

use axis_lang::frontend::ast_builder::{build_generic_ast, ASTChild, ASTNode};
use axis_lang::frontend::parser_runtime::{ParseNode, ParseTree};
use axis_lang::frontend::token::{Span, Token, TokenKind};

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Create a test token
fn token(kind: TokenKind, lexeme: &str, start: usize, end: usize) -> Token {
    Token {
        kind,
        lexeme: lexeme.to_string(),
        span: Span::new(start, end),
    }
}

/// Helper: create a simple ParseTree for testing
fn parse_tree(rule: &str, children: Vec<ParseNode>, start: usize, end: usize) -> ParseTree {
    ParseTree {
        rule: rule.to_string(),
        children,
        span: Span::new(start, end),
    }
}

/// Count total nodes in an AST (for structural comparison)
fn count_ast_nodes(ast: &ASTNode) -> usize {
    let mut count = 1; // this node
    for child in &ast.children {
        if let ASTChild::Node(node) = child {
            count += count_ast_nodes(node);
        }
    }
    count
}

/// Count total nodes in a ParseTree (for structural comparison)
fn count_parse_nodes(tree: &ParseTree) -> usize {
    let mut count = 1; // this node
    for child in &tree.children {
        if let ParseNode::Rule(subtree) = child {
            count += count_parse_nodes(subtree);
        }
    }
    count
}

/// Count terminals in AST
fn count_ast_terminals(ast: &ASTNode) -> usize {
    let mut count = 0;
    for child in &ast.children {
        match child {
            ASTChild::Terminal(_) => count += 1,
            ASTChild::Node(node) => count += count_ast_terminals(node),
        }
    }
    count
}

/// Count terminals in ParseTree
fn count_parse_terminals(tree: &ParseTree) -> usize {
    let mut count = 0;
    for child in &tree.children {
        match child {
            ParseNode::Terminal(_) => count += 1,
            ParseNode::Rule(subtree) => count += count_parse_terminals(subtree),
        }
    }
    count
}

/// Verify span invariants for an AST node and all descendants
fn verify_span_invariants(ast: &ASTNode) -> Result<(), String> {
    // Rule: node span must cover all children
    for child in &ast.children {
        let child_span = match child {
            ASTChild::Node(node) => &node.span,
            ASTChild::Terminal(token) => &token.span,
        };

        if child_span.start < ast.span.start {
            return Err(format!(
                "child span starts before parent: child.start={} < parent.start={}",
                child_span.start, ast.span.start
            ));
        }

        if child_span.end > ast.span.end {
            return Err(format!(
                "child span ends after parent: child.end={} > parent.end={}",
                child_span.end, ast.span.end
            ));
        }
    }

    // Rule: sibling spans must be monotonic (non-decreasing)
    for i in 1..ast.children.len() {
        let prev_span = match &ast.children[i - 1] {
            ASTChild::Node(node) => &node.span,
            ASTChild::Terminal(token) => &token.span,
        };
        let curr_span = match &ast.children[i] {
            ASTChild::Node(node) => &node.span,
            ASTChild::Terminal(token) => &token.span,
        };

        if curr_span.start < prev_span.end {
            return Err(format!(
                "sibling spans overlap or out of order: prev.end={} > curr.start={}",
                prev_span.end, curr_span.start
            ));
        }
    }

    // Recursively verify all child nodes
    for child in &ast.children {
        if let ASTChild::Node(node) = child {
            verify_span_invariants(node)?;
        }
    }

    Ok(())
}

/// Serialize AST structure to string (for fidelity comparison)
fn serialize_ast_structure(ast: &ASTNode) -> String {
    let mut s = format!("({}:", ast.kind);
    for child in &ast.children {
        s.push(' ');
        match child {
            ASTChild::Node(node) => s.push_str(&serialize_ast_structure(node)),
            ASTChild::Terminal(tok) => s.push_str(&format!("'{}'", tok.lexeme)),
        }
    }
    s.push(')');
    s
}

/// Serialize ParseTree structure to string (for fidelity comparison)
fn serialize_parse_structure(tree: &ParseTree) -> String {
    let mut s = format!("({}:", tree.rule);
    for child in &tree.children {
        s.push(' ');
        match child {
            ParseNode::Rule(subtree) => s.push_str(&serialize_parse_structure(subtree)),
            ParseNode::Terminal(tok) => s.push_str(&format!("'{}'", tok.lexeme)),
        }
    }
    s.push(')');
    s
}

/// Check if AST contains any EOF terminal (should never happen)
fn ast_contains_eof_terminal(ast: &ASTNode) -> bool {
    for child in &ast.children {
        match child {
            ASTChild::Terminal(tok) => {
                if matches!(tok.kind, TokenKind::Eof) {
                    return true;
                }
            }
            ASTChild::Node(node) => {
                if ast_contains_eof_terminal(node) {
                    return true;
                }
            }
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════════
// INVARIANT VERIFICATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// INVARIANT 1: EOF Token Never Appears in AST
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn invariant_eof_never_in_ast() {
    // Verify EOF never leaks into AST terminals
    // This relies on Wave 2 contract: EOF is a sentinel, never a ParseNode::Terminal

    // Test with various parse tree structures
    let tok_x = token(TokenKind::Ident, "x", 0, 1);
    let tok_42 = token(TokenKind::IntLit, "42", 2, 4);
    let tok_plus = token(TokenKind::Punct("+".to_string()), "+", 5, 6);

    let term1 = parse_tree("term", vec![ParseNode::Terminal(tok_x)], 0, 1);
    let term2 = parse_tree("term", vec![ParseNode::Terminal(tok_42)], 2, 4);

    let expr = parse_tree(
        "expr",
        vec![
            ParseNode::Rule(term1),
            ParseNode::Terminal(tok_plus),
            ParseNode::Rule(term2),
        ],
        0,
        6,
    );

    let ast = build_generic_ast(&expr).expect("AST build failed");

    // CRITICAL INVARIANT: EOF must never appear in AST
    assert!(
        !ast_contains_eof_terminal(&ast),
        "EOF token leaked into AST — Wave 2 contract violation!"
    );
}

#[test]
fn invariant_eof_never_in_ast_nested() {
    // Verify EOF invariant holds for deeply nested structures
    let tok = token(TokenKind::Ident, "a", 0, 1);

    let level3 = parse_tree("level3", vec![ParseNode::Terminal(tok)], 0, 1);
    let level2 = parse_tree("level2", vec![ParseNode::Rule(level3)], 0, 1);
    let level1 = parse_tree("level1", vec![ParseNode::Rule(level2)], 0, 1);

    let ast = build_generic_ast(&level1).expect("AST build failed");

    assert!(
        !ast_contains_eof_terminal(&ast),
        "EOF token leaked into nested AST"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// INVARIANT 2: Zero-Child Nodes Copy Spans Exactly
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn invariant_zero_child_span_exact_copy() {
    // Verify that zero-child ParseTree nodes copy spans exactly (no fabrication)

    // Synthetic ParseTree with no children and a specific span
    let tree = parse_tree("empty_rule", vec![], 42, 42);

    let ast = build_generic_ast(&tree).expect("AST build failed");

    // CRITICAL INVARIANT: Span must be copied exactly, not fabricated
    assert_eq!(
        ast.span, tree.span,
        "Zero-child node span was not copied exactly from ParseTree"
    );
    assert_eq!(ast.span.start, 42, "Span start was modified");
    assert_eq!(ast.span.end, 42, "Span end was modified");
}

#[test]
fn invariant_zero_child_span_various_positions() {
    // Test zero-child span copying at various positions
    let test_cases = vec![
        (0, 0),
        (10, 10),
        (100, 100),
        (0, 5), // Non-zero length but still zero children (edge case)
    ];

    for (start, end) in test_cases {
        let tree = parse_tree("empty", vec![], start, end);
        let ast = build_generic_ast(&tree).expect("AST build failed");

        assert_eq!(
            ast.span,
            Span::new(start, end),
            "Zero-child span not copied exactly for ({}, {})",
            start,
            end
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: HAPPY-PATH PROJECTION (Single Terminal)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_single_terminal() {
    // ParseTree: Rule("expr", [Terminal("42")])
    let tok = token(TokenKind::IntLit, "42", 0, 2);
    let tree = parse_tree("expr", vec![ParseNode::Terminal(tok.clone())], 0, 2);

    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Verify structure
    assert_eq!(ast.kind, "expr");
    assert_eq!(ast.children.len(), 1);
    assert_eq!(ast.span, Span::new(0, 2));

    // Verify node count matches
    assert_eq!(count_ast_nodes(&ast), count_parse_nodes(&tree));
    assert_eq!(count_ast_terminals(&ast), count_parse_terminals(&tree));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: HAPPY-PATH PROJECTION (Nested Grammar)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_nested_grammar() {
    // ParseTree: Rule("outer", [Rule("inner", [Terminal("99")])])
    let tok = token(TokenKind::IntLit, "99", 10, 12);
    let inner = parse_tree("inner", vec![ParseNode::Terminal(tok)], 10, 12);
    let outer = parse_tree("outer", vec![ParseNode::Rule(inner)], 10, 12);

    let ast = build_generic_ast(&outer).expect("AST build failed");

    // Verify nesting
    assert_eq!(ast.kind, "outer");
    assert_eq!(ast.children.len(), 1);

    match &ast.children[0] {
        ASTChild::Node(inner_node) => {
            assert_eq!(inner_node.kind, "inner");
            assert_eq!(inner_node.children.len(), 1);
        }
        _ => panic!("expected inner node"),
    }

    // Verify counts
    assert_eq!(count_ast_nodes(&ast), 2); // outer + inner
    assert_eq!(count_ast_terminals(&ast), 1); // one int
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: HAPPY-PATH PROJECTION (Multiple Children)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_happy_path_multiple_children() {
    // ParseTree: Rule("seq", [item, item, item])
    let tok_a = token(TokenKind::Ident, "a", 0, 1);
    let tok_b = token(TokenKind::Ident, "b", 2, 3);
    let tok_c = token(TokenKind::Ident, "c", 4, 5);

    let item_a = parse_tree("item", vec![ParseNode::Terminal(tok_a)], 0, 1);
    let item_b = parse_tree("item", vec![ParseNode::Terminal(tok_b)], 2, 3);
    let item_c = parse_tree("item", vec![ParseNode::Terminal(tok_c)], 4, 5);

    let seq = parse_tree(
        "seq",
        vec![
            ParseNode::Rule(item_a),
            ParseNode::Rule(item_b),
            ParseNode::Rule(item_c),
        ],
        0,
        5,
    );

    let ast = build_generic_ast(&seq).expect("AST build failed");

    // Should preserve all repetitions - no flattening!
    assert_eq!(ast.kind, "seq");
    assert_eq!(ast.children.len(), 3); // three 'item' nodes

    // All children should be 'item' nodes
    for child in &ast.children {
        match child {
            ASTChild::Node(node) => assert_eq!(node.kind, "item"),
            _ => panic!("expected item node"),
        }
    }

    // Verify counts match ParseTree
    assert_eq!(count_ast_nodes(&ast), count_parse_nodes(&seq));
    assert_eq!(count_ast_terminals(&ast), count_parse_terminals(&seq));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: SPAN CORRECTNESS (Derived from Children)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_span_correctness() {
    // Build: seq: ident ident ident
    let tok1 = token(TokenKind::Ident, "first", 10, 15);
    let tok2 = token(TokenKind::Ident, "middle", 20, 26);
    let tok3 = token(TokenKind::Ident, "last", 30, 34);

    let tree = parse_tree(
        "seq",
        vec![
            ParseNode::Terminal(tok1),
            ParseNode::Terminal(tok2),
            ParseNode::Terminal(tok3),
        ],
        0,
        100, // ParseTree span is wider - should be ignored
    );

    let ast = build_generic_ast(&tree).expect("AST build failed");

    // Span should be from first token start (10) to last token end (34)
    assert_eq!(ast.span.start, 10);
    assert_eq!(ast.span.end, 34);

    // Verify all span invariants hold
    verify_span_invariants(&ast).expect("span invariants violated");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: SPAN CORRECTNESS (Nested Nodes)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_span_correctness_nested() {
    // outer: inner1 inner2
    // inner1: ident
    // inner2: int
    let tok1 = token(TokenKind::Ident, "x", 5, 6);
    let tok2 = token(TokenKind::IntLit, "42", 10, 12);

    let inner1 = parse_tree("inner1", vec![ParseNode::Terminal(tok1)], 5, 6);
    let inner2 = parse_tree("inner2", vec![ParseNode::Terminal(tok2)], 10, 12);

    let outer = parse_tree(
        "outer",
        vec![ParseNode::Rule(inner1), ParseNode::Rule(inner2)],
        5,
        12,
    );

    let ast = build_generic_ast(&outer).expect("AST build failed");

    // Outer span should cover both children
    assert_eq!(ast.span.start, 5);
    assert_eq!(ast.span.end, 12);

    // Each inner node should have correct span
    match &ast.children[0] {
        ASTChild::Node(inner_node) => {
            assert_eq!(inner_node.span.start, 5);
            assert_eq!(inner_node.span.end, 6);
        }
        _ => panic!("expected inner1 node"),
    }

    match &ast.children[1] {
        ASTChild::Node(inner_node) => {
            assert_eq!(inner_node.span.start, 10);
            assert_eq!(inner_node.span.end, 12);
        }
        _ => panic!("expected inner2 node"),
    }

    // Verify all span invariants
    verify_span_invariants(&ast).expect("span invariants violated");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 6: STRUCTURAL FIDELITY (Isomorphism Check)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_structural_fidelity() {
    // program: stmt
    // stmt: ident = expr ;
    // expr: term + term
    // term: int | ident

    let tok_x = token(TokenKind::Ident, "x", 0, 1);
    let tok_eq = token(TokenKind::Punct("=".to_string()), "=", 2, 3);
    let tok_1 = token(TokenKind::IntLit, "1", 4, 5);
    let tok_plus = token(TokenKind::Punct("+".to_string()), "+", 6, 7);
    let tok_2 = token(TokenKind::IntLit, "2", 8, 9);
    let tok_semi = token(TokenKind::Punct(";".to_string()), ";", 10, 11);

    let term1 = parse_tree("term", vec![ParseNode::Terminal(tok_1)], 4, 5);
    let term2 = parse_tree("term", vec![ParseNode::Terminal(tok_2)], 8, 9);

    let expr = parse_tree(
        "expr",
        vec![
            ParseNode::Rule(term1),
            ParseNode::Terminal(tok_plus),
            ParseNode::Rule(term2),
        ],
        4,
        9,
    );

    let stmt = parse_tree(
        "stmt",
        vec![
            ParseNode::Terminal(tok_x),
            ParseNode::Terminal(tok_eq),
            ParseNode::Rule(expr),
            ParseNode::Terminal(tok_semi),
        ],
        0,
        11,
    );

    let program = parse_tree("program", vec![ParseNode::Rule(stmt)], 0, 11);

    let ast = build_generic_ast(&program).expect("AST build failed");

    // CRITICAL: AST shape must be isomorphic to ParseTree shape
    assert_eq!(count_ast_nodes(&ast), count_parse_nodes(&program));
    assert_eq!(count_ast_terminals(&ast), count_parse_terminals(&program));

    // Structure serialization should match
    let parse_structure = serialize_parse_structure(&program);
    let ast_structure = serialize_ast_structure(&ast);

    assert_eq!(
        parse_structure, ast_structure,
        "AST structure does not match ParseTree structure"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 7: DETERMINISM (Same Input → Same Output)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_determinism() {
    // expr: term + ident + int
    let tok_1 = token(TokenKind::IntLit, "1", 0, 1);
    let tok_plus1 = token(TokenKind::Punct("+".to_string()), "+", 2, 3);
    let tok_x = token(TokenKind::Ident, "x", 4, 5);
    let tok_plus2 = token(TokenKind::Punct("+".to_string()), "+", 6, 7);
    let tok_42 = token(TokenKind::IntLit, "42", 8, 10);

    let term1 = parse_tree("term", vec![ParseNode::Terminal(tok_1)], 0, 1);
    let term2 = parse_tree("term", vec![ParseNode::Terminal(tok_x)], 4, 5);
    let term3 = parse_tree("term", vec![ParseNode::Terminal(tok_42)], 8, 10);

    let expr = parse_tree(
        "expr",
        vec![
            ParseNode::Rule(term1),
            ParseNode::Terminal(tok_plus1),
            ParseNode::Rule(term2),
            ParseNode::Terminal(tok_plus2),
            ParseNode::Rule(term3),
        ],
        0,
        10,
    );

    // Build AST multiple times
    let ast1 = build_generic_ast(&expr).expect("AST build 1 failed");
    let ast2 = build_generic_ast(&expr).expect("AST build 2 failed");
    let ast3 = build_generic_ast(&expr).expect("AST build 3 failed");

    // Should be bitwise identical
    assert_eq!(ast1, ast2);
    assert_eq!(ast2, ast3);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 8: NO DROPPING (Keywords Preserved)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_no_dropping_keywords() {
    // stmt: if expr then expr
    let tok_if = token(TokenKind::Keyword("if".to_string()), "if", 0, 2);
    let tok_x = token(TokenKind::Ident, "x", 3, 4);
    let tok_then = token(TokenKind::Keyword("then".to_string()), "then", 5, 9);
    let tok_y = token(TokenKind::Ident, "y", 10, 11);

    let expr1 = parse_tree("expr", vec![ParseNode::Terminal(tok_x)], 3, 4);
    let expr2 = parse_tree("expr", vec![ParseNode::Terminal(tok_y)], 10, 11);

    let stmt = parse_tree(
        "stmt",
        vec![
            ParseNode::Terminal(tok_if),
            ParseNode::Rule(expr1),
            ParseNode::Terminal(tok_then),
            ParseNode::Rule(expr2),
        ],
        0,
        11,
    );

    let ast = build_generic_ast(&stmt).expect("AST build failed");

    // MUST preserve all 4 tokens (2 keywords + 2 idents)
    assert_eq!(count_ast_terminals(&ast), 4);

    // Verify keywords are present in AST
    let mut found_if = false;
    let mut found_then = false;

    fn check_keywords(node: &ASTNode, found_if: &mut bool, found_then: &mut bool) {
        for child in &node.children {
            match child {
                ASTChild::Terminal(tok) => {
                    if tok.lexeme == "if" {
                        *found_if = true;
                    }
                    if tok.lexeme == "then" {
                        *found_then = true;
                    }
                }
                ASTChild::Node(n) => check_keywords(n, found_if, found_then),
            }
        }
    }

    check_keywords(&ast, &mut found_if, &mut found_then);

    assert!(found_if, "keyword 'if' was dropped");
    assert!(found_then, "keyword 'then' was dropped");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 9: NO DROPPING (Punctuation Preserved)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_no_dropping_punctuation() {
    // list: item , item , item
    let tok_a = token(TokenKind::Ident, "a", 0, 1);
    let tok_comma1 = token(TokenKind::Punct(",".to_string()), ",", 1, 2);
    let tok_b = token(TokenKind::Ident, "b", 2, 3);
    let tok_comma2 = token(TokenKind::Punct(",".to_string()), ",", 3, 4);
    let tok_c = token(TokenKind::Ident, "c", 4, 5);

    let item_a = parse_tree("item", vec![ParseNode::Terminal(tok_a)], 0, 1);
    let item_b = parse_tree("item", vec![ParseNode::Terminal(tok_b)], 2, 3);
    let item_c = parse_tree("item", vec![ParseNode::Terminal(tok_c)], 4, 5);

    let list = parse_tree(
        "list",
        vec![
            ParseNode::Rule(item_a),
            ParseNode::Terminal(tok_comma1),
            ParseNode::Rule(item_b),
            ParseNode::Terminal(tok_comma2),
            ParseNode::Rule(item_c),
        ],
        0,
        5,
    );

    let ast = build_generic_ast(&list).expect("AST build failed");

    // MUST preserve all commas
    let mut comma_count = 0;

    fn count_commas(node: &ASTNode, count: &mut usize) {
        for child in &node.children {
            match child {
                ASTChild::Terminal(tok) => {
                    if tok.lexeme == "," {
                        *count += 1;
                    }
                }
                ASTChild::Node(n) => count_commas(n, count),
            }
        }
    }

    count_commas(&ast, &mut comma_count);

    assert_eq!(comma_count, 2, "commas were dropped");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 10: EMPTY CHILDREN (Edge Case)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_empty_children_edge_case() {
    // Edge case: ParseTree with no children
    let tree = parse_tree("empty", vec![], 10, 10);

    let ast = build_generic_ast(&tree).expect("AST build failed");

    assert_eq!(ast.kind, "empty");
    assert_eq!(ast.children.len(), 0);
    assert_eq!(ast.span, Span::new(10, 10)); // fallback to ParseTree span
}

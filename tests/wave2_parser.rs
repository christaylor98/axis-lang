use axis_lang::frontend::parser::parse;
use axis_lang::frontend::parser_lex::lex;

#[test]
fn parse_simple_if() {
    let src = "fn test() { if {} { {} } else { {} } }";
    let tokens = lex(src).expect("lex failed");
    let ast = parse(&tokens).expect("parse failed");

    match &ast.body {
        axis_lang::frontend::ast::FunctionBody::Block(block) => {
            assert_eq!(block.exprs.len(), 1);
            match &block.exprs[0] {
                axis_lang::frontend::ast::Expr::If { .. } => {}
                _ => panic!("expected If expression"),
            }
        }
        _ => panic!("expected block body"),
    }
}

#[test]
fn parse_nested_if() {
    let src = "fn test() { if {} { if {} { {} } else { {} } } else { {} } }";
    let tokens = lex(src).expect("lex failed");
    let ast = parse(&tokens).expect("parse failed");

    match &ast.body {
        axis_lang::frontend::ast::FunctionBody::Block(block) => {
            assert_eq!(block.exprs.len(), 1);
            match &block.exprs[0] {
                axis_lang::frontend::ast::Expr::If { then_block, .. } => {
                    assert_eq!(then_block.exprs.len(), 1);
                    match &then_block.exprs[0] {
                        axis_lang::frontend::ast::Expr::If { .. } => {}
                        _ => panic!("expected nested If expression"),
                    }
                }
                _ => panic!("expected If expression"),
            }
        }
        _ => panic!("expected block body"),
    }
}

#[test]
fn parse_if_in_block() {
    let src = "fn test() { {}; if {} { {} } else { {} }; {} }";
    let tokens = lex(src).expect("lex failed");
    let ast = parse(&tokens).expect("parse failed");

    match &ast.body {
        axis_lang::frontend::ast::FunctionBody::Block(block) => {
            assert_eq!(block.exprs.len(), 3);
            match &block.exprs[1] {
                axis_lang::frontend::ast::Expr::If { .. } => {}
                _ => panic!("expected If expression at position 1"),
            }
        }
        _ => panic!("expected block body"),
    }
}

#[test]
fn parse_rejects_missing_else() {
    let src = "fn test() { if {} { {} } }";
    let tokens = lex(src).expect("lex failed");
    let result = parse(&tokens);
    assert!(result.is_err(), "should reject if without else");
}

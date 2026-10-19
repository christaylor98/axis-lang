// Production parser tests - token reference syntax

use axis_lang::frontend::production_parser::parse_production_string;

#[test]
fn test_quoted_token_reference_arrow() {
    // Test quoted syntax for => token
    let prod_str = r#"<KW_FN> <IDENT> <"PUNCT_=>"> Expr"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse quoted PUNCT_=> token: {:?}",
        result.err()
    );
}

#[test]
fn test_quoted_token_reference_less_than() {
    // Test quoted syntax for < token
    let prod_str = r#"<"PUNCT_<"> TypeName"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse quoted PUNCT_< token: {:?}",
        result.err()
    );
}

#[test]
fn test_quoted_token_reference_greater_than() {
    // Test quoted syntax for > token
    let prod_str = r#"TypeName <"PUNCT_>">"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse quoted PUNCT_> token: {:?}",
        result.err()
    );
}

#[test]
fn test_quoted_token_reference_gte() {
    // Test quoted syntax for >= token
    let prod_str = r#"<"PUNCT_>="> AddExpr"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse quoted PUNCT_>= token: {:?}",
        result.err()
    );
}

#[test]
fn test_unquoted_token_reference_still_works() {
    // Test existing unquoted syntax still works
    let prod_str = r#"<IDENT> <INT_LIT> <PUNCT_==> <KW_FN>"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse unquoted tokens: {:?}",
        result.err()
    );
}

#[test]
fn test_quoted_token_unclosed_quote() {
    // Test error handling for unclosed quote
    let prod_str = r#"<"PUNCT_=>"#;
    let result = parse_production_string(prod_str);
    assert!(result.is_err(), "should fail on unclosed quote");
    let err = result.unwrap_err();
    assert!(
        err.contains("unclosed") || err.contains("quote"),
        "error should mention unclosed quote: {}",
        err
    );
}

#[test]
fn test_quoted_token_missing_closing_bracket() {
    // Test error handling for missing > after quote
    let prod_str = r#"<"PUNCT_=>" Expr"#;
    let result = parse_production_string(prod_str);
    assert!(result.is_err(), "should fail on missing closing >");
    let err = result.unwrap_err();
    assert!(
        err.contains(">") || err.contains("closing"),
        "error should mention closing bracket: {}",
        err
    );
}

#[test]
fn test_mixed_quoted_and_unquoted() {
    // Test that quoted and unquoted can be mixed in same production
    let prod_str = r#"<KW_LET> <IDENT> <"PUNCT_=>"> Expr <KW_IN> Expr"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse mixed quoted/unquoted: {:?}",
        result.err()
    );
}

#[test]
fn test_complex_production_with_quoted_tokens() {
    // Test realistic production with multiple quoted tokens
    let prod_str = r#"<"PUNCT_<"> TypeName <"PUNCT_>"> <"PUNCT_=>"> Expr"#;
    let result = parse_production_string(prod_str);
    assert!(
        result.is_ok(),
        "should parse complex quoted production: {:?}",
        result.err()
    );
}

// Tests for hard rejection of legacy parser spec syntax

#[cfg(test)]
mod legacy_syntax_rejection_tests {
    use axis_lang::frontend::production_parser::parse_production_string;

    #[test]
    fn test_reject_quoted_keyword_string() {
        let result = parse_production_string(r#""fn" <IDENT>"#);
        assert!(result.is_err(), "Should reject quoted keyword string");

        let err = result.unwrap_err();
        assert!(
            err.contains("FORBIDDEN"),
            "Error should mention FORBIDDEN: {}",
            err
        );
        assert!(
            err.contains("Quoted string"),
            "Error should mention quoted string: {}",
            err
        );
    }

    #[test]
    fn test_reject_quoted_punctuation_string() {
        let result = parse_production_string(r#""(" Expr ")""#);
        assert!(result.is_err(), "Should reject quoted punctuation string");

        let err = result.unwrap_err();
        assert!(
            err.contains("FORBIDDEN"),
            "Error should mention FORBIDDEN: {}",
            err
        );
        assert!(
            err.contains("Quoted string"),
            "Error should mention quoted string: {}",
            err
        );
    }

    #[test]
    fn test_reject_all_caps_token() {
        let result = parse_production_string("IDENT INT");
        assert!(result.is_err(), "Should reject ALL_CAPS token");

        let err = result.unwrap_err();
        assert!(
            err.contains("FORBIDDEN"),
            "Error should mention FORBIDDEN: {}",
            err
        );
        assert!(
            err.contains("ALL_CAPS"),
            "Error should mention ALL_CAPS: {}",
            err
        );
    }

    #[test]
    fn test_accept_explicit_token_kinds() {
        let result = parse_production_string("<IDENT> <INT>");
        assert!(
            result.is_ok(),
            "Should accept explicit token kinds: {:?}",
            result
        );
    }

    #[test]
    fn test_accept_explicit_keyword_syntax() {
        let result = parse_production_string(r#"<KW_FN> <IDENT>"#);
        assert!(
            result.is_ok(),
            "Should accept <KW_XXX> syntax: {:?}",
            result
        );
    }

    #[test]
    fn test_accept_explicit_punct_syntax() {
        let result = parse_production_string(r#"<PUNCT_(> <IDENT> <PUNCT_)>"#);
        assert!(
            result.is_ok(),
            "Should accept <PUNCT_XXX> syntax: {:?}",
            result
        );
    }

    #[test]
    fn test_accept_nonterminals() {
        let result = parse_production_string("Expr Stmt");
        assert!(result.is_ok(), "Should accept non-terminals: {:?}", result);
    }

    #[test]
    fn test_reject_legacy_mixed_with_valid() {
        let result = parse_production_string(r#"<KW_FN> IDENT"#);
        assert!(
            result.is_err(),
            "Should reject ALL_CAPS even when mixed with valid"
        );

        let err = result.unwrap_err();
        assert!(
            err.contains("FORBIDDEN"),
            "Error should mention FORBIDDEN: {}",
            err
        );
    }

    #[test]
    fn test_error_message_suggests_fix_for_quoted() {
        let result = parse_production_string(r#""if""#);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            err.contains("<KW_XXX>") || err.contains("<PUNCT_XXX>"),
            "Error should suggest fix: {}",
            err
        );
    }

    #[test]
    fn test_error_message_suggests_fix_for_all_caps() {
        let result = parse_production_string("INT");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(
            err.contains("<INT>"),
            "Error should suggest angle bracket syntax: {}",
            err
        );
    }
}

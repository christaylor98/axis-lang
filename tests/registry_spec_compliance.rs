// Regression tests for registry DSL compliance with core_spec/axis-registry-0.1.md
//
// These tests verify:
// 1. Spec-compliant syntax (fn/profile/end) parses successfully
// 2. Legacy/incorrect syntax fails loudly
// 3. Non-empty registry producing 0 entries fails loudly

use axis_lang::registry::Registry;

#[test]
fn test_spec_compliant_registry_parses() {
    // SPEC-COMPLIANT: Uses fn, profile (singular), end
    let src = r#"registry test 0.1

fn add
  arity 2
  deterministic true
  profile axis
end

fn mul
  arity 2
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Spec-compliant registry should parse successfully");
    
    let registry = result.unwrap();
    assert_eq!(registry.entries.len(), 2, "Should parse 2 functions");
    
    let add = registry.find_by_name("add");
    assert!(add.is_some(), "Should find 'add' function");
    assert_eq!(add.unwrap().arity, 2);
    assert_eq!(add.unwrap().deterministic, true);
    assert_eq!(add.unwrap().profiles, vec!["axis"]);
    
    let mul = registry.find_by_name("mul");
    assert!(mul.is_some(), "Should find 'mul' function");
    assert_eq!(mul.unwrap().arity, 2);
}

#[test]
fn test_multiple_profiles_accepted() {
    // SPEC: Multiple 'profile' lines are permitted (§4.4)
    let src = r#"registry test 0.1

fn experimental_fn
  arity 1
  deterministic false
  profile axis
  profile experimental
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Multiple profile lines should be accepted");
    
    let registry = result.unwrap();
    let func = registry.find_by_name("experimental_fn").unwrap();
    assert_eq!(func.profiles.len(), 2);
    assert!(func.profiles.contains(&"axis".to_string()));
    assert!(func.profiles.contains(&"experimental".to_string()));
}

#[test]
fn test_comments_are_supported() {
    // SPEC: Line comments with // are supported (§3.2)
    let src = r#"registry test 0.1

// This is a comment
fn test_fn  // inline comment
  arity 0  // another comment
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Comments should be supported");
    
    let registry = result.unwrap();
    assert_eq!(registry.entries.len(), 1);
}

#[test]
fn test_legacy_function_keyword_fails() {
    // OLD SYNTAX (non-spec): 'function' instead of 'fn'
    let src = r#"registry test 0.1

function add
  arity 2
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Legacy 'function' keyword should be rejected");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("function add") || err_msg.contains("unexpected directive"),
            "Error should mention invalid syntax. Got: {}", err_msg);
}

#[test]
fn test_legacy_profiles_plural_fails() {
    // OLD SYNTAX (non-spec): 'profiles' (plural) instead of 'profile' (singular)
    let src = r#"registry test 0.1

fn add
  arity 2
  deterministic true
  profiles axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Legacy 'profiles' keyword should be rejected");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("profiles") || err_msg.contains("unknown"),
            "Error should mention unknown field. Got: {}", err_msg);
}

#[test]
fn test_missing_end_fails() {
    // SPEC VIOLATION: Missing 'end' terminator
    let src = r#"registry test 0.1

fn incomplete
  arity 1
  deterministic true
  profile axis
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Missing 'end' should cause parse failure");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("end") || err_msg.contains("incomplete"),
            "Error should mention missing 'end'. Got: {}", err_msg);
}

#[test]
fn test_fail_loud_non_empty_zero_entries() {
    // FAIL-LOUD ENFORCEMENT: Non-empty file with 0 parsed entries must fail
    let src = r#"registry test 0.1

// This file has content but uses wrong syntax
function wrongsyntax
  arity 2
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Non-empty registry with 0 entries should fail loudly");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("0 function entries") || err_msg.contains("authority") 
            || err_msg.contains("wrongsyntax"),
            "Error should mention 0 entries or authority violation. Got: {}", err_msg);
}

#[test]
fn test_fail_loud_shows_first_line() {
    // FAIL-LOUD: Error should show first non-comment line
    let src = r#"registry test 0.1

// Comment line
bogus directive here
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Invalid directive should fail");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("bogus") || err_msg.contains("directive"),
            "Error should mention the invalid line. Got: {}", err_msg);
}

#[test]
fn test_missing_required_field_fails() {
    // Missing 'arity' field
    let src = r#"registry test 0.1

fn incomplete
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Missing required field should fail");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("arity") && err_msg.contains("missing"),
            "Error should mention missing arity. Got: {}", err_msg);
}

#[test]
fn test_missing_profile_fails() {
    // Missing 'profile' field
    let src = r#"registry test 0.1

fn no_profile
  arity 0
  deterministic true
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_err(), "Missing profile should fail");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(err_msg.contains("profile") && err_msg.contains("missing"),
            "Error should mention missing profile. Got: {}", err_msg);
}

#[test]
fn test_empty_registry_allowed() {
    // An empty registry (only header + comments) is allowed
    let src = r#"registry test 0.1

// Empty registry
// No functions declared
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Empty registry with only comments should be allowed");
    
    let registry = result.unwrap();
    assert_eq!(registry.entries.len(), 0);
}

#[test]
fn test_zero_arity_function() {
    // Zero-arity functions are valid
    let src = r#"registry test 0.1

fn get_constant
  arity 0
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Zero-arity function should be valid");
    
    let registry = result.unwrap();
    let func = registry.find_by_name("get_constant").unwrap();
    assert_eq!(func.arity, 0);
}

#[test]
fn test_non_deterministic_function() {
    // Non-deterministic functions are valid
    let src = r#"registry test 0.1

fn random
  arity 0
  deterministic false
  profile axis
end
"#;

    let result = Registry::parse(src);
    assert!(result.is_ok(), "Non-deterministic function should be valid");
    
    let registry = result.unwrap();
    let func = registry.find_by_name("random").unwrap();
    assert_eq!(func.deterministic, false);
}

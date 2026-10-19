// Demonstration: Fail-loud enforcement in action
//
// This test shows concrete examples of the fail-loud behavior
// when registry authority is violated

use axis_lang::registry::Registry;

#[test]
fn demo_fail_loud_old_function_keyword() {
    // Using old 'function' keyword (non-spec) should fail loudly
    let old_syntax = r#"registry demo 0.1

function my_function
  arity 2
  deterministic true
  profile axis
end
"#;

    let result = Registry::parse(old_syntax);
    assert!(result.is_err());
    
    let error = format!("{:?}", result.unwrap_err());
    println!("\n=== FAIL-LOUD DEMO 1: Old 'function' keyword ===");
    println!("{}\n", error);
    
    // Verify error mentions the problem
    assert!(error.contains("function my_function") || error.contains("unexpected directive"));
    assert!(error.contains("axis-registry-0.1.md") || error.contains("expected 'fn"));
}

#[test]
fn demo_fail_loud_missing_end() {
    // Missing 'end' terminator should fail loudly
    let no_end = r#"registry demo 0.1

fn incomplete
  arity 1
  deterministic true
  profile axis
"#;

    let result = Registry::parse(no_end);
    assert!(result.is_err());
    
    let error = format!("{:?}", result.unwrap_err());
    println!("\n=== FAIL-LOUD DEMO 2: Missing 'end' terminator ===");
    println!("{}\n", error);
    
    assert!(error.contains("end") && error.contains("incomplete"));
}

#[test]
fn demo_fail_loud_content_but_zero_entries() {
    // Registry with content but 0 parsed entries should fail loudly
    let wrong_syntax = r#"registry demo 0.1

// This looks like it has content
define some_function
  arity 2
"#;

    let result = Registry::parse(wrong_syntax);
    assert!(result.is_err());
    
    let error = format!("{:?}", result.unwrap_err());
    println!("\n=== FAIL-LOUD DEMO 3: Content but 0 entries ===");
    println!("{}\n", error);
    
    assert!(error.contains("0 function entries") || error.contains("authority") 
            || error.contains("define some_function"));
}

#[test]
fn demo_success_spec_compliant() {
    // Spec-compliant syntax should succeed
    let correct = r#"registry demo 0.1

// Spec-compliant function declaration
fn correct_function
  arity 2
  deterministic true
  profile axis
end

fn another_function
  arity 0
  deterministic false
  profile experimental
end
"#;

    let result = Registry::parse(correct);
    assert!(result.is_ok());
    
    let registry = result.unwrap();
    println!("\n=== SUCCESS DEMO: Spec-compliant syntax ===");
    println!("Parsed {} functions successfully:", registry.entries.len());
    for entry in &registry.entries {
        println!("  - {} (arity: {}, deterministic: {}, profiles: {:?})",
                 entry.name, entry.arity, entry.deterministic, entry.profiles);
    }
    println!();
    
    assert_eq!(registry.entries.len(), 2);
}

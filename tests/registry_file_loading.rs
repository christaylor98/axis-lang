// Integration test: Verify fail-loud enforcement with actual file loading
//
// This test verifies that the load_from_paths method correctly enforces
// the fail-loud invariant when registry files have content but 0 entries

use axis_lang::registry::Registry;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_fail_loud_file_loading_invalid_syntax() {
    let temp_dir = TempDir::new().unwrap();
    let registry_path = temp_dir.path().join("invalid.axreg");
    
    // Write a registry with invalid syntax (old 'function' keyword)
    fs::write(
        &registry_path,
        r#"registry test 0.1

function wrong_keyword
  arity 2
"#,
    )
    .unwrap();
    
    let result = Registry::load_from_paths(&[registry_path.clone()]);
    assert!(result.is_err(), "Should fail when loading invalid registry");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains(&registry_path.to_string_lossy().to_string()) 
            || err_msg.contains("invalid.axreg"),
        "Error should mention the file path. Got: {}",
        err_msg
    );
}

#[test]
fn test_fail_loud_file_loading_multiple_registries() {
    let temp_dir = TempDir::new().unwrap();
    
    // Create a valid registry
    let valid_path = temp_dir.path().join("valid.axreg");
    fs::write(
        &valid_path,
        r#"registry valid 0.1

fn good_fn
  arity 1
  deterministic true
  profile axis
end
"#,
    )
    .unwrap();
    
    // Create an invalid registry
    let invalid_path = temp_dir.path().join("invalid.axreg");
    fs::write(
        &invalid_path,
        r#"registry invalid 0.1

function bad_syntax
  arity 1
"#,
    )
    .unwrap();
    
    let result = Registry::load_from_paths(&[valid_path, invalid_path.clone()]);
    assert!(result.is_err(), "Should fail when any registry is invalid");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("invalid.axreg") || err_msg.contains(&invalid_path.to_string_lossy().to_string()),
        "Error should mention the invalid file. Got: {}",
        err_msg
    );
}

#[test]
fn test_duplicate_function_across_files_fails() {
    let temp_dir = TempDir::new().unwrap();
    
    // First registry with 'test_fn'
    let reg1_path = temp_dir.path().join("reg1.axreg");
    fs::write(
        &reg1_path,
        r#"registry reg1 0.1

fn test_fn
  arity 1
  deterministic true
  profile axis
end
"#,
    )
    .unwrap();
    
    // Second registry also with 'test_fn' (duplicate)
    let reg2_path = temp_dir.path().join("reg2.axreg");
    fs::write(
        &reg2_path,
        r#"registry reg2 0.1

fn test_fn
  arity 2
  deterministic false
  profile axis
end
"#,
    )
    .unwrap();
    
    let result = Registry::load_from_paths(&[reg1_path, reg2_path]);
    assert!(result.is_err(), "Should fail when function names are duplicated");
    
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err);
    assert!(
        err_msg.contains("ambiguous") && err_msg.contains("test_fn"),
        "Error should mention duplicate function. Got: {}",
        err_msg
    );
}

#[test]
fn test_valid_multi_registry_loading() {
    let temp_dir = TempDir::new().unwrap();
    
    // First registry
    let reg1_path = temp_dir.path().join("core.axreg");
    fs::write(
        &reg1_path,
        r#"registry core 0.1

fn add
  arity 2
  deterministic true
  profile axis
end

fn sub
  arity 2
  deterministic true
  profile axis
end
"#,
    )
    .unwrap();
    
    // Second registry
    let reg2_path = temp_dir.path().join("experimental.axreg");
    fs::write(
        &reg2_path,
        r#"registry experimental 0.1

fn exp_feature
  arity 1
  deterministic false
  profile experimental
end
"#,
    )
    .unwrap();
    
    let result = Registry::load_from_paths(&[reg1_path, reg2_path]);
    assert!(result.is_ok(), "Should load multiple valid registries");
    
    let registry = result.unwrap();
    assert_eq!(registry.entries.len(), 3, "Should have 3 total functions");
    assert!(registry.find_by_name("add").is_some());
    assert!(registry.find_by_name("sub").is_some());
    assert!(registry.find_by_name("exp_feature").is_some());
}

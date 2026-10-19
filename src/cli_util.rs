/// CLI Input Normalization Utilities
///
/// Provides deterministic, set-based input handling for the Axis toolchain.
/// All path inputs are:
/// - Expanded (files and directories)
/// - Sorted deterministically
/// - Deduplicated
/// - Canonicalized
///
/// This module ensures that CLI semantics are idempotent and order-independent,
/// except for default name derivation which uses the FIRST resolved input.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::fabric::hash::Hash256;
use crate::registry::core05::fold::Scope;

/// Load and merge .axreg registry files into a single Core-05 Scope.
///
/// Type names are resolved as one namespace across every registry in
/// `paths` (a type declared in one file may be used by another without
/// redeclaring it — see [`text::lower_many`]), and any fn or type name
/// declared in more than one registry is a hard error naming both
/// declaration sites, even if the two declarations are byte-identical. No
/// registry is ranked, preferred, or chosen over another; the error simply
/// stops the merge. Returns Err on any unreadable, malformed, or
/// name-colliding file.
pub fn load_and_merge_registries(paths: &[PathBuf]) -> Result<Scope, String> {
    use crate::registry::core05::codec::type_identity;
    use crate::registry::core05::record::Record;
    use crate::registry::core05::text;

    let mut sources: Vec<(PathBuf, String)> = Vec::with_capacity(paths.len());
    for path in paths {
        let src = std::fs::read_to_string(path)
            .map_err(|e| format!("Cannot read registry {}: {}", path.display(), e))?;
        sources.push((path.clone(), src));
    }
    let lowered_files = text::lower_many(&sources)?;

    let mut merged = Scope::new();
    // Tracks the first file each identity/name was seen in, so a conflict
    // error can name both declaration sites (diagnostic only — never
    // consulted to arbitrate, never affects what gets merged).
    let mut type_sites: HashMap<Hash256, PathBuf> = HashMap::new();
    let mut fn_sites: HashMap<Hash256, PathBuf> = HashMap::new();
    let mut name_sites: HashMap<String, PathBuf> = HashMap::new();

    for ((path, _src), lowered) in sources.iter().zip(lowered_files) {
        let s = lowered.scope;

        for (_, t) in s.types {
            let id = type_identity(&t);
            merged
                .insert(Record::Type(t))
                .map_err(|e| conflict_msg(&e, &type_sites, &id, path))?;
            type_sites.entry(id).or_insert_with(|| path.clone());
        }
        for (_, f) in s.fns {
            let id = f.identity;
            merged
                .insert(Record::FnGenesis(f))
                .map_err(|e| conflict_msg(&e, &fn_sites, &id, path))?;
            fn_sites.entry(id).or_insert_with(|| path.clone());
        }
        for (_, l) in s.leaf_facts {
            let id = l.identity;
            merged
                .insert(Record::LeafFacts(l))
                .map_err(|e| conflict_msg(&e, &fn_sites, &id, path))?;
            fn_sites.entry(id).or_insert_with(|| path.clone());
        }
        for (_, c) in s.composite_bodies {
            let id = c.identity;
            merged
                .insert(Record::CompositeBody(c))
                .map_err(|e| conflict_msg(&e, &fn_sites, &id, path))?;
            fn_sites.entry(id).or_insert_with(|| path.clone());
        }
        for (k, v) in s.contract_appends {
            merged.contract_appends.entry(k).or_default().extend(v);
        }
        for (k, v) in s.implementations {
            merged.implementations.entry(k).or_default().extend(v);
        }
        for (name, id) in s.names {
            if let Some(first) = name_sites.get(&name) {
                return Err(format!(
                    "Registry conflict: duplicate name '{}' — declared in {} and {}",
                    name,
                    first.display(),
                    path.display()
                ));
            }
            merged.names.insert(name.clone(), id);
            name_sites.insert(name, path.clone());
        }
        merged.hof_callback_params.extend(s.hof_callback_params);
        // Keyed by fn identity, and a fn is declared in exactly one file of a
        // conflict-free set, so no two files can contribute contracts for the
        // same key here.
        merged
            .hof_callback_contracts
            .extend(s.hof_callback_contracts);
    }
    Ok(merged)
}

/// Render a `ResolveError` from a record-identity conflict, naming the first
/// site the identity was seen at (from `sites`) and the current file.
fn conflict_msg(
    e: &crate::registry::core05::fold::ResolveError,
    sites: &HashMap<Hash256, PathBuf>,
    id: &Hash256,
    current: &Path,
) -> String {
    let first = sites
        .get(id)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    format!(
        "Registry conflict: {} — declared in {} and {}",
        e,
        first,
        current.display()
    )
}

/// Normalize a list of path inputs into a deterministic, deduplicated set.
///
/// # Process:
/// 1. Expand each path (file → that file, directory → recursive walk)
/// 2. Sort all expanded paths
/// 3. Canonicalize or normalize paths
/// 4. Insert into HashSet for deduplication
///
/// # Returns:
/// A vector of unique paths in deterministic order.
///
/// # Note:
/// Repeated inputs are silently deduplicated with no warnings or diagnostics.
pub fn normalize_path_inputs(
    inputs: &[PathBuf],
    extension_filter: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let mut expanded = Vec::new();

    // Step 1: Expand all inputs
    for path in inputs {
        if !path.exists() {
            return Err(format!("Path does not exist: {}", path.display()));
        }

        if path.is_file() {
            expanded.push(path.clone());
        } else if path.is_dir() {
            // Recursive walk
            walk_dir_recursive(path, &mut expanded, extension_filter)?;
        }
    }

    // Step 2: Sort deterministically (lexicographic)
    expanded.sort();

    // Step 3: Canonicalize paths
    let mut canonicalized = Vec::new();
    for path in &expanded {
        match path.canonicalize() {
            Ok(canonical) => canonicalized.push(canonical),
            Err(_) => {
                // Fallback to normalized relative path if canonicalization fails
                canonicalized.push(normalize_path(path));
            }
        }
    }

    // Step 4: Deduplicate using HashSet
    let unique_set: HashSet<PathBuf> = canonicalized.into_iter().collect();

    // Step 5: Return as sorted vector for determinism
    let mut result: Vec<PathBuf> = unique_set.into_iter().collect();
    result.sort();

    Ok(result)
}

/// Recursively walk a directory and collect all files matching the optional extension filter.
fn walk_dir_recursive(
    dir: &Path,
    output: &mut Vec<PathBuf>,
    extension_filter: Option<&str>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        format!("Failed to read directory {}: {}", dir.display(), e)
    })?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            format!("Failed to read directory entry in {}: {}", dir.display(), e)
        })?;

        let path = entry.path();

        if path.is_file() {
            // Apply extension filter if specified
            if let Some(ext) = extension_filter {
                if path.extension().and_then(|s| s.to_str()) == Some(ext) {
                    output.push(path);
                }
            } else {
                output.push(path);
            }
        } else if path.is_dir() {
            walk_dir_recursive(&path, output, extension_filter)?;
        }
    }

    Ok(())
}

/// Normalize a path (make it clean and consistent).
fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            _ => components.push(component),
        }
    }
    components.iter().collect()
}

/// Derive a default name from the first resolved input path.
///
/// # Returns:
/// The basename (filename without extension) of the first path.
pub fn derive_name_from_first_input(inputs: &[PathBuf]) -> Option<String> {
    inputs.first().and_then(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|s| s.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_normalize_deduplicates() {
        let temp = TempDir::new().unwrap();
        let file1 = temp.path().join("test.ax");
        fs::write(&file1, "content").unwrap();

        // Same file specified twice
        let inputs = vec![file1.clone(), file1.clone()];
        let result = normalize_path_inputs(&inputs, None).unwrap();

        // Should deduplicate to one file
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_directory_expansion() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();
        
        fs::write(dir.join("a.ax"), "a").unwrap();
        fs::write(dir.join("b.ax"), "b").unwrap();
        
        let subdir = dir.join("sub");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("c.ax"), "c").unwrap();

        let inputs = vec![dir.to_path_buf()];
        let result = normalize_path_inputs(&inputs, Some("ax")).unwrap();

        // Should find all 3 .ax files
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_derive_name() {
        let path = PathBuf::from("path/to/myfile.ax");
        let name = derive_name_from_first_input(&[path]);
        assert_eq!(name, Some("myfile".to_string()));
    }

    fn fn_block(effect: &str) -> String {
        format!(
            "registry dup_test 0.5\n\n\
             type Unit = prim unit\n\n\
             fn my_fn\n\
             \x20 identity 0x1111111111111111111111111111111111111111111111111111111111111111\n\
             \x20 kind leaf\n\
             \x20 in (Unit)\n\
             \x20 out Unit\n\
             \x20 effect {effect}\n\
             \x20 deterministic true\n\
             \x20 idempotent true\n\
             end\n"
        )
    }

    #[test]
    fn test_merge_identical_duplicate_name_still_errors() {
        let temp = TempDir::new().unwrap();
        let a = temp.path().join("a.axreg");
        let b = temp.path().join("b.axreg");
        fs::write(&a, fn_block("pure")).unwrap();
        fs::write(&b, fn_block("pure")).unwrap();

        let err = load_and_merge_registries(&[a.clone(), b.clone()])
            .expect_err("any duplicate reuse of a name across registries is an error, even if identical");
        assert!(err.contains("duplicate"), "error must flag a duplicate: {err}");
        assert!(err.contains(&a.display().to_string()), "error must name the first site: {err}");
        assert!(err.contains(&b.display().to_string()), "error must name the second site: {err}");
    }

    #[test]
    fn test_merge_divergent_duplicate_errors_naming_both_sites() {
        let temp = TempDir::new().unwrap();
        let a = temp.path().join("a.axreg");
        let b = temp.path().join("b.axreg");
        fs::write(&a, fn_block("pure")).unwrap();
        fs::write(&b, fn_block("fullIo")).unwrap();

        let err = load_and_merge_registries(&[a.clone(), b.clone()])
            .expect_err("divergent duplicate declaration must be a hard error, not last-write-wins");
        assert!(err.contains(&a.display().to_string()), "error must name the first site: {err}");
        assert!(err.contains(&b.display().to_string()), "error must name the second site: {err}");
    }

    #[test]
    fn test_merge_resolves_type_declared_in_another_registry() {
        let temp = TempDir::new().unwrap();
        let types = temp.path().join("types.axreg");
        let fns = temp.path().join("fns.axreg");
        // `types.axreg` is the sole owner of `Unit`; `fns.axreg` uses it in a
        // fn signature without redeclaring it — this must resolve, not error,
        // now that type names are one namespace across the whole invocation.
        fs::write(&types, "registry types 0.5\n\ntype Unit = prim unit\n").unwrap();
        fs::write(
            &fns,
            "registry fns 0.5\n\n\
             fn my_fn\n\
             \x20 identity 0x3333333333333333333333333333333333333333333333333333333333333333\n\
             \x20 kind leaf\n\
             \x20 in (Unit)\n\
             \x20 out Unit\n\
             \x20 effect pure\n\
             \x20 deterministic true\n\
             \x20 idempotent true\n\
             end\n",
        )
        .unwrap();

        let scope = load_and_merge_registries(&[types, fns])
            .expect("a type declared in one registry must resolve when used from another");
        assert_eq!(scope.types.len(), 1);
        assert_eq!(scope.fns.len(), 1);
    }

    #[test]
    fn test_merge_duplicate_type_name_across_registries_errors() {
        let temp = TempDir::new().unwrap();
        let a = temp.path().join("a.axreg");
        let b = temp.path().join("b.axreg");
        fs::write(&a, "registry a 0.5\n\ntype Unit = prim unit\n").unwrap();
        fs::write(&b, "registry b 0.5\n\ntype Unit = prim unit\n").unwrap();

        let err = load_and_merge_registries(&[a.clone(), b.clone()])
            .expect_err("the same type name declared in two registries must be a hard error");
        assert!(err.contains("duplicate type name"), "error must flag a duplicate type name: {err}");
        assert!(err.contains(&a.display().to_string()), "error must name the first site: {err}");
        assert!(err.contains(&b.display().to_string()), "error must name the second site: {err}");
    }
}

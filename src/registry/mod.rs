// Core IR 0.5 registry (Phase 1 of the migration). The legacy 0.3 model below
// is unchanged; lowerings still target it and migrate onto `core05` in Phase 3.
pub mod core05;

use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct RegistryEntry {
    pub name: String,
    pub arity: usize,
    pub deterministic: bool,
    pub profiles: Vec<String>,
    pub id: u64,
}

#[derive(Debug, Clone)]
pub struct Registry {
    pub entries: Vec<RegistryEntry>,
}

#[derive(Debug)]
pub enum RegistryLoadError {
    Io(std::io::Error),
    Parse(String),
}

impl From<std::io::Error> for RegistryLoadError {
    fn from(e: std::io::Error) -> Self {
        RegistryLoadError::Io(e)
    }
}

impl Registry {
    pub fn load_active() -> Result<Self, RegistryLoadError> {
        // Load configuration (optional). If missing, preserve Wave 3 behavior.
        let cfg = match crate::config::language_lab::LanglabConfig::load() {
            Ok(c) => c,
            Err(_) => crate::config::language_lab::LanglabConfig::default(),
        };

        // If no registries specified, fall back to Wave 3 default
        if cfg.registry_paths.is_empty() {
            let path = Path::new("registries/wave3_test.axreg");
            let s = fs::read_to_string(path)?;
            return Self::parse(&s);
        }

        Self::load_from_paths(&cfg.registry_paths)
    }

    pub fn load_from_paths(paths: &[std::path::PathBuf]) -> Result<Self, RegistryLoadError> {
        let mut entries: Vec<RegistryEntry> = Vec::new();
        let mut id_counter: u64 = 0;

        for p in paths {
            let s = fs::read_to_string(p).map_err(|e| {
                RegistryLoadError::Parse(format!(
                    "failed to read registry file {}: {}",
                    p.display(),
                    e
                ))
            })?;
            
            // parse entries without relying on ids from the file
            let parsed = Self::parse_entries(&s).map_err(|e| {
                match e {
                    RegistryLoadError::Parse(msg) => {
                        RegistryLoadError::Parse(format!("in {}: {}", p.display(), msg))
                    }
                    other => other,
                }
            })?;
            
            // FAIL-LOUD ENFORCEMENT (SPEC §1):
            // If a registry file contains non-comment content but yields 0 entries,
            // compilation MUST fail.
            let has_content = s.lines().any(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("registry ")
            });
            
            if has_content && parsed.is_empty() {
                return Err(RegistryLoadError::Parse(format!(
                    "Registry file {} contains non-comment content but parsed 0 function entries.\n\
                     This violates registry authority (core_spec/axis-registry-0.1.md §1).\n\
                     If a registry file has content, it must declare at least one function using 'fn <name>' syntax.",
                    p.display()
                )));
            }
            
            // check for duplicate names across already-loaded entries
            for e in parsed {
                if entries.iter().any(|ex| ex.name == e.name) {
                    return Err(RegistryLoadError::Parse(format!(
                        "ambiguous registry definition for '{}' (duplicate function name across registry files)",
                        e.name
                    )));
                }
                let mut new_e = e.clone();
                new_e.id = id_counter;
                id_counter += 1;
                entries.push(new_e);
            }
        }

        Ok(Registry { entries })
    }

    // Helper: parse registry source into entries with id==0 (ids assigned by loader)
    // SPEC-COMPLIANT: core_spec/axis-registry-0.1.md §3
    fn parse_entries(src: &str) -> Result<Vec<RegistryEntry>, RegistryLoadError> {
        let mut entries: Vec<RegistryEntry> = Vec::new();
        let lines_iter = src.lines().enumerate();
        let mut first_non_comment_line: Option<(usize, String)> = None;

        // Skip comments and whitespace to find first meaningful line
        let mut lines: Vec<(usize, String)> = Vec::new();
        for (line_num, line) in lines_iter {
            let trimmed = line.trim();
            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }
            // Skip comment lines
            if trimmed.starts_with("//") {
                continue;
            }
            // Strip inline comments
            let clean = if let Some(pos) = trimmed.find("//") {
                trimmed[..pos].trim()
            } else {
                trimmed
            };
            if !clean.is_empty() {
                if first_non_comment_line.is_none() {
                    first_non_comment_line = Some((line_num + 1, clean.to_string()));
                }
                lines.push((line_num + 1, clean.to_string()));
            }
        }

        let mut idx = 0;
        
        // Expect leading registry header
        if idx < lines.len() {
            let (_, ref line) = lines[idx];
            if !line.starts_with("registry ") {
                return Err(RegistryLoadError::Parse(
                    "missing registry header (expected 'registry <name> <version>')".into()
                ));
            }
            idx += 1;
        }

        // Parse function blocks
        while idx < lines.len() {
            let (line_num, ref line) = lines[idx];
            
            // SPEC: fn <name>
            if line.starts_with("fn ") {
                let name = line[3..].trim().to_string();
                idx += 1;
                
                let mut arity: Option<usize> = None;
                let mut deterministic: Option<bool> = None;
                let mut profiles: Vec<String> = Vec::new();

                // Read fields until 'end'
                let mut found_end = false;
                while idx < lines.len() {
                    let (field_line_num, ref field) = lines[idx];
                    
                    if field == "end" {
                        found_end = true;
                        idx += 1;
                        break;
                    } else if field.starts_with("arity ") {
                        let rest = field[6..].trim();
                        match rest.parse::<usize>() {
                            Ok(n) => arity = Some(n),
                            Err(_) => {
                                return Err(RegistryLoadError::Parse(format!(
                                    "line {}: invalid arity: {}",
                                    field_line_num, rest
                                )))
                            }
                        }
                        idx += 1;
                    } else if field.starts_with("deterministic ") {
                        let rest = field[14..].trim();
                        match rest {
                            "true" => deterministic = Some(true),
                            "false" => deterministic = Some(false),
                            _ => {
                                return Err(RegistryLoadError::Parse(format!(
                                    "line {}: invalid deterministic value '{}' (expected 'true' or 'false')",
                                    field_line_num, rest
                                )))
                            }
                        }
                        idx += 1;
                    } else if field.starts_with("profile ") {
                        // SPEC: singular 'profile', multiple lines allowed
                        let rest = field[8..].trim();
                        profiles.push(rest.to_string());
                        idx += 1;
                    } else {
                        return Err(RegistryLoadError::Parse(format!(
                            "line {}: unknown registry field '{}' (expected: arity, deterministic, profile, or end)\nSee core_spec/axis-registry-0.1.md for valid syntax",
                            field_line_num, field
                        )));
                    }
                }

                if !found_end {
                    return Err(RegistryLoadError::Parse(format!(
                        "line {}: missing 'end' for function '{}'\nSee core_spec/axis-registry-0.1.md §3",
                        line_num, name
                    )));
                }

                let arity = arity.ok_or_else(|| {
                    RegistryLoadError::Parse(format!(
                        "line {}: missing 'arity' field for function '{}'",
                        line_num, name
                    ))
                })?;
                let deterministic = deterministic.ok_or_else(|| {
                    RegistryLoadError::Parse(format!(
                        "line {}: missing 'deterministic' field for function '{}'",
                        line_num, name
                    ))
                })?;
                if profiles.is_empty() {
                    return Err(RegistryLoadError::Parse(format!(
                        "line {}: missing 'profile' field for function '{}'",
                        line_num, name
                    )));
                }

                entries.push(RegistryEntry {
                    name,
                    arity,
                    deterministic,
                    profiles,
                    id: 0,
                });
            } else {
                // Unknown top-level directive
                return Err(RegistryLoadError::Parse(format!(
                    "line {}: unexpected directive '{}' (expected 'fn <name>')\nSee core_spec/axis-registry-0.1.md §3",
                    line_num, line
                )));
            }
        }

        Ok(entries)
    }

    pub fn parse(src: &str) -> Result<Self, RegistryLoadError> {
        let entries = Self::parse_entries(src)?;
        
        // FAIL-LOUD ENFORCEMENT (SPEC §1):
        // If a registry file contains non-comment content but yields 0 entries,
        // compilation MUST fail.
        let has_content = src.lines().any(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("registry ")
        });
        
        if has_content && entries.is_empty() {
            // Find first non-comment line for error reporting
            let first_line = src.lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with("registry "))
                .unwrap_or("<unknown>");
            
            return Err(RegistryLoadError::Parse(format!(
                "Registry contains non-comment content but parsed 0 function entries.\n\
                 First non-comment line: '{}'\n\
                 This violates registry authority (core_spec/axis-registry-0.1.md §1).\n\
                 Valid syntax: 'fn <name>' followed by fields and 'end'.",
                first_line
            )));
        }
        
        // Assign sequential IDs
        let entries_with_ids: Vec<RegistryEntry> = entries
            .into_iter()
            .enumerate()
            .map(|(idx, mut e)| {
                e.id = idx as u64;
                e
            })
            .collect();

        Ok(Registry { entries: entries_with_ids })
    }

    pub fn find_by_name(&self, name: &str) -> Option<&RegistryEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Create a new empty registry
    pub fn new() -> Self {
        Registry {
            entries: Vec::new(),
        }
    }

    /// Get all registry entries (for merging)
    pub fn entries(&self) -> &[RegistryEntry] {
        &self.entries
    }

    /// Register a new entry (for manual merging with last-wins policy)
    pub fn register(&mut self, name: String, entry: RegistryEntry) {
        // Remove any existing entry with the same name (last-wins)
        self.entries.retain(|e| e.name != name);
        self.entries.push(entry);
    }
}

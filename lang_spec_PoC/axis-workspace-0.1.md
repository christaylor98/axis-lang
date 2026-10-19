# Axis Workspace Specification

## Version 0.1 — Local Project Composition  
**Status: Normative**

---

## 1. Purpose

An Axis Workspace composes multiple local Axis projects into a single,
explicit registry set for compilation and execution.

Workspaces exist to:
- support multi-project development
- avoid ad-hoc path dependencies
- keep registry composition explicit and deterministic

---

## 2. Workspace Manifest (`Axis.toml`)

A workspace is defined by a single manifest file.

### 2.1 Structure

```toml
[workspace]
members = [
  "projects/core",
  "projects/math",
  "projects/app",
]

[profile]
name = "Surface_0_7_Strict"
````

---

## 3. Project Structure

Each workspace member is an Axis project.

Canonical layout:

```
project/
├── axis/
│   ├── *.ax
│   └── registry.axreg
└── Axis.toml
```

Projects:

* own their Axis code
* export registry fragments
* do not import other projects directly

---

## 4. Project Manifest

Example `Axis.toml` inside a project:

```toml
[project]
name = "local.math"

[exports]
registry = "axis/registry.axreg"
```

---

## 5. Workspace Registry Generation

The workspace tool MUST:

1. Read the workspace manifest
2. Discover all member projects
3. Collect all exported registry fragments
4. Validate global uniqueness of function names
5. Emit a single flat registry file

Canonical output:

```
target/axis/workspace.axreg
```

---

## 6. Name Conflicts (Normative)

If two workspace projects export the same function name:

* Workspace registry generation MUST fail
* No shadowing or precedence is permitted
* Conflicts must be resolved explicitly

---

## 7. Compiler Interaction

The compiler/runtime MUST be invoked with:

```text
--registry axis/registry/core.axreg
--registry target/axis/workspace.axreg
```

Axis code MUST NOT observe workspace boundaries.

---

## 8. Versioning Boundary (Normative)

Workspaces operate on unversioned local projects.

Versioning applies only to packaged artifacts.
Local projects MUST NOT participate in version resolution.

---

## 9. Generated Registry Immutability

Generated registry files:

* MUST NOT be edited by hand
* MUST be regenerated on change
* MUST be treated as build artifacts

---

## 10. Non-Goals

Workspaces do NOT:

* define language semantics
* permit implicit dependency resolution
* introduce registry precedence rules

---

## Guiding Principle

Projects export registries.
Workspaces compose registries.
The compiler sees one flat world.
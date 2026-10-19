# Axis Federation Specification

## Version 0.1 — Multi-Workspace Composition  
**Status: Normative**

---

## 1. Purpose

A Federation composes multiple Axis workspaces into a single,
explicit, system-level registry.

Federations exist to:
- manage real co-dependence
- prevent accidental coupling
- give AI a complete system view

---

## 2. Federation Manifest (`Axis.fed.toml`)

```toml
[federation]
name = "local.system"
profile = "Surface_0_7_Strict"

[workspaces]
pricing   = { path = "workspaces/pricing" }
risk      = { path = "workspaces/risk" }
analytics = { path = "workspaces/analytics" }

[visibility]
pricing   = ["risk"]
risk      = ["analytics"]
analytics = []

[cycles]
groups = [
  ["pricing", "risk"]
]
````

---

## 3. Visibility Rules (Normative)

* Visibility is explicit and directional
* A workspace may call only functions exported by visible workspaces
* Visibility is enforced during registry generation

---

## 4. Circular Dependencies and Cycle Groups (Normative)

Dependency cycles between workspaces are forbidden unless explicitly declared.

### 4.1 Cycle Groups

A cycle group:

* lists all participating workspaces
* defines a strongly connected component
* grants symmetric visibility within the group

Rules:

1. Undeclared cycles MUST cause failure
2. Cycle groups MUST be explicit
3. Cycle groups MUST NOT overlap
4. Outside a cycle group, the group behaves as a single unit

---

## 5. Federation Registry Generation

The federation tool MUST:

1. Load workspace registries
2. Enforce visibility rules
3. Validate cycle declarations
4. Validate global uniqueness
5. Emit one flat registry

Output:

```
target/axis/federation.axreg
```

---

## 6. Name Conflicts Across Workspaces (Normative)

If two federated workspaces export the same function name:

* Federation registry generation MUST fail
* Cycle groups do not permit name conflicts
* No overrides are allowed

---

## 7. Versioning Boundary (Normative)

Federations operate on resolved workspaces.

Versioning is handled outside federations.
Federations MUST NOT introduce version selection logic.

---

## 8. Generated Registry Immutability

Federation registries are build artifacts.

* Manual modification is undefined behavior
* Regeneration MUST be deterministic

---

## 9. Failure Philosophy

Axis prefers early, structural failure over partial execution.

If federation composition is invalid:

* execution MUST NOT proceed
* no best-effort behavior is permitted

---

## Guiding Principle

When systems are co-dependent, make it explicit.
When composition is explicit, scale is manageable.

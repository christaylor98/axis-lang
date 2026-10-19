//! `.axreg` text projection loader (Registry Format §8). The text form is
//! lossy, never hashed, and lowers to the canonical binary of §§3–6.
//!
//! Lowering determinism: canonical field order is fixed by the encoder
//! regardless of authoring order; comments / whitespace / declaration ordering
//! are stripped; names resolve to identities and never appear in canonical
//! bytes. Two projections differing only in names, comments, layout, or field
//! order MUST lower to identical canonical bytes (Format §8 conformance).
//!
//! Identity rule (locked in intent): a function's minted identity is AUTHORED
//! LITERALLY in the `fn` block (`identity 0x…`); the loader NEVER mints at
//! lowering time. Type identities are computed (content hash of canonical
//! `TypeDef`) and never written.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::fabric::hash::{Hash256, HashParseError};

use super::codec::{encode_fn_genesis, encode_record, type_identity};
use super::fold::{CallbackContract, ResolveError, Scope};
use super::record::{
    CompositeBody, ContractAppend, Disposition, EffectClass, FnGenesis, FnKind, Implementation,
    LeafFacts, PrimCode, Record, TypeDef, TypeShape,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Parse { line: usize, msg: String },
    UnknownName { line: usize, name: String },
    BadHash { line: usize, err: HashParseError },
    MissingField { block: String, field: &'static str },
    DuplicateName(String),
    Resolve(ResolveError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Parse { line, msg } => write!(f, "line {}: {}", line, msg),
            LoadError::UnknownName { line, name } => {
                write!(f, "line {}: unknown name '{}'", line, name)
            }
            LoadError::BadHash { line, err } => write!(f, "line {}: bad hash: {}", line, err),
            LoadError::MissingField { block, field } => {
                write!(f, "block '{}': missing required field '{}'", block, field)
            }
            LoadError::DuplicateName(n) => write!(f, "duplicate projection name '{}'", n),
            LoadError::Resolve(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<ResolveError> for LoadError {
    fn from(e: ResolveError) -> Self {
        LoadError::Resolve(e)
    }
}

// ───────────────────────────── tokenisation ─────────────────────────────────

/// One non-blank, comment-stripped logical line.
#[derive(Debug, Clone)]
struct Line {
    n: usize,
    text: String,
}

fn strip_lines(src: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let no_comment = match raw.find("//") {
            Some(p) => &raw[..p],
            None => raw,
        };
        let trimmed = no_comment.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(Line {
            n: i + 1,
            text: trimmed.to_string(),
        });
    }
    out
}

fn parse_hash(s: &str, line: usize) -> Result<Hash256, LoadError> {
    Hash256::from_hex(s).map_err(|err| LoadError::BadHash { line, err })
}

/// Strip leading/trailing parens/brackets and split a comma-separated list,
/// trimming each item.
fn split_csv(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

fn parse_paren_list(s: &str, line: usize) -> Result<Vec<String>, LoadError> {
    let s = s.trim();
    if !s.starts_with('(') || !s.ends_with(')') {
        return Err(LoadError::Parse {
            line,
            msg: format!("expected `(...)`, got `{}`", s),
        });
    }
    Ok(split_csv(&s[1..s.len() - 1]))
}

// ───────────────────────────── parsed-AST ───────────────────────────────────

#[derive(Debug, Clone)]
enum TypeBody {
    Prim(PrimCode),
    Product(Vec<String>),     // field type names
    Sum(Vec<(String, Vec<String>)>), // ctor name + field type names
    List(String),             // element type name
}

#[derive(Debug, Clone)]
struct TypeDecl {
    name: String,
    body: TypeBody,
    line: usize,
}

#[derive(Debug, Clone, Default)]
struct FnDecl {
    name: String,
    line: usize,
    identity: Option<Hash256>,
    kind: Option<FnKind>,
    inputs: Option<Vec<String>>,
    output: Option<String>,
    effect: Option<EffectClass>,
    deterministic: Option<bool>,
    idempotent: Option<bool>,
    disposition: Option<Disposition>,
    body_hash: Option<Hash256>,
    contract: Option<(u64, Vec<String>)>, // (generation, clause names)
    oracle: Option<Hash256>,
    variadic: Option<bool>,
    /// One per `callback <pos> (<in>) -> <out>` line in the block.
    callbacks: Vec<CallbackDecl>,
}

/// A `callback` line before type-name resolution.
#[derive(Debug, Clone)]
struct CallbackDecl {
    position: u32,
    inputs: Vec<String>,
    output: String,
    line: usize,
}

#[derive(Debug, Clone)]
struct ImplDecl {
    line: usize,
    fn_name: String,
    satisfies: u64,
    artifact: Hash256,
}

#[derive(Debug, Default)]
struct Parsed {
    types: Vec<TypeDecl>,
    fns: Vec<FnDecl>,
    impls: Vec<ImplDecl>,
}

// ─────────────────────────────── parsing ────────────────────────────────────

fn parse(src: &str) -> Result<Parsed, LoadError> {
    let lines = strip_lines(src);
    let mut out = Parsed::default();
    let mut i = 0;
    while i < lines.len() {
        let ln = &lines[i];
        let head = ln.text.split_whitespace().next().unwrap_or("");
        match head {
            "registry" => {
                // projection header — ignored semantically.
                i += 1;
            }
            "type" => {
                out.types.push(parse_type_line(ln)?);
                i += 1;
            }
            "fn" => {
                let (decl, consumed) = parse_fn_block(&lines[i..])?;
                out.fns.push(decl);
                i += consumed;
            }
            "impl" => {
                out.impls.push(parse_impl_line(ln)?);
                i += 1;
            }
            // Foreign-but-known block types (parsed by other tools sharing the
            // file, e.g. the bridge's `channel` blocks): scan to the block's own
            // `end` and discard. This tolerance is an explicit ALLOWLIST, not
            // content-blind: an arbitrary unrecognized token used to be treated
            // as a block head and silently swallowed every line through the next
            // `end` — a malformed comment (`#`) before the first `fn` ate that
            // entire entry and the registry loaded one fn short. A registry that
            // silently drops entries is a closure oracle that silently authorizes
            // deletions (AXVERITY_GRAPH_CORE_V1), so unknown tokens now FAIL LOUD.
            "channel" => {
                let mut j = i + 1;
                while j < lines.len() && lines[j].text != "end" {
                    j += 1;
                }
                if j >= lines.len() {
                    return Err(LoadError::Parse {
                        line: ln.n,
                        msg: format!("unexpected directive `{}`: missing `end`", ln.text),
                    });
                }
                i = j + 1;
            }
            other => {
                return Err(LoadError::Parse {
                    line: ln.n,
                    msg: format!(
                        "unrecognized top-level directive `{}` — not a known block \
(registry/type/fn/impl/channel). Refusing to skip: a silently discarded block \
would shrink the registry without an error (note: comments are `//`, not `#`)",
                        other
                    ),
                });
            }
        }
    }
    Ok(out)
}

fn parse_type_line(ln: &Line) -> Result<TypeDecl, LoadError> {
    // `type <Name> = prim <code>`
    // `type <Name> = product (T1, T2)`
    // `type <Name> = sum [ Ctor1(T1, T2) | Ctor2(T3) ]`
    let rest = ln.text.strip_prefix("type ").unwrap().trim();
    let eq = rest.find('=').ok_or_else(|| LoadError::Parse {
        line: ln.n,
        msg: "missing `=` in type decl".into(),
    })?;
    let name = rest[..eq].trim().to_string();
    let body_src = rest[eq + 1..].trim();
    let body = if let Some(p) = body_src.strip_prefix("prim ") {
        let p = p.trim();
        let code = match p {
            "unit" => PrimCode::Unit,
            "bool" => PrimCode::Bool,
            "int" => PrimCode::Int,
            "float" => PrimCode::Float,
            "bytes" => PrimCode::Bytes,
            "text" => PrimCode::Text,
            "value" => PrimCode::Value,
            "dec" => PrimCode::Dec,
            "fn" => PrimCode::Fn,
            "param" => PrimCode::Param,
            other => {
                return Err(LoadError::Parse {
                    line: ln.n,
                    msg: format!("unknown primitive `{}`", other),
                })
            }
        };
        TypeBody::Prim(code)
    } else if let Some(p) = body_src.strip_prefix("product") {
        TypeBody::Product(parse_paren_list(p.trim(), ln.n)?)
    } else if let Some(p) = body_src.strip_prefix("sum") {
        let p = p.trim();
        if !p.starts_with('[') || !p.ends_with(']') {
            // This parser is line-oriented: a `type` declaration is ONE line,
            // so a sum wrapped across lines for readability arrives here with
            // an opening `[` and no closing one. The bare "expected `[ ... ]`"
            // sent a reader looking for a bracket-syntax mistake in text whose
            // brackets are fine, and was misread once as a declaration-ORDER
            // problem — forward references are legal (the loader resolves the
            // type graph order-independently; `type Value = sum [ AsDec(Dec) ]`
            // above `type Dec = prim dec` loads), so moving the declaration
            // "fixed" it only because the move re-typed it onto one line.
            // Name the real constraint instead.
            let unbalanced = p.starts_with('[') && !p.ends_with(']');
            return Err(LoadError::Parse {
                line: ln.n,
                msg: if unbalanced {
                    "unterminated `sum [ ... ]`: a type declaration must be on ONE line, and \
                     this one opens `[` without closing it. Wrapping the arms \
                     across lines is what breaks it — join them. (Declaration \
                     order is not the problem: a sum arm may reference a type \
                     declared further down the file.)"
                        .into()
                } else {
                    "expected `[ ... ]` after `sum`".to_string()
                },
            });
        }
        let inner = &p[1..p.len() - 1];
        let mut ctors = Vec::new();
        for ctor_src in inner.split('|') {
            let ctor_src = ctor_src.trim();
            if ctor_src.is_empty() {
                continue;
            }
            // `Name(T1, T2)` or `Name` (no fields)
            let (cname, fields) = match ctor_src.find('(') {
                Some(p) => {
                    let cname = ctor_src[..p].trim().to_string();
                    let fields = parse_paren_list(&ctor_src[p..], ln.n)?;
                    (cname, fields)
                }
                None => (ctor_src.to_string(), Vec::new()),
            };
            ctors.push((cname, fields));
        }
        TypeBody::Sum(ctors)
    } else if let Some(p) = body_src.strip_prefix("list") {
        let elem = p.trim().to_string();
        if elem.is_empty() {
            return Err(LoadError::Parse {
                line: ln.n,
                msg: "expected element type after `list`".into(),
            });
        }
        TypeBody::List(elem)
    } else {
        return Err(LoadError::Parse {
            line: ln.n,
            msg: format!("unknown type body `{}`", body_src),
        });
    };
    Ok(TypeDecl {
        name,
        body,
        line: ln.n,
    })
}

fn parse_fn_block(lines: &[Line]) -> Result<(FnDecl, usize), LoadError> {
    let head = &lines[0];
    let name = head
        .text
        .strip_prefix("fn ")
        .ok_or_else(|| LoadError::Parse {
            line: head.n,
            msg: "expected `fn <name>`".into(),
        })?
        .trim()
        .to_string();
    let mut decl = FnDecl {
        name,
        line: head.n,
        ..FnDecl::default()
    };
    let mut i = 1;
    while i < lines.len() {
        let ln = &lines[i];
        if ln.text == "end" {
            return Ok((decl, i + 1));
        }
        let mut parts = ln.text.splitn(2, char::is_whitespace);
        let kw = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        match kw {
            "identity" => decl.identity = Some(parse_hash(rest, ln.n)?),
            "kind" => {
                decl.kind = Some(match rest {
                    "leaf" => FnKind::Leaf,
                    "composite" => FnKind::Composite,
                    other => {
                        return Err(LoadError::Parse {
                            line: ln.n,
                            msg: format!("unknown kind `{}`", other),
                        })
                    }
                });
            }
            "in" => decl.inputs = Some(parse_paren_list(rest, ln.n)?),
            "out" => decl.output = Some(rest.to_string()),
            "effect" => {
                decl.effect = Some(match rest {
                    "pure" => EffectClass::Pure,
                    "reads" => EffectClass::Reads,
                    "writes" => EffectClass::Writes,
                    "fullIo" => EffectClass::FullIo,
                    "irreversible" => EffectClass::Irreversible,
                    other => {
                        return Err(LoadError::Parse {
                            line: ln.n,
                            msg: format!("unknown effect class `{}`", other),
                        })
                    }
                });
            }
            "deterministic" => decl.deterministic = Some(parse_bool(rest, ln.n)?),
            "idempotent" => decl.idempotent = Some(parse_bool(rest, ln.n)?),
            "variadic" => decl.variadic = Some(parse_bool(rest, ln.n)?),
            "callback" => decl.callbacks.push(parse_callback(rest, ln.n)?),
            "disposition" => {
                decl.disposition = Some(match rest {
                    "interactive" => Disposition::Interactive,
                    "batch" => Disposition::Batch,
                    other => {
                        return Err(LoadError::Parse {
                            line: ln.n,
                            msg: format!("unknown disposition `{}`", other),
                        })
                    }
                });
            }
            "body" => decl.body_hash = Some(parse_hash(rest, ln.n)?),
            "oracle" => decl.oracle = Some(parse_hash(rest, ln.n)?),
            "contract" => {
                // `contract gen <n> [ c1, c2 ]`
                let rest = rest.strip_prefix("gen ").ok_or_else(|| LoadError::Parse {
                    line: ln.n,
                    msg: "expected `contract gen <n> [...]`".into(),
                })?;
                let (gen_s, list_s) = match rest.find('[') {
                    Some(p) => (rest[..p].trim(), &rest[p..]),
                    None => (rest.trim(), "[]"),
                };
                let gen: u64 = gen_s.parse().map_err(|_| LoadError::Parse {
                    line: ln.n,
                    msg: format!("bad generation `{}`", gen_s),
                })?;
                let list_s = list_s.trim();
                if !list_s.starts_with('[') || !list_s.ends_with(']') {
                    return Err(LoadError::Parse {
                        line: ln.n,
                        msg: "expected `[ clauses ]`".into(),
                    });
                }
                let clauses = split_csv(&list_s[1..list_s.len() - 1]);
                decl.contract = Some((gen, clauses));
            }
            "bridge_contract" => {
                // Bridge-only content: content-blind tolerance — scan to
                // this block's own `end`, discard, continue. Does not
                // end the enclosing fn block.
                let mut j = i + 1;
                while j < lines.len() && lines[j].text != "end" {
                    j += 1;
                }
                if j >= lines.len() {
                    return Err(LoadError::Parse {
                        line: ln.n,
                        msg: "bridge_contract: missing `end`".into(),
                    });
                }
                i = j;
            }
            other => {
                return Err(LoadError::Parse {
                    line: ln.n,
                    msg: format!("unknown fn field `{}`", other),
                });
            }
        }
        i += 1;
    }
    Err(LoadError::Parse {
        line: head.n,
        msg: format!("fn `{}`: missing `end`", decl.name),
    })
}

/// Parse a `callback` directive body: `<pos> (<T1>, <T2>) -> <Out>`.
///
/// `<pos>` is the 0-based index of the `Fn`-typed parameter this contract
/// describes; the parenthesised list and the type after `->` are the signature
/// the fn-ref placed in that slot must have. An empty list (`()`) declares a
/// nullary callback. Type names are resolved later, against the same table
/// `in`/`out` use, so a callback may name any type in the loaded set.
fn parse_callback(s: &str, line: usize) -> Result<CallbackDecl, LoadError> {
    let arrow = s.find("->").ok_or_else(|| LoadError::Parse {
        line,
        msg: format!("expected `callback <pos> (<in>) -> <out>`, got `{}`", s),
    })?;
    let lhs = s[..arrow].trim();
    let output = s[arrow + 2..].trim().to_string();
    if output.is_empty() {
        return Err(LoadError::Parse {
            line,
            msg: "callback: missing return type after `->`".into(),
        });
    }
    let paren = lhs.find('(').ok_or_else(|| LoadError::Parse {
        line,
        msg: format!("callback: expected `(...)` after the position, got `{}`", lhs),
    })?;
    let pos_s = lhs[..paren].trim();
    let position: u32 = pos_s.parse().map_err(|_| LoadError::Parse {
        line,
        msg: format!("callback: bad parameter position `{}`", pos_s),
    })?;
    let inputs = parse_paren_list(&lhs[paren..], line)?;
    Ok(CallbackDecl {
        position,
        inputs,
        output,
        line,
    })
}

/// Resolve one parsed `callback` line against `type_ids`, checking that the
/// position it names really is an `Fn`-typed parameter of the enclosing fn.
///
/// A contract on a non-`Fn` slot is an authoring error, not a no-op: the
/// verifier only consults contracts for `Fn` parameters, so such a line would
/// silently do nothing.
fn lower_callback(
    cb: &CallbackDecl,
    fn_name: &str,
    fn_inputs: &[Hash256],
    type_ids: &HashMap<String, Hash256>,
) -> Result<CallbackContract, LoadError> {
    let fn_ty = type_identity(&TypeDef {
        shape: TypeShape::Primitive(PrimCode::Fn),
    });
    match fn_inputs.get(cb.position as usize) {
        None => {
            return Err(LoadError::Parse {
                line: cb.line,
                msg: format!(
                    "fn `{}`: callback position {} is out of range — `in (...)` declares {} parameter(s)",
                    fn_name,
                    cb.position,
                    fn_inputs.len()
                ),
            })
        }
        Some(&declared) if declared != fn_ty => {
            return Err(LoadError::Parse {
                line: cb.line,
                msg: format!(
                    "fn `{}`: callback position {} is not an `Fn` parameter — \
                     a callback contract only applies to an `Fn`-typed slot",
                    fn_name, cb.position
                ),
            })
        }
        Some(_) => {}
    }
    let inputs = resolve_type_names(&cb.inputs, type_ids, cb.line)?;
    let output = *type_ids
        .get(&cb.output)
        .ok_or_else(|| LoadError::UnknownName {
            line: cb.line,
            name: cb.output.clone(),
        })?;
    Ok(CallbackContract {
        position: cb.position,
        inputs,
        output,
    })
}

/// Resolve every `callback` line of every fn in `fns` into scope contracts.
/// Shared by the single-file (`lower`) and multi-file (`lower_parsed`) paths so
/// the two cannot drift.
fn lower_callback_contracts(
    fns: &[FnDecl],
    fn_ids: &HashMap<String, Hash256>,
    type_ids: &HashMap<String, Hash256>,
    scope: &mut Scope,
) -> Result<(), LoadError> {
    for f in fns {
        if f.callbacks.is_empty() {
            continue;
        }
        let id = fn_ids[&f.name];
        let inputs = resolve_type_names(
            f.inputs.as_deref().unwrap_or(&[]),
            type_ids,
            f.line,
        )?;
        let mut contracts = Vec::with_capacity(f.callbacks.len());
        for cb in &f.callbacks {
            if contracts
                .iter()
                .any(|c: &CallbackContract| c.position == cb.position)
            {
                return Err(LoadError::Parse {
                    line: cb.line,
                    msg: format!(
                        "fn `{}`: duplicate callback contract for position {}",
                        f.name, cb.position
                    ),
                });
            }
            contracts.push(lower_callback(cb, &f.name, &inputs, type_ids)?);
        }
        scope.hof_callback_contracts.insert(id, contracts);
    }
    Ok(())
}

/// Resolve a list of type names to identities, preserving order.
fn resolve_type_names(
    names: &[String],
    type_ids: &HashMap<String, Hash256>,
    line: usize,
) -> Result<Vec<Hash256>, LoadError> {
    names
        .iter()
        .map(|n| {
            type_ids.get(n).copied().ok_or_else(|| LoadError::UnknownName {
                line,
                name: n.clone(),
            })
        })
        .collect()
}

fn parse_bool(s: &str, line: usize) -> Result<bool, LoadError> {
    match s {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(LoadError::Parse {
            line,
            msg: format!("expected true|false, got `{}`", other),
        }),
    }
}

fn parse_impl_line(ln: &Line) -> Result<ImplDecl, LoadError> {
    // `impl <fn-name> satisfies gen <n> artifact <0x...>`
    let rest = ln.text.strip_prefix("impl ").ok_or_else(|| LoadError::Parse {
        line: ln.n,
        msg: "expected `impl ...`".into(),
    })?;
    let satisfies_pos = rest.find(" satisfies gen ").ok_or_else(|| LoadError::Parse {
        line: ln.n,
        msg: "expected `satisfies gen <n>`".into(),
    })?;
    let fn_name = rest[..satisfies_pos].trim().to_string();
    let after = &rest[satisfies_pos + " satisfies gen ".len()..];
    let artifact_pos = after.find(" artifact ").ok_or_else(|| LoadError::Parse {
        line: ln.n,
        msg: "expected `artifact <0x...>`".into(),
    })?;
    let gen_s = after[..artifact_pos].trim();
    let gen: u64 = gen_s.parse().map_err(|_| LoadError::Parse {
        line: ln.n,
        msg: format!("bad generation `{}`", gen_s),
    })?;
    let artifact_s = after[artifact_pos + " artifact ".len()..].trim();
    let artifact = parse_hash(artifact_s, ln.n)?;
    Ok(ImplDecl {
        line: ln.n,
        fn_name,
        satisfies: gen,
        artifact,
    })
}

// ─────────────────────────────── lowering ───────────────────────────────────

/// Result of lowering an `.axreg` projection: the active scope (records indexed
/// by identity, with projection-only name table) and the canonical record
/// stream in canonical order.
#[derive(Debug, Clone)]
pub struct Lowered {
    pub scope: Scope,
    /// All records emitted, in a canonical order (by tag, then by content
    /// hash). Two projections that differ only in names/comments/layout/field
    /// order produce equal `records` vectors.
    pub records: Vec<Record>,
}

impl Lowered {
    /// Concatenated canonical bytes of every record in the canonical order.
    /// This is the byte stream that Format §8 requires to be equal between two
    /// projections that differ only cosmetically.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        for rec in &self.records {
            buf.extend_from_slice(&encode_record(rec));
        }
        buf
    }
}

/// Lower `.axreg` source text to canonical records + active scope.
pub fn lower(src: &str) -> Result<Lowered, LoadError> {
    let parsed = parse(src)?;

    // ── Phase 1: build the type-name → identity table.
    //
    // Types may reference other types; the projection requires types to be
    // declared before use (no forward references), which keeps lowering a
    // single forward pass and makes identity computation order-free.
    let mut type_ids: HashMap<String, Hash256> = HashMap::new();
    let mut type_records: Vec<TypeDef> = Vec::new();
    for td in &parsed.types {
        let body = lower_type_body(&td.body, &type_ids, td.line)?;
        let rec = TypeDef { shape: body };
        let id = type_identity(&rec);
        if type_ids.insert(td.name.clone(), id).is_some() {
            return Err(LoadError::DuplicateName(td.name.clone()));
        }
        type_records.push(rec);
    }

    // ── Phase 2: function identities are AUTHORED (never minted at load time).
    let mut fn_ids: HashMap<String, Hash256> = HashMap::new();
    for f in &parsed.fns {
        let id = f.identity.ok_or_else(|| LoadError::MissingField {
            block: format!("fn {}", f.name),
            field: "identity",
        })?;
        if fn_ids.insert(f.name.clone(), id).is_some() {
            return Err(LoadError::DuplicateName(f.name.clone()));
        }
    }

    // ── Phase 3: emit the six record kinds in canonical form.
    let mut records: Vec<Record> = Vec::new();
    for t in type_records {
        records.push(Record::Type(t));
    }
    for f in &parsed.fns {
        let fg = lower_fn_genesis(f, &type_ids)?;
        records.push(Record::FnGenesis(fg.clone()));

        match fg.kind {
            FnKind::Leaf => {
                records.push(Record::LeafFacts(lower_leaf_facts(f, &fg)?));
            }
            FnKind::Composite => {
                let body = f.body_hash.ok_or_else(|| LoadError::MissingField {
                    block: format!("fn {}", f.name),
                    field: "body",
                })?;
                records.push(Record::CompositeBody(CompositeBody {
                    identity: fg.identity,
                    body,
                }));
            }
        }

        if let Some((gen, clauses)) = &f.contract {
            let added = resolve_names(clauses, &fn_ids, f.line)?;
            // `prev` for generation 1 is content_hash(encode_fn_genesis(fg)).
            // For higher generations the projection only declares the *highest*
            // generation in Phase 1; chaining intermediate appends is a future
            // extension. We support gen 1 by chaining against the genesis hash.
            // (Generations > 1 with no intermediate records would fail the fold
            // — that is desirable: the chain must be complete.)
            let prev = if *gen == 1 {
                crate::fabric::hash::content_hash(&encode_fn_genesis(&fg))
            } else {
                return Err(LoadError::Parse {
                    line: f.line,
                    msg: format!(
                        "contract gen {} > 1 requires explicit prior appends \
                         (multi-generation projection not yet supported)",
                        gen
                    ),
                });
            };
            records.push(Record::ContractAppend(ContractAppend {
                identity: fg.identity,
                prev,
                generation: *gen,
                added,
                oracle: f.oracle.unwrap_or(Hash256::ZERO),
            }));
        }
    }
    for im in &parsed.impls {
        let id = fn_ids
            .get(&im.fn_name)
            .copied()
            .ok_or_else(|| LoadError::UnknownName {
                line: im.line,
                name: im.fn_name.clone(),
            })?;
        records.push(Record::Implementation(Implementation {
            identity: id,
            satisfies: im.satisfies,
            artifact: im.artifact,
        }));
    }

    // ── Phase 4: build the scope and bind projection names.
    let mut scope = Scope::new();
    for rec in &records {
        scope.insert(rec.clone())?;
    }
    for (name, id) in type_ids.iter() {
        scope.bind_name(name.clone(), *id)?;
    }
    for (name, id) in fn_ids.iter() {
        scope.bind_name(name.clone(), *id)?;
    }
    // Callback contracts are scope-only (like `names`): they carry no record
    // and so never reach `records` or the canonical byte stream below.
    lower_callback_contracts(&parsed.fns, &fn_ids, &type_ids, &mut scope)?;

    // ── Phase 5: canonicalise record order — (tag, content-hash) ascending.
    //
    // Authoring order has no canonical meaning; sorting by (tag, hash) yields
    // a stable byte stream from any equivalent projection.
    records.sort_by(|a, b| {
        let ta = record_tag(a);
        let tb = record_tag(b);
        ta.cmp(&tb).then_with(|| {
            let ha = super::codec::record_content_hash(a);
            let hb = super::codec::record_content_hash(b);
            ha.cmp(&hb)
        })
    });

    Ok(Lowered { scope, records })
}

/// Lower multiple `.axreg` sources together as one invocation, resolving
/// type names across all of them as a single namespace instead of requiring
/// every file to redeclare shared primitive types for self-containment.
///
/// A type name declared in more than one of `sources` is a hard error naming
/// both sites — no file is preferred, ranked, or chosen over another. A type
/// declared in one file may be referenced (`in`/`out`, or a product/sum/list
/// field) by any other file in the set without redeclaring it; resolution
/// order is the dependency order of the types themselves, not the order
/// `sources` are listed in.
///
/// Each returned [`Lowered`] corresponds 1:1 to `sources` (same order) and
/// contains only the records that file itself declared — a file that merely
/// *uses* a cross-file type does not claim to own it in its own scope, so
/// the caller's cross-file merge sees each type exactly once.
pub fn lower_many(sources: &[(PathBuf, String)]) -> Result<Vec<Lowered>, String> {
    let mut parsed: Vec<Parsed> = Vec::with_capacity(sources.len());
    for (path, src) in sources {
        let p = parse(src)
            .map_err(|e| format!("Registry {} failed to load: {}", path.display(), e))?;
        parsed.push(p);
    }

    // One global type-name -> owner-file table. Duplicate declaration of the
    // same type name in two files is a hard error naming both sites.
    let mut owner: HashMap<String, &PathBuf> = HashMap::new();
    for (i, p) in parsed.iter().enumerate() {
        let path = &sources[i].0;
        for td in &p.types {
            if let Some(first) = owner.get(&td.name) {
                return Err(format!(
                    "Registry conflict: duplicate type name '{}' — declared in {} and {}",
                    td.name,
                    first.display(),
                    path.display()
                ));
            }
            owner.insert(td.name.clone(), path);
        }
    }

    let global_type_ids = resolve_type_graph(sources, &parsed)?;

    let mut out = Vec::with_capacity(sources.len());
    for (i, (path, _src)) in sources.iter().enumerate() {
        let lowered = lower_parsed(&parsed[i], &global_type_ids)
            .map_err(|e| format!("Registry {} failed to load: {}", path.display(), e))?;
        out.push(lowered);
    }
    Ok(out)
}

/// Resolve every type declared across `parsed` into one global
/// name → identity table, order-independently: a fixed-point worklist that
/// repeatedly lowers whichever pending type declarations have all their
/// referenced names already resolved, regardless of which file declared
/// what or in what order. A round that resolves nothing with declarations
/// still pending means an unknown referenced name or a dependency cycle.
fn resolve_type_graph(
    sources: &[(PathBuf, String)],
    parsed: &[Parsed],
) -> Result<HashMap<String, Hash256>, String> {
    struct PendingType {
        name: String,
        body: TypeBody,
        line: usize,
        path: PathBuf,
    }

    let mut pending: Vec<PendingType> = Vec::new();
    for (i, p) in parsed.iter().enumerate() {
        for td in &p.types {
            pending.push(PendingType {
                name: td.name.clone(),
                body: td.body.clone(),
                line: td.line,
                path: sources[i].0.clone(),
            });
        }
    }

    let mut resolved: HashMap<String, Hash256> = HashMap::new();
    loop {
        if pending.is_empty() {
            return Ok(resolved);
        }
        let mut progressed = false;
        let mut still_pending = Vec::new();
        for item in pending {
            match lower_type_body(&item.body, &resolved, item.line) {
                Ok(shape) => {
                    let id = type_identity(&TypeDef { shape });
                    resolved.insert(item.name.clone(), id);
                    progressed = true;
                }
                Err(_) => still_pending.push(item),
            }
        }
        pending = still_pending;
        if !progressed {
            let stuck: Vec<String> = pending.iter().map(|p| p.name.clone()).collect();
            let first = &pending[0];
            return Err(format!(
                "Registry {}: line {}: cannot resolve type(s) [{}] — unknown referenced type name or dependency cycle",
                first.path.display(),
                first.line,
                stuck.join(", ")
            ));
        }
    }
}

/// Lower one file's already-parsed declarations against a fully-resolved
/// global type table (which may include types declared in other files).
/// Only this file's OWN type and fn declarations are bound into its
/// returned scope's projection-name table.
fn lower_parsed(
    parsed: &Parsed,
    global_type_ids: &HashMap<String, Hash256>,
) -> Result<Lowered, LoadError> {
    let mut local_type_ids: HashMap<String, Hash256> = HashMap::new();
    let mut type_records: Vec<TypeDef> = Vec::new();
    for td in &parsed.types {
        let body = lower_type_body(&td.body, global_type_ids, td.line)?;
        let rec = TypeDef { shape: body };
        let id = type_identity(&rec);
        local_type_ids.insert(td.name.clone(), id);
        type_records.push(rec);
    }

    let mut fn_ids: HashMap<String, Hash256> = HashMap::new();
    for f in &parsed.fns {
        let id = f.identity.ok_or_else(|| LoadError::MissingField {
            block: format!("fn {}", f.name),
            field: "identity",
        })?;
        if fn_ids.insert(f.name.clone(), id).is_some() {
            return Err(LoadError::DuplicateName(f.name.clone()));
        }
    }

    let mut records: Vec<Record> = Vec::new();
    for t in type_records {
        records.push(Record::Type(t));
    }
    for f in &parsed.fns {
        let fg = lower_fn_genesis(f, global_type_ids)?;
        records.push(Record::FnGenesis(fg.clone()));

        match fg.kind {
            FnKind::Leaf => {
                records.push(Record::LeafFacts(lower_leaf_facts(f, &fg)?));
            }
            FnKind::Composite => {
                let body = f.body_hash.ok_or_else(|| LoadError::MissingField {
                    block: format!("fn {}", f.name),
                    field: "body",
                })?;
                records.push(Record::CompositeBody(CompositeBody {
                    identity: fg.identity,
                    body,
                }));
            }
        }

        if let Some((gen, clauses)) = &f.contract {
            let added = resolve_names(clauses, &fn_ids, f.line)?;
            let prev = if *gen == 1 {
                crate::fabric::hash::content_hash(&encode_fn_genesis(&fg))
            } else {
                return Err(LoadError::Parse {
                    line: f.line,
                    msg: format!(
                        "contract gen {} > 1 requires explicit prior appends \
                         (multi-generation projection not yet supported)",
                        gen
                    ),
                });
            };
            records.push(Record::ContractAppend(ContractAppend {
                identity: fg.identity,
                prev,
                generation: *gen,
                added,
                oracle: f.oracle.unwrap_or(Hash256::ZERO),
            }));
        }
    }
    for im in &parsed.impls {
        let id = fn_ids
            .get(&im.fn_name)
            .copied()
            .ok_or_else(|| LoadError::UnknownName {
                line: im.line,
                name: im.fn_name.clone(),
            })?;
        records.push(Record::Implementation(Implementation {
            identity: id,
            satisfies: im.satisfies,
            artifact: im.artifact,
        }));
    }

    let mut scope = Scope::new();
    for rec in &records {
        scope.insert(rec.clone())?;
    }
    for (name, id) in local_type_ids.iter() {
        scope.bind_name(name.clone(), *id)?;
    }
    for (name, id) in fn_ids.iter() {
        scope.bind_name(name.clone(), *id)?;
    }
    // Resolved against the GLOBAL type table, not this file's own: a fn here
    // may name a callback signature in types another file of the set owns,
    // exactly as its `in`/`out` clause already may.
    lower_callback_contracts(&parsed.fns, &fn_ids, global_type_ids, &mut scope)?;

    records.sort_by(|a, b| {
        let ta = record_tag(a);
        let tb = record_tag(b);
        ta.cmp(&tb).then_with(|| {
            let ha = super::codec::record_content_hash(a);
            let hb = super::codec::record_content_hash(b);
            ha.cmp(&hb)
        })
    });

    Ok(Lowered { scope, records })
}

fn record_tag(r: &Record) -> u64 {
    use super::record::*;
    match r {
        Record::Type(_) => TAG_TYPE_DEF,
        Record::FnGenesis(_) => TAG_FN_GENESIS,
        Record::ContractAppend(_) => TAG_CONTRACT_APPEND,
        Record::LeafFacts(_) => TAG_LEAF_FACTS,
        Record::CompositeBody(_) => TAG_COMPOSITE_BODY,
        Record::Implementation(_) => TAG_IMPLEMENTATION,
    }
}

fn lower_type_body(
    body: &TypeBody,
    type_ids: &HashMap<String, Hash256>,
    line: usize,
) -> Result<TypeShape, LoadError> {
    match body {
        TypeBody::Prim(p) => Ok(TypeShape::Primitive(*p)),
        TypeBody::Product(fields) => {
            Ok(TypeShape::Product(resolve_names(fields, type_ids, line)?))
        }
        TypeBody::Sum(ctors) => {
            let mut out = Vec::with_capacity(ctors.len());
            for (_name, fields) in ctors {
                out.push(resolve_names(fields, type_ids, line)?);
            }
            Ok(TypeShape::Sum(out))
        }
        TypeBody::List(elem) => {
            let ids = resolve_names(std::slice::from_ref(elem), type_ids, line)?;
            Ok(TypeShape::List(ids[0]))
        }
    }
}

fn resolve_names(
    names: &[String],
    table: &HashMap<String, Hash256>,
    line: usize,
) -> Result<Vec<Hash256>, LoadError> {
    let mut out = Vec::with_capacity(names.len());
    for n in names {
        let id = table.get(n).copied().ok_or_else(|| LoadError::UnknownName {
            line,
            name: n.clone(),
        })?;
        out.push(id);
    }
    Ok(out)
}

fn lower_fn_genesis(
    f: &FnDecl,
    type_ids: &HashMap<String, Hash256>,
) -> Result<FnGenesis, LoadError> {
    let identity = f.identity.ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "identity",
    })?;
    let kind = f.kind.ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "kind",
    })?;
    let input_names = f.inputs.as_ref().ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "in",
    })?;
    let output_name = f.output.as_ref().ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "out",
    })?;
    let inputs = resolve_names(input_names, type_ids, f.line)?;
    let output = type_ids
        .get(output_name)
        .copied()
        .ok_or_else(|| LoadError::UnknownName {
            line: f.line,
            name: output_name.clone(),
        })?;
    Ok(FnGenesis {
        identity,
        inputs,
        output,
        kind,
        disposition: f.disposition.unwrap_or(Disposition::Unspecified),
        variadic: f.variadic.unwrap_or(false),
    })
}

fn lower_leaf_facts(f: &FnDecl, fg: &FnGenesis) -> Result<LeafFacts, LoadError> {
    let effect_class = f.effect.ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "effect",
    })?;
    // deterministic default is false (intent grammar: "absent => false").
    let deterministic = f.deterministic.unwrap_or(false);
    let idempotent = f.idempotent.ok_or_else(|| LoadError::MissingField {
        block: format!("fn {}", f.name),
        field: "idempotent",
    })?;
    Ok(LeafFacts {
        identity: fg.identity,
        effect_class,
        deterministic,
        idempotent,
    })
}

// encode_composite_body, encode_contract_append, encode_implementation,
// encode_leaf_facts, encode_type removed from imports and _force_encoders_in_scope
// deleted in Bundle 8 (2026-06-08): none were called from this loader path.

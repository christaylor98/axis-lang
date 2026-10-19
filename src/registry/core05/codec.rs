//! Canonical binary encode/decode for the six registry records
//! (Registry Format §§3–6). Every field is written in fixed order with no
//! optional-field omission (absent reference = `Hash256::ZERO`).
//!
//! Identity computations:
//!
//! * `TypeDef`: identity = `content_hash(encode_type(td))`.
//! * `FnGenesis`: identity is the MINTED token stored in the record.
//!   The *content hash* of the genesis record (used for chaining
//!   `ContractAppend.prev` at gen 1, and for ledger pinning) is
//!   `content_hash(encode_fn_genesis(fg))` and is DISTINCT from `identity`.
//! * Other records' content hash is `content_hash(encode_<record>(rec))`.

use crate::fabric::codec::{
    read_bool, read_hash, read_seq_len, read_varint, write_bool, write_hash, write_seq_len,
    write_varint, CodecError, Reader,
};
use crate::fabric::hash::{content_hash, Hash256};

use super::record::{
    CompositeBody, ContractAppend, Disposition, EffectClass, FnGenesis, FnKind, Implementation,
    LeafFacts, PrimCode, Record, TypeDef, TypeShape, TAG_COMPOSITE_BODY, TAG_CONTRACT_APPEND,
    TAG_FN_GENESIS, TAG_IMPLEMENTATION, TAG_LEAF_FACTS, TAG_TYPE_DEF,
};

/// Decode error specific to registry records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordCodecError {
    Codec(CodecError),
    UnknownTag(u64),
    UnknownShapeKind(u64),
    UnknownPrimCode(u64),
    UnknownFnKind(u64),
    UnknownDisposition(u64),
    UnknownEffectClass(u64),
}

impl From<CodecError> for RecordCodecError {
    fn from(e: CodecError) -> Self {
        RecordCodecError::Codec(e)
    }
}

impl std::fmt::Display for RecordCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordCodecError::Codec(e) => write!(f, "{}", e),
            RecordCodecError::UnknownTag(t) => write!(f, "unknown record tag 0x{:02x}", t),
            RecordCodecError::UnknownShapeKind(k) => write!(f, "unknown shape_kind {}", k),
            RecordCodecError::UnknownPrimCode(c) => write!(f, "unknown prim_code {}", c),
            RecordCodecError::UnknownFnKind(k) => write!(f, "unknown record_kind {}", k),
            RecordCodecError::UnknownDisposition(d) => write!(f, "unknown disposition {}", d),
            RecordCodecError::UnknownEffectClass(e) => write!(f, "unknown effect_class {}", e),
        }
    }
}

impl std::error::Error for RecordCodecError {}

// ───────────────────────────── TypeDef (§3) ─────────────────────────────────

fn write_seq_hash(buf: &mut Vec<u8>, items: &[Hash256]) {
    write_seq_len(buf, items.len());
    for h in items {
        write_hash(buf, h);
    }
}

fn read_seq_hash(r: &mut Reader<'_>) -> Result<Vec<Hash256>, RecordCodecError> {
    let n = read_seq_len(r)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(read_hash(r)?);
    }
    Ok(out)
}

pub fn encode_type(td: &TypeDef) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_TYPE_DEF);
    match &td.shape {
        TypeShape::Primitive(p) => {
            write_varint(&mut buf, 0);
            write_varint(&mut buf, p.as_varint());
        }
        TypeShape::Product(fields) => {
            write_varint(&mut buf, 1);
            write_seq_hash(&mut buf, fields);
        }
        TypeShape::Sum(ctors) => {
            write_varint(&mut buf, 2);
            write_seq_len(&mut buf, ctors.len());
            for ctor in ctors {
                write_seq_hash(&mut buf, ctor);
            }
        }
        TypeShape::List(elem) => {
            write_varint(&mut buf, 3);
            write_hash(&mut buf, elem);
        }
    }
    buf
}

/// Structural type identity: content hash of the canonical TypeDef encoding
/// (Registry Format §3). Two structurally identical types collide to the same
/// identity — correct, dedupes shapes.
pub fn type_identity(td: &TypeDef) -> Hash256 {
    content_hash(&encode_type(td))
}

// ───────────────────────────── FnGenesis (§4) ───────────────────────────────

pub fn encode_fn_genesis(fg: &FnGenesis) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_FN_GENESIS);
    write_hash(&mut buf, &fg.identity);
    write_seq_hash(&mut buf, &fg.inputs);
    write_hash(&mut buf, &fg.output);
    write_varint(&mut buf, fg.kind.as_varint());
    write_varint(&mut buf, fg.disposition.as_varint());
    buf
}

// ─────────────────────────── ContractAppend (§5) ────────────────────────────

pub fn encode_contract_append(ca: &ContractAppend) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_CONTRACT_APPEND);
    write_hash(&mut buf, &ca.identity);
    write_hash(&mut buf, &ca.prev);
    write_varint(&mut buf, ca.generation);
    write_seq_hash(&mut buf, &ca.added);
    write_hash(&mut buf, &ca.oracle);
    buf
}

pub fn decode_contract_append(r: &mut Reader<'_>) -> Result<ContractAppend, RecordCodecError> {
    let tag = read_varint(r)?;
    if tag != TAG_CONTRACT_APPEND {
        return Err(RecordCodecError::UnknownTag(tag));
    }
    let identity = read_hash(r)?;
    let prev = read_hash(r)?;
    let generation = read_varint(r)?;
    let added = read_seq_hash(r)?;
    let oracle = read_hash(r)?;
    Ok(ContractAppend {
        identity,
        prev,
        generation,
        added,
        oracle,
    })
}

// ───────────────────────────── LeafFacts (§6) ───────────────────────────────

pub fn encode_leaf_facts(lf: &LeafFacts) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_LEAF_FACTS);
    write_hash(&mut buf, &lf.identity);
    write_varint(&mut buf, lf.effect_class.as_varint());
    write_bool(&mut buf, lf.deterministic);
    write_bool(&mut buf, lf.idempotent);
    buf
}

pub fn decode_leaf_facts(r: &mut Reader<'_>) -> Result<LeafFacts, RecordCodecError> {
    let tag = read_varint(r)?;
    if tag != TAG_LEAF_FACTS {
        return Err(RecordCodecError::UnknownTag(tag));
    }
    let identity = read_hash(r)?;
    let eff_v = read_varint(r)?;
    let effect_class =
        EffectClass::from_varint(eff_v).ok_or(RecordCodecError::UnknownEffectClass(eff_v))?;
    let deterministic = read_bool(r)?;
    let idempotent = read_bool(r)?;
    Ok(LeafFacts {
        identity,
        effect_class,
        deterministic,
        idempotent,
    })
}

// ────────────────────────── CompositeBody (§6) ──────────────────────────────

pub fn encode_composite_body(cb: &CompositeBody) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_COMPOSITE_BODY);
    write_hash(&mut buf, &cb.identity);
    write_hash(&mut buf, &cb.body);
    buf
}

pub fn decode_composite_body(r: &mut Reader<'_>) -> Result<CompositeBody, RecordCodecError> {
    let tag = read_varint(r)?;
    if tag != TAG_COMPOSITE_BODY {
        return Err(RecordCodecError::UnknownTag(tag));
    }
    let identity = read_hash(r)?;
    let body = read_hash(r)?;
    Ok(CompositeBody { identity, body })
}

// ────────────────────────── Implementation (§6) ─────────────────────────────

pub fn encode_implementation(im: &Implementation) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, TAG_IMPLEMENTATION);
    write_hash(&mut buf, &im.identity);
    write_varint(&mut buf, im.satisfies);
    write_hash(&mut buf, &im.artifact);
    buf
}

pub fn decode_implementation(r: &mut Reader<'_>) -> Result<Implementation, RecordCodecError> {
    let tag = read_varint(r)?;
    if tag != TAG_IMPLEMENTATION {
        return Err(RecordCodecError::UnknownTag(tag));
    }
    let identity = read_hash(r)?;
    let satisfies = read_varint(r)?;
    let artifact = read_hash(r)?;
    Ok(Implementation {
        identity,
        satisfies,
        artifact,
    })
}

// ───────────────────────────── Record (sum) ─────────────────────────────────

pub fn encode_record(rec: &Record) -> Vec<u8> {
    match rec {
        Record::Type(t) => encode_type(t),
        Record::FnGenesis(f) => encode_fn_genesis(f),
        Record::ContractAppend(c) => encode_contract_append(c),
        Record::LeafFacts(l) => encode_leaf_facts(l),
        Record::CompositeBody(c) => encode_composite_body(c),
        Record::Implementation(i) => encode_implementation(i),
    }
}

/// Peek the leading tag varint without consuming any input the caller cares
/// about. Used to dispatch decoding when reading a stream of records.
pub fn decode_record(r: &mut Reader<'_>) -> Result<Record, RecordCodecError> {
    // We peek by saving and restoring through a sub-reader, but Reader is
    // single-pass; instead we duplicate the buffer view via a fresh Reader.
    let saved = r.position();
    let tag = read_varint(r)?;
    // Rewind by constructing a new Reader over the same remaining slice — but
    // Reader exposes no rewind. Simpler: handle each tag inline, re-reading
    // tag-prefixed encodings means each decode_X reads its own tag, so we must
    // give it back the tag. We rebuild a reader on a slice that starts at the
    // saved offset by exposing a small helper here.
    let _ = saved;
    match tag {
        TAG_TYPE_DEF => {
            // Re-decode the shape since we already consumed the tag.
            let shape_kind = read_varint(r)?;
            let shape = match shape_kind {
                0 => {
                    let code = read_varint(r)?;
                    let p = PrimCode::from_varint(code)
                        .ok_or(RecordCodecError::UnknownPrimCode(code))?;
                    TypeShape::Primitive(p)
                }
                1 => TypeShape::Product(read_seq_hash(r)?),
                2 => {
                    let n = read_seq_len(r)?;
                    let mut ctors = Vec::with_capacity(n);
                    for _ in 0..n {
                        ctors.push(read_seq_hash(r)?);
                    }
                    TypeShape::Sum(ctors)
                }
                other => return Err(RecordCodecError::UnknownShapeKind(other)),
            };
            Ok(Record::Type(TypeDef { shape }))
        }
        TAG_FN_GENESIS => {
            let identity = read_hash(r)?;
            let inputs = read_seq_hash(r)?;
            let output = read_hash(r)?;
            let kind_v = read_varint(r)?;
            let kind =
                FnKind::from_varint(kind_v).ok_or(RecordCodecError::UnknownFnKind(kind_v))?;
            let disp_v = read_varint(r)?;
            let disposition = Disposition::from_varint(disp_v)
                .ok_or(RecordCodecError::UnknownDisposition(disp_v))?;
            Ok(Record::FnGenesis(FnGenesis {
                identity,
                inputs,
                output,
                kind,
                disposition,
                variadic: false,
            }))
        }
        TAG_CONTRACT_APPEND => {
            let identity = read_hash(r)?;
            let prev = read_hash(r)?;
            let generation = read_varint(r)?;
            let added = read_seq_hash(r)?;
            let oracle = read_hash(r)?;
            Ok(Record::ContractAppend(ContractAppend {
                identity,
                prev,
                generation,
                added,
                oracle,
            }))
        }
        TAG_LEAF_FACTS => {
            let identity = read_hash(r)?;
            let eff_v = read_varint(r)?;
            let effect_class = EffectClass::from_varint(eff_v)
                .ok_or(RecordCodecError::UnknownEffectClass(eff_v))?;
            let deterministic = read_bool(r)?;
            let idempotent = read_bool(r)?;
            Ok(Record::LeafFacts(LeafFacts {
                identity,
                effect_class,
                deterministic,
                idempotent,
            }))
        }
        TAG_COMPOSITE_BODY => {
            let identity = read_hash(r)?;
            let body = read_hash(r)?;
            Ok(Record::CompositeBody(CompositeBody { identity, body }))
        }
        TAG_IMPLEMENTATION => {
            let identity = read_hash(r)?;
            let satisfies = read_varint(r)?;
            let artifact = read_hash(r)?;
            Ok(Record::Implementation(Implementation {
                identity,
                satisfies,
                artifact,
            }))
        }
        other => Err(RecordCodecError::UnknownTag(other)),
    }
}

/// Content hash of any record's canonical encoding (Format §§3–6).
/// For `Record::FnGenesis` this is the chain-pinning hash, NOT the function's
/// minted identity.
pub fn record_content_hash(rec: &Record) -> Hash256 {
    content_hash(&encode_record(rec))
}

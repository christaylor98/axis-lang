//! Core IR 0.6 in-memory model, builder, canonical serializer, and the .coreir file codec
//! (core_ir_spec/axis-core-ir-0.6.md). The module keeps its 0.5 name: 0.6 is 0.5 plus blocks, which say
//! where nodes run. Coexists with `core_ir` (0.3) until Phase 3 rewrites lowering.

use std::collections::HashMap;

use crate::fabric::codec::{
    decode_noderef, encode_noderef, read_bytes, read_hash, read_seq_len, write_bytes, write_hash,
    write_seq_len, write_varint, read_varint, CodecError, NodeRefBin, Reader,
};
use crate::fabric::hash::{content_hash, Hash256};
use crate::registry::core05::fold::Scope;

const VERSION: &str = "0.6";

// ═══════════════════════════════════════════════════════════════════════════
// DATA MODEL
// ═══════════════════════════════════════════════════════════════════════════

/// An edge in the Core IR graph: index into the node table or constant pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeRef {
    Node(u32),
    Pool(u32),
}

impl From<NodeRef> for NodeRefBin {
    fn from(r: NodeRef) -> NodeRefBin {
        match r {
            NodeRef::Node(i) => NodeRefBin::Node(i),
            NodeRef::Pool(i) => NodeRefBin::Pool(i),
        }
    }
}

impl From<NodeRefBin> for NodeRef {
    fn from(r: NodeRefBin) -> NodeRef {
        match r {
            NodeRefBin::Node(i) => NodeRef::Node(i),
            NodeRefBin::Pool(i) => NodeRef::Pool(i),
        }
    }
}

/// Inline constant value: type identity + payload bytes (per Registry Format §10).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstantPoolEntry {
    pub def_hash: Hash256,
    pub payload: Vec<u8>,
}

/// Core IR 0.6 node kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    CCall {
        /// Human-readable function name — mandatory for debuggability.
        target_name: String,
        /// 256-bit content identity — the authoritative key for registry lookup.
        target_identity: Hash256,
        args: Vec<NodeRef>,
    },
    CIf {
        cond: NodeRef,
        then_: NodeRef,
        else_: NodeRef,
    },
    /// Determinacy discharge gate. A pure marker node with no operands.
    ///
    /// An Irreversible CCall must be dominated by a CDeterminate on every
    /// control-flow path from the bundle entry to that node; otherwise
    /// verification fails (fail-closed). Domination is established when
    /// CDeterminate is a required data ancestor of the Irreversible node
    /// on every execution path — not merely present in the bundle.
    ///
    /// Produces a value of type Unit (a discharge token). The M1 `determine`
    /// keyword lowers to this node (see nf_lowering.rs). To gate an Irreversible
    /// CCall it must be written in ARGUMENT position — domination is established
    /// only via arg dataflow, so a bare `let _ = determine` statement does not gate.
    CDeterminate,
    /// Opens a block (Core IR 0.6). No fields. A block whose `CEnd` is a `CIf`'s `then`/`else` is an arm
    /// and runs only when that `CIf` selects it; every other block is plain grouping and always runs.
    /// A node runs exactly when every block enclosing it runs, in index order; nothing moves a node.
    CBlock,
    /// Closes the innermost open block and yields its value (Core IR 0.6). The value is mandatory:
    /// pool Unit when the block returns nothing.
    CEnd { value: NodeRef },
}

/// The canonical binary form of a Core IR 0.5 program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreBundle {
    /// Wire-format transport marker only; excluded from the identity hash.
    pub version: String,
    pub constant_pool: Vec<ConstantPoolEntry>,
    /// Flat, topologically-ordered node table.
    /// Invariant: for every NodeRef::Node(i) inside node at index j, i < j.
    pub nodes: Vec<Node>,
    /// The bundle's semantic value — the ref a consumer must emit/return.
    /// Unconstrained by kind: may point at a `CCall`, `CIf`, `CDeterminate`
    /// node, or directly at a bare pool entry (a literal or param tail with
    /// no node of its own). Without this field, "the last node in `nodes`"
    /// was the only available signal, which is simply wrong whenever the
    /// source's tail expression is a bare literal/VarRef following a real
    /// call — that ref never gets its own node slot. See BRANCH_SCOPING_V1's
    /// sibling defect in axis-codegen-bridge-rs for the exact failure mode.
    pub result: NodeRef,
}

// ═══════════════════════════════════════════════════════════════════════════
// CANONICAL SERIALIZATION
// ═══════════════════════════════════════════════════════════════════════════
//
// Version is excluded — it is a wire marker, not part of the identity hash.
// Encoding uses fabric::codec primitives: deterministic, minimal, no framing.

fn push_noderef_bytes(buf: &mut Vec<u8>, r: NodeRef) {
    encode_noderef(buf, r.into());
}

fn push_node_bytes(buf: &mut Vec<u8>, node: &Node) {
    match node {
        Node::CCall { target_name, target_identity, args } => {
            write_varint(buf, 0); // sum tag: CCall = 0
            write_bytes(buf, target_name.as_bytes()); // name before hash (canonical order)
            write_hash(buf, target_identity);
            write_seq_len(buf, args.len());
            for arg in args {
                push_noderef_bytes(buf, *arg);
            }
        }
        Node::CIf { cond, then_, else_ } => {
            write_varint(buf, 1); // sum tag: CIf = 1
            push_noderef_bytes(buf, *cond);
            push_noderef_bytes(buf, *then_);
            push_noderef_bytes(buf, *else_);
        }
        Node::CDeterminate => {
            write_varint(buf, 2); // sum tag: CDeterminate = 2; no fields
        }
        Node::CBlock => {
            write_varint(buf, 3); // sum tag: CBlock = 3; no fields (0.6)
        }
        Node::CEnd { value } => {
            write_varint(buf, 4); // sum tag: CEnd = 4 (0.6)
            push_noderef_bytes(buf, *value);
        }
    }
}

/// Canonical bytes of a bundle (excludes version; hash these for the bundle identity).
pub fn serialize_canonical(bundle: &CoreBundle) -> Vec<u8> {
    let mut buf = Vec::new();
    write_seq_len(&mut buf, bundle.constant_pool.len());
    for entry in &bundle.constant_pool {
        write_hash(&mut buf, &entry.def_hash);
        write_bytes(&mut buf, &entry.payload);
    }
    write_seq_len(&mut buf, bundle.nodes.len());
    for node in &bundle.nodes {
        push_node_bytes(&mut buf, node);
    }
    push_noderef_bytes(&mut buf, bundle.result);
    buf
}

/// Content hash of the canonical encoding — this IS the bundle's identity.
pub fn bundle_identity(bundle: &CoreBundle) -> Hash256 {
    content_hash(&serialize_canonical(bundle))
}

// ── canonical decode ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalDecodeError {
    Codec(CodecError),
    UnknownNodeKind(u64),
    ForwardEdge { referrer: u32, target: u32 },
    PoolIndexOutOfRange { index: u32, pool_len: u32 },
    Block(BlockError),
}

impl From<CodecError> for CanonicalDecodeError {
    fn from(e: CodecError) -> Self {
        CanonicalDecodeError::Codec(e)
    }
}

impl std::fmt::Display for CanonicalDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanonicalDecodeError::Codec(e) => write!(f, "codec: {}", e),
            CanonicalDecodeError::UnknownNodeKind(k) => write!(f, "unknown node kind tag {}", k),
            CanonicalDecodeError::ForwardEdge { referrer, target } => {
                write!(f, "forward edge at node {}: references node {}", referrer, target)
            }
            CanonicalDecodeError::PoolIndexOutOfRange { index, pool_len } => {
                write!(f, "pool index {} out of range (len {})", index, pool_len)
            }
            CanonicalDecodeError::Block(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for CanonicalDecodeError {}

fn decode_edge_canonical(
    r: &mut Reader<'_>,
    node_idx: u32,
    pool_len: u32,
) -> Result<NodeRef, CanonicalDecodeError> {
    let nr = decode_noderef(r)?;
    match nr {
        NodeRefBin::Node(i) => {
            if i >= node_idx {
                return Err(CanonicalDecodeError::ForwardEdge { referrer: node_idx, target: i });
            }
        }
        NodeRefBin::Pool(i) => {
            if i >= pool_len {
                return Err(CanonicalDecodeError::PoolIndexOutOfRange {
                    index: i,
                    pool_len,
                });
            }
        }
    }
    Ok(NodeRef::from(nr))
}

fn decode_node_canonical(
    r: &mut Reader<'_>,
    node_idx: u32,
    pool_len: u32,
) -> Result<Node, CanonicalDecodeError> {
    match read_varint(r)? {
        0 => {
            let name_bytes = read_bytes(r)?.to_vec();
            let target_name = String::from_utf8(name_bytes).map_err(|_| {
                CanonicalDecodeError::Codec(CodecError::Truncated)
            })?;
            let h = read_hash(r)?;
            let arg_count = read_seq_len(r)?;
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                args.push(decode_edge_canonical(r, node_idx, pool_len)?);
            }
            Ok(Node::CCall { target_name, target_identity: h, args })
        }
        1 => {
            let cond = decode_edge_canonical(r, node_idx, pool_len)?;
            let then_ = decode_edge_canonical(r, node_idx, pool_len)?;
            let else_ = decode_edge_canonical(r, node_idx, pool_len)?;
            Ok(Node::CIf { cond, then_, else_ })
        }
        2 => Ok(Node::CDeterminate),
        3 => Ok(Node::CBlock),
        4 => Ok(Node::CEnd { value: decode_edge_canonical(r, node_idx, pool_len)? }),
        k => Err(CanonicalDecodeError::UnknownNodeKind(k)),
    }
}

/// Decode a bundle from canonical bytes (the inverse of `serialize_canonical`), then run the 0.6 block
/// verifier (`verify_blocks`): a bundle whose blocks are malformed is refused at load, naming the node.
pub fn deserialize_canonical(bytes: &[u8]) -> Result<CoreBundle, CanonicalDecodeError> {
    let mut r = Reader::new(bytes);

    let pool_count = read_seq_len(&mut r)?;
    let mut constant_pool = Vec::with_capacity(pool_count);
    for _ in 0..pool_count {
        let def_hash = read_hash(&mut r)?;
        let payload = read_bytes(&mut r)?.to_vec();
        constant_pool.push(ConstantPoolEntry { def_hash, payload });
    }

    let node_count = read_seq_len(&mut r)?;
    let mut nodes = Vec::with_capacity(node_count);
    let pool_len = pool_count as u32;
    for i in 0..node_count {
        nodes.push(decode_node_canonical(&mut r, i as u32, pool_len)?);
    }
    let result = decode_edge_canonical(&mut r, node_count as u32, pool_len)?;

    let bundle = CoreBundle {
        version: VERSION.to_string(),
        constant_pool,
        nodes,
        result,
    };
    verify_blocks(&bundle).map_err(CanonicalDecodeError::Block)?;
    Ok(bundle)
}

// ═══════════════════════════════════════════════════════════════════════════
// AXBI FILE FORMAT  (.coreir)
// ═══════════════════════════════════════════════════════════════════════════
//
// 6-byte header (magic + IR version) followed by the canonical payload.
// The header is NOT included in the identity hash.
//
//   offset  size  value
//   0       4     0x41 0x58 0x43 0x49  ('A','X','C','I')
//   4       1     ir_major: 0x00
//   5       1     ir_minor: 0x06  (0x05 is refused: recompile from source)
//   6       *     serialize_canonical() output

pub const AXBI_MAGIC: [u8; 4] = *b"AXCI";

/// The refusal of a Core IR 0.5 bundle, from every loader.
pub const REFUSE_05: &str =
    "Core IR 0.5 bundle: it does not record where its nodes run; recompile from source";
pub const AXBI_IR_MAJOR: u8 = 0x00;
pub const AXBI_IR_MINOR: u8 = 0x06;

#[derive(Debug)]
pub enum AxbiError {
    TooShort,
    BadMagic([u8; 4]),
    UnsupportedVersion { major: u8, minor: u8 },
    /// A Core IR 0.5 bundle. It does not say where its nodes run, and that cannot be recovered from
    /// it, so there is no upgrade: it is refused (axis-core-ir-0.6.md, "0.5 bundles").
    Version05,
    Canonical(CanonicalDecodeError),
}

impl From<CanonicalDecodeError> for AxbiError {
    fn from(e: CanonicalDecodeError) -> Self { AxbiError::Canonical(e) }
}

impl std::fmt::Display for AxbiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AxbiError::TooShort => write!(f, "file too short to be .coreir"),
            AxbiError::BadMagic(m) => write!(f, "bad magic: {:?}", m),
            AxbiError::UnsupportedVersion { major, minor } =>
                write!(f, "unsupported IR version {}.{}", major, minor),
            AxbiError::Version05 => write!(f, "{}", REFUSE_05),
            AxbiError::Canonical(e) => write!(f, "canonical decode: {}", e),
        }
    }
}

impl std::error::Error for AxbiError {}

/// Encode a bundle as `.coreir` bytes (6-byte header + canonical payload).
pub fn write_axbi(bundle: &CoreBundle) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + 64);
    out.extend_from_slice(&AXBI_MAGIC);
    out.push(AXBI_IR_MAJOR);
    out.push(AXBI_IR_MINOR);
    out.extend_from_slice(&serialize_canonical(bundle));
    out
}

/// Decode `.coreir` bytes into a `CoreBundle`. Validates magic and IR version.
pub fn read_axbi(data: &[u8]) -> Result<CoreBundle, AxbiError> {
    if data.len() < 6 {
        return Err(AxbiError::TooShort);
    }
    let magic: [u8; 4] = data[0..4].try_into().unwrap();
    if magic != AXBI_MAGIC {
        return Err(AxbiError::BadMagic(magic));
    }
    let (major, minor) = (data[4], data[5]);
    if major == 0 && minor == 0x05 {
        return Err(AxbiError::Version05);
    }
    if major != AXBI_IR_MAJOR || minor != AXBI_IR_MINOR {
        return Err(AxbiError::UnsupportedVersion { major, minor });
    }
    Ok(deserialize_canonical(&data[6..])?)
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON FORMAT  (.axbi.json)
// ═══════════════════════════════════════════════════════════════════════════
//
// Hashes → 64-char lowercase hex. Payload bytes → standard base64 (RFC 4648
// §4, with '=' padding). NodeRefs → {"space":"node"|"pool","index":N}.
// Identity is computed over serialize_canonical(), not over JSON bytes.

#[derive(Debug)]
pub enum JsonError {
    Serde(serde_json::Error),
    MissingField(&'static str),
    BadHash(String),
    BadBase64,
    UnknownKind(String),
    UnknownSpace(String),
    ForwardEdge { referrer: usize, target: usize },
    PoolIndexOutOfRange { index: usize, pool_len: usize },
    Version05,
    Block(BlockError),
}

impl From<serde_json::Error> for JsonError {
    fn from(e: serde_json::Error) -> Self { JsonError::Serde(e) }
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonError::Serde(e)             => write!(f, "json: {}", e),
            JsonError::MissingField(k)      => write!(f, "missing field: {}", k),
            JsonError::BadHash(s)           => write!(f, "bad hash hex: {}", s),
            JsonError::BadBase64            => write!(f, "bad base64 payload"),
            JsonError::UnknownKind(k)       => write!(f, "unknown node kind: {}", k),
            JsonError::UnknownSpace(s)      => write!(f, "unknown noderef space: {}", s),
            JsonError::ForwardEdge { referrer, target } =>
                write!(f, "forward edge at node {}: references {}", referrer, target),
            JsonError::PoolIndexOutOfRange { index, pool_len } =>
                write!(f, "pool index {} out of range (len {})", index, pool_len),
            JsonError::Version05 => write!(f, "{}", REFUSE_05),
            JsonError::Block(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for JsonError {}

// ── base64 (RFC 4648 §4, standard alphabet with padding) ─────────────────

const B64_CHARS: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        let combined = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[(combined >> 18) & 0x3F] as char);
        out.push(B64_CHARS[(combined >> 12) & 0x3F] as char);
        out.push(if chunk.len() > 1 { B64_CHARS[(combined >> 6) & 0x3F] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64_CHARS[combined & 0x3F] as char } else { '=' });
    }
    out
}

fn decode_base64(s: &str) -> Result<Vec<u8>, JsonError> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let val = |c: u8| -> Result<usize, JsonError> {
        match c {
            b'A'..=b'Z' => Ok((c - b'A') as usize),
            b'a'..=b'z' => Ok((c - b'a' + 26) as usize),
            b'0'..=b'9' => Ok((c - b'0' + 52) as usize),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err(JsonError::BadBase64),
        }
    };
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let a = val(bytes[i])?;
        let b = val(bytes[i + 1])?;
        out.push(((a << 2) | (b >> 4)) as u8);
        if i + 2 < bytes.len() {
            let c = val(bytes[i + 2])?;
            out.push(((b << 4) | (c >> 2)) as u8);
            if i + 3 < bytes.len() {
                let d = val(bytes[i + 3])?;
                out.push(((c << 6) | d) as u8);
                i += 4;
            } else { i += 3; }
        } else { i += 2; }
    }
    Ok(out)
}

// ── NodeRef JSON helpers ──────────────────────────────────────────────────

fn noderef_to_json(nr: NodeRef) -> serde_json::Value {
    match nr {
        NodeRef::Pool(i) => serde_json::json!({ "space": "pool", "index": i }),
        NodeRef::Node(i) => serde_json::json!({ "space": "node", "index": i }),
    }
}

fn noderef_from_json(
    v: &serde_json::Value,
    node_idx: usize,
    pool_len: usize,
) -> Result<NodeRef, JsonError> {
    let space = v["space"].as_str().ok_or(JsonError::MissingField("space"))?;
    let index = v["index"].as_u64().ok_or(JsonError::MissingField("index"))? as usize;
    match space {
        "pool" => {
            if index >= pool_len {
                return Err(JsonError::PoolIndexOutOfRange { index, pool_len });
            }
            Ok(NodeRef::Pool(index as u32))
        }
        "node" => {
            if index >= node_idx {
                return Err(JsonError::ForwardEdge { referrer: node_idx, target: index });
            }
            Ok(NodeRef::Node(index as u32))
        }
        other => Err(JsonError::UnknownSpace(other.to_string())),
    }
}

/// Encode a bundle as a JSON string.
pub fn encode_json(bundle: &CoreBundle) -> String {
    use serde_json::{json, Value};

    let pool: Vec<Value> = bundle.constant_pool.iter().map(|e| json!({
        "def_hash": e.def_hash.to_hex(),
        "payload":  encode_base64(&e.payload),
    })).collect();

    let nodes: Vec<Value> = bundle.nodes.iter().map(|node| match node {
        Node::CCall { target_name, target_identity, args } => json!({
            "kind":            "CCall",
            "target_name":     target_name,
            "target_identity": target_identity.to_hex(),
            "args":            args.iter().map(|a| noderef_to_json(*a)).collect::<Vec<_>>(),
        }),
        Node::CIf { cond, then_, else_ } => json!({
            "kind": "CIf",
            "cond": noderef_to_json(*cond),
            "then": noderef_to_json(*then_),
            "else": noderef_to_json(*else_),
        }),
        Node::CDeterminate => json!({ "kind": "CDeterminate" }),
        Node::CBlock => json!({ "kind": "CBlock" }),
        Node::CEnd { value } => json!({ "kind": "CEnd", "value": noderef_to_json(*value) }),
    }).collect();

    serde_json::to_string_pretty(&json!({
        "axis_core_ir":  bundle.version,
        "constant_pool": pool,
        "nodes":         nodes,
        "result":        noderef_to_json(bundle.result),
    })).expect("serde_json serialisation of CoreBundle is infallible")
}

/// Decode a JSON string into a `CoreBundle`.
pub fn decode_json(s: &str) -> Result<CoreBundle, JsonError> {
    let root: serde_json::Value = serde_json::from_str(s)?;
    if root["axis_core_ir"].as_str() == Some("0.5") {
        return Err(JsonError::Version05);
    }

    let pool_arr = root["constant_pool"].as_array()
        .ok_or(JsonError::MissingField("constant_pool"))?;
    let mut constant_pool = Vec::with_capacity(pool_arr.len());
    for entry in pool_arr {
        let hex = entry["def_hash"].as_str().ok_or(JsonError::MissingField("def_hash"))?;
        let def_hash = Hash256::from_hex(hex)
            .map_err(|_| JsonError::BadHash(hex.to_string()))?;
        let b64 = entry["payload"].as_str().ok_or(JsonError::MissingField("payload"))?;
        let payload = decode_base64(b64)?;
        constant_pool.push(ConstantPoolEntry { def_hash, payload });
    }

    let nodes_arr = root["nodes"].as_array()
        .ok_or(JsonError::MissingField("nodes"))?;
    let pool_len = constant_pool.len();
    let mut nodes = Vec::with_capacity(nodes_arr.len());

    for (i, node_v) in nodes_arr.iter().enumerate() {
        let kind = node_v["kind"].as_str().ok_or(JsonError::MissingField("kind"))?;
        match kind {
            "CCall" => {
                let target_name = node_v["target_name"].as_str()
                    .ok_or(JsonError::MissingField("target_name"))?.to_string();
                let id_hex = node_v["target_identity"].as_str()
                    .ok_or(JsonError::MissingField("target_identity"))?;
                let target_identity = Hash256::from_hex(id_hex)
                    .map_err(|_| JsonError::BadHash(id_hex.to_string()))?;
                let args_v = node_v["args"].as_array()
                    .ok_or(JsonError::MissingField("args"))?;
                let mut args = Vec::with_capacity(args_v.len());
                for av in args_v {
                    args.push(noderef_from_json(av, i, pool_len)?);
                }
                nodes.push(Node::CCall { target_name, target_identity, args });
            }
            "CIf" => {
                let cond  = noderef_from_json(&node_v["cond"], i, pool_len)?;
                let then_ = noderef_from_json(&node_v["then"], i, pool_len)?;
                let else_ = noderef_from_json(&node_v["else"], i, pool_len)?;
                nodes.push(Node::CIf { cond, then_, else_ });
            }
            "CDeterminate" => nodes.push(Node::CDeterminate),
            "CBlock" => nodes.push(Node::CBlock),
            "CEnd" => {
                if node_v.get("value").is_none() {
                    return Err(JsonError::MissingField("value"));
                }
                let value = noderef_from_json(&node_v["value"], i, pool_len)?;
                nodes.push(Node::CEnd { value });
            }
            other => return Err(JsonError::UnknownKind(other.to_string())),
        }
    }

    let result = root.get("result")
        .ok_or(JsonError::MissingField("result"))
        .and_then(|v| noderef_from_json(v, nodes.len(), pool_len))?;

    let version = root["axis_core_ir"].as_str().unwrap_or(VERSION).to_string();
    let bundle = CoreBundle { version, constant_pool, nodes, result };
    verify_blocks(&bundle).map_err(JsonError::Block)?;
    Ok(bundle)
}
// ═══════════════════════════════════════════════════════════════════════════

// BUILDER
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuilderError {
    /// A NodeRef::Node edge references an index ≥ the current table size.
    ForwardEdge { target: u32, current_len: u32 },
    /// A NodeRef::Pool edge references an index ≥ the pool size.
    PoolIndexOutOfRange { index: u32, pool_len: u32 },
    /// CCall target_identity not in the active scope — the UNKNOWN gate.
    UnknownTarget(Hash256),
    /// The finished bundle breaks a 0.6 block rule (`verify_blocks`).
    Block(BlockError),
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuilderError::ForwardEdge { target, current_len } => {
                write!(f, "forward edge to node {} (table len {})", target, current_len)
            }
            BuilderError::PoolIndexOutOfRange { index, pool_len } => {
                write!(f, "pool index {} out of range (len {})", index, pool_len)
            }
            BuilderError::UnknownTarget(id) => {
                write!(f, "UNKNOWN gate: {} not in active scope", id)
            }
            BuilderError::Block(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for BuilderError {}

/// Builds a `CoreBundle` enforcing topological order, constant deduplication,
/// and structural sharing (hash-cons on canonical node bytes).
pub struct Builder<'s> {
    constant_pool: Vec<ConstantPoolEntry>,
    pool_index: HashMap<ConstantPoolEntry, u32>,
    nodes: Vec<Node>,
    node_index: HashMap<Vec<u8>, u32>,
    /// Every key put in `node_index`, with its node, in index order -- so closing a block can take back
    /// the ones made inside it.
    node_index_log: Vec<(u32, Vec<u8>)>,
    /// The `CBlock` of every open block, innermost last.
    open_blocks: Vec<u32>,
    scope: Option<&'s Scope>,
}

impl<'s> Builder<'s> {
    pub fn new() -> Self {
        Self {
            constant_pool: Vec::new(),
            pool_index: HashMap::new(),
            nodes: Vec::new(),
            node_index: HashMap::new(),
            node_index_log: Vec::new(),
            open_blocks: Vec::new(),
            scope: None,
        }
    }

    /// Attach a scope; enables the UNKNOWN gate on CCall target_identity.
    pub fn with_scope(mut self, scope: &'s Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Intern a constant. Identical `(def_hash, payload)` pairs share one pool index.
    pub fn intern_constant(&mut self, def_hash: Hash256, payload: Vec<u8>) -> NodeRef {
        let entry = ConstantPoolEntry { def_hash, payload };
        if let Some(&idx) = self.pool_index.get(&entry) {
            return NodeRef::Pool(idx);
        }
        let idx = self.constant_pool.len() as u32;
        self.pool_index.insert(entry.clone(), idx);
        self.constant_pool.push(entry);
        NodeRef::Pool(idx)
    }

    /// Push a node. All edges must point to prior nodes or valid pool entries;
    /// identical nodes (by canonical bytes) are hash-consed to one index —
    /// but ONLY when sharing is semantics-preserving (see below).
    ///
    /// EFFECT-AWARE HASH-CONS (fix for the effect-blind CSE bug found by the
    /// axVerity indexer battle harness, 2026-07-10): the original builder
    /// hash-consed EVERY node by canonical bytes, so two textually identical
    /// effectful CCalls (`io_println(x)` twice; two identical `fs_append_text`
    /// calls) collapsed into ONE node and the second call's side effect was
    /// SILENTLY DROPPED at runtime. Minimal repro: a fn body with
    /// `flood_spin(Int(41))` twice + printlns emitted `41, 99, END` instead of
    /// `41, 41, 99, END`. A CCall is now shareable ONLY when its resolved
    /// callee is a leaf with `effect pure` AND `deterministic true` — same
    /// inputs provably produce the same value and no observable effect, so
    /// merging N calls into one is meaning-preserving. Everything else gets a
    /// distinct node, conservatively:
    ///   * effectful / nondeterministic leaves — the bug class itself;
    ///   * composites (`Facts::PendingDerivation`) — their effect facts are
    ///     not derived at lowering time, so purity cannot be proven here;
    ///   * no scope attached — no way to resolve facts at all.
    /// Non-shareable nodes are also never INSERTED into the index, so nothing
    /// later dedups against them. `CIf` sharing stays byte-keyed and safe by
    /// construction: identical keys require identical child refs, and children
    /// containing effectful calls no longer collapse, so their parents' keys
    /// diverge too. `CDeterminate` is a pure structural marker — unchanged.
    pub fn push_node(&mut self, node: Node) -> Result<NodeRef, BuilderError> {
        let current_idx = self.nodes.len() as u32;
        let pool_len = self.constant_pool.len() as u32;

        let shareable = match &node {
            Node::CCall { target_identity, args, .. } => {
                let mut pure_and_deterministic = false;
                if let Some(scope) = self.scope {
                    let resolved = scope
                        .resolve_fn(*target_identity)
                        .map_err(|_| BuilderError::UnknownTarget(*target_identity))?;
                    if let crate::registry::core05::fold::Facts::Leaf(lf) = &resolved.facts {
                        pure_and_deterministic = lf.effect_class
                            == crate::registry::core05::record::EffectClass::Pure
                            && lf.deterministic;
                    }
                }

                // THE CALLBACK COLOURS THE CALL.
                //
                // A higher-order primitive's own facts describe its plumbing,
                // not the call: `loop_count` runs its callback N times, so
                // `loop_count(n, seed, f)` is pure exactly when `f` is. Reading
                // only the callee's row would make every `loop_count` shareable
                // and collapse two identical loops into one — the 2026-07-10
                // effect-blind CSE bug, on the one primitive whose entire
                // purpose is to run a body repeatedly.
                //
                // Declaring the loops `fullIo` instead would fix that by giving
                // up: a loop over a genuinely pure callback would never share
                // again. Folding the callback's own facts in keeps both halves
                // right, and needs nothing new in the registry — a fn-ref arg
                // is a pool entry whose def_hash is `fn_type_id()` and whose
                // payload IS the callee's 32-byte identity (emit_05.rs
                // `intern_fn_ref`), so it resolves in this same scope.
                //
                // Anything not provably pure closes the gate: a composite
                // callback (`PendingDerivation` — effects not derived at
                // lowering time), an unresolvable one, or a plainly effectful
                // one. Fn is callee-position only, so a callback is always a
                // direct argument and there is no nesting to search.
                if pure_and_deterministic {
                    if let Some(scope) = self.scope {
                        for &arg in args {
                            let cb = match arg {
                                NodeRef::Pool(idx) => self.fn_ref_target(idx),
                                _ => None,
                            };
                            let Some(cb_identity) = cb else { continue };
                            let cb_pure = match scope.resolve_fn(cb_identity) {
                                Ok(r) => match &r.facts {
                                    crate::registry::core05::fold::Facts::Leaf(lf) => {
                                        lf.effect_class
                                            == crate::registry::core05::record::EffectClass::Pure
                                            && lf.deterministic
                                    }
                                    _ => false,
                                },
                                Err(_) => false,
                            };
                            if !cb_pure {
                                pure_and_deterministic = false;
                                break;
                            }
                        }
                    }
                }

                for &arg in args {
                    check_ref(arg, current_idx, pool_len)?;
                }
                pure_and_deterministic
            }
            // Core IR 0.6: a CIf, CBlock or CEnd is a place in the program, never shared. A CIf's arms are
            // the blocks just written, so two equal-looking ifs are two ifs; every CBlock has the same
            // bytes.
            Node::CIf { cond, then_, else_ } => {
                check_ref(*cond, current_idx, pool_len)?;
                check_ref(*then_, current_idx, pool_len)?;
                check_ref(*else_, current_idx, pool_len)?;
                false
            }
            Node::CDeterminate => {
                // No operands, no edges. Pure structural marker; nothing to validate.
                true
            }
            Node::CBlock => false,
            Node::CEnd { value } => {
                check_ref(*value, current_idx, pool_len)?;
                false
            }
        };

        if shareable {
            let mut key = Vec::new();
            push_node_bytes(&mut key, &node);
            if let Some(&idx) = self.node_index.get(&key) {
                return Ok(NodeRef::Node(idx));
            }
            self.node_index.insert(key.clone(), current_idx);
            self.node_index_log.push((current_idx, key));
        }

        // SHARING IS LIMITED TO OPEN BLOCKS (Core IR 0.6, verifier rule 5): a node may be reused only from
        // the block being written or one enclosing it. A node made inside a block runs only when that block
        // runs, so reusing it after the block -- from the sibling arm, or after the `if` -- would name a
        // value that may never have been computed: `nested_if_effect_hoist`, `bool_and(f, h)` made in one
        // branch and reused at top level. When a block closes, the nodes made inside it leave the index.
        match &node {
            Node::CBlock => self.open_blocks.push(current_idx),
            Node::CEnd { .. } => {
                // an unbalanced CEnd is left for `verify_blocks` to refuse, naming it
                if let Some(start) = self.open_blocks.pop() {
                    while let Some((idx, _)) = self.node_index_log.last() {
                        if *idx < start {
                            break;
                        }
                        let (_, key) = self.node_index_log.pop().expect("a last entry");
                        self.node_index.remove(&key);
                    }
                }
            }
            _ => {}
        }

        self.nodes.push(node);
        Ok(NodeRef::Node(current_idx))
    }

    /// The callee identity a fn-ref pool entry carries, or `None` when the
    /// entry at `idx` is any other constant.
    ///
    /// Mirrors `emit_05::intern_fn_ref`: def_hash is the `Fn` primitive's type
    /// identity and the payload is the 32 raw bytes of the target's identity.
    fn fn_ref_target(&self, idx: u32) -> Option<Hash256> {
        let entry = self.constant_pool.get(idx as usize)?;
        if entry.def_hash != crate::lowering::emit_05::fn_type_id() {
            return None;
        }
        let bytes: [u8; 32] = entry.payload.as_slice().try_into().ok()?;
        Some(Hash256::from_bytes(bytes))
    }

    /// Number of nodes pushed so far (also the index the next `push_node` will
    /// return). Used by lowering to bracket the nodes created for one `if` arm.
    pub fn nodes_len(&self) -> u32 {
        self.nodes.len() as u32
    }

    /// Borrow a previously-pushed node by index, for reachability analysis in
    /// lowering (e.g. deciding whether an `if`-arm binding is a discarded effect).
    pub fn node_at(&self, i: u32) -> &Node {
        &self.nodes[i as usize]
    }

    /// Finalize with the bundle's semantic value. `result` must be a ref
    /// already valid against this builder's own tables (in practice always
    /// the `NodeRef` most recently returned by `push_node`/`intern_constant`
    /// on this same builder) — checked here for the same reason `push_node`
    /// checks every edge.
    pub fn finish(self, result: NodeRef) -> Result<CoreBundle, BuilderError> {
        check_ref(result, self.nodes.len() as u32, self.constant_pool.len() as u32)?;
        let bundle = CoreBundle {
            version: VERSION.to_string(),
            constant_pool: self.constant_pool,
            nodes: self.nodes,
            result,
        };
        // the compiler is held to the same rules as any bundle it will be given to load
        verify_blocks(&bundle).map_err(BuilderError::Block)?;
        Ok(bundle)
    }
}

impl<'s> Default for Builder<'s> {
    fn default() -> Self {
        Self::new()
    }
}

fn format_noderef_display(nr: NodeRef) -> String {
    match nr {
        NodeRef::Node(i) => format!("node[{}]", i),
        NodeRef::Pool(i) => format!("pool[{}]", i),
    }
}

/// Human-readable rendering of a `CoreBundle`. `scope` is accepted for future
/// use (type annotation) but not required.
pub fn bundle_display(bundle: &CoreBundle, _scope: Option<&Scope>) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(
        out,
        "Bundle (pool: {}, nodes: {}):",
        bundle.constant_pool.len(),
        bundle.nodes.len()
    )
    .unwrap();
    for (i, entry) in bundle.constant_pool.iter().enumerate() {
        writeln!(out, "  pool[{}] = {} payload=[{}b]", i, entry.def_hash, entry.payload.len())
            .unwrap();
    }
    // 0.6: the nodes inside a block are indented one step per level of nesting
    let mut depth = 0usize;
    for (i, node) in bundle.nodes.iter().enumerate() {
        if matches!(node, Node::CEnd { .. }) {
            depth = depth.saturating_sub(1);
        }
        let indent = "  ".repeat(depth);
        if matches!(node, Node::CBlock) {
            depth += 1;
        }
        let desc = match node {
            Node::CCall { target_name, args, .. } => {
                let arg_strs: Vec<String> =
                    args.iter().map(|r| format_noderef_display(*r)).collect();
                format!("{}({})", target_name, arg_strs.join(", "))
            }
            Node::CIf { cond, then_, else_ } => format!(
                "if {} then {} else {}",
                format_noderef_display(*cond),
                format_noderef_display(*then_),
                format_noderef_display(*else_)
            ),
            Node::CDeterminate => "determinate".to_string(),
            Node::CBlock => "CBlock".to_string(),
            Node::CEnd { value } => format!("CEnd value={}", format_noderef_display(*value)),
        };
        writeln!(out, "  {:<10}{}{}", format!("node[{}]", i), indent, desc).unwrap();
    }
    out
}

impl std::fmt::Display for CoreBundle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", bundle_display(self, None))
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// BLOCK VERIFIER (Core IR 0.6)
// ═══════════════════════════════════════════════════════════════════════════
//
// A scope stack (axis-core-ir-0.6.md, "Verifier"), run at load and by `Builder::finish`. Source order is
// the compiler's obligation; the bundle can show only that every edge points backwards (every decoder
// checks that). Each refusal names the node.

/// Which arm of a `CIf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arm {
    Then,
    Else,
}

impl Arm {
    fn name(self) -> &'static str {
        match self {
            Arm::Then => "then",
            Arm::Else => "else",
        }
    }
}

/// A broken 0.6 block rule. `Display` gives the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockError {
    /// Rule 1: a `CEnd` with no open block.
    EndWithoutBlock { node: u32 },
    /// Rule 1: the block opened at `block` is never closed.
    Unclosed { block: u32 },
    /// Rule 2: if node `cif`'s arms are not the blocks directly before it.
    ArmNotBefore { cif: u32 },
    /// Rule 3: `end` closes an arm of if node `cif`, and `by` references it too.
    ArmEndShared { end: u32, cif: u32, by: String },
    /// Rule 4: `user` uses `used`, inside the `arm` arm of if node `cif`.
    UseInsideArm { user: String, used: u32, cif: u32, arm: Arm },
    /// Rule 5: `user` uses `used`, inside the closed block opened at `block`.
    OutOfScope { user: String, used: u32, block: u32 },
    /// Rule 5: `user` references the `CBlock` at `used`.
    BlockReferenced { user: String, used: u32 },
}

impl std::fmt::Display for BlockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BlockError::EndWithoutBlock { node } => {
                write!(f, "block rule 1: node {} is a CEnd with no open block", node)
            }
            BlockError::Unclosed { block } => {
                write!(f, "block rule 1: the block opened at node {} is never closed", block)
            }
            BlockError::ArmNotBefore { cif } => write!(
                f,
                "block rule 2: if node {}: its arms must be the blocks directly before it (else = the CEnd at \
                 node {}, then = the CEnd just before that block; or one-armed: then = the CEnd at node {}, \
                 else = pool Unit)",
                cif,
                cif.wrapping_sub(1),
                cif.wrapping_sub(1)
            ),
            BlockError::ArmEndShared { end, cif, by } => write!(
                f,
                "block rule 3: node {} closes an arm of if node {}, and {} references it too",
                end, cif, by
            ),
            BlockError::UseInsideArm { user, used, cif, arm } => write!(
                f,
                "block rule 4: {} uses node {}, which is inside the {} arm of if node {}",
                user,
                used,
                arm.name(),
                cif
            ),
            BlockError::OutOfScope { user, used, block } => write!(
                f,
                "block rule 5: {} uses node {}, which is inside the closed block opened at node {}",
                user, used, block
            ),
            BlockError::BlockReferenced { user, used } => {
                write!(f, "block rule 5: {} references node {}, a CBlock", user, used)
            }
        }
    }
}

impl std::error::Error for BlockError {}

/// The block structure of a bundle that has passed rules 1-3: each node's block, and each block's end.
#[derive(Debug, Clone)]
pub struct Blocks {
    /// The `CBlock` of the block each node sits in (`None`: top level). A block's own `CBlock` and `CEnd`
    /// sit in the enclosing block: the `CEnd`, as a value, is read there.
    pub home: Vec<Option<u32>>,
    /// For a `CBlock`, its `CEnd`; for a `CEnd`, its `CBlock`.
    pub partner: Vec<Option<u32>>,
    /// For an arm's `CEnd`: the `CIf` that selects it, and which arm.
    pub arm: Vec<Option<(u32, Arm)>>,
}

impl Blocks {
    /// The arm a `CBlock` opens, if it is one.
    pub fn arm_of_block(&self, block: u32) -> Option<(u32, Arm)> {
        self.partner[block as usize].and_then(|end| self.arm[end as usize])
    }
}

fn is_unit_pool(bundle: &CoreBundle, r: NodeRef) -> bool {
    match r {
        NodeRef::Pool(p) => bundle
            .constant_pool
            .get(p as usize)
            .map_or(false, |e| e.def_hash == crate::lowering::emit_05::unit_type_id()),
        NodeRef::Node(_) => false,
    }
}

/// Rules 1-3: the blocks balance and nest, each `CIf`'s arms are the blocks directly before it, and an
/// arm's `CEnd` belongs to exactly one `CIf`.
pub fn block_structure(bundle: &CoreBundle) -> Result<Blocks, BlockError> {
    let n = bundle.nodes.len();
    let mut home = vec![None; n];
    let mut partner = vec![None; n];
    let mut arm: Vec<Option<(u32, Arm)>> = vec![None; n];
    let mut stack: Vec<u32> = Vec::new();
    for (i, node) in bundle.nodes.iter().enumerate() {
        let i32_ = i as u32;
        match node {
            Node::CBlock => {
                home[i] = stack.last().copied();
                stack.push(i32_);
            }
            Node::CEnd { .. } => {
                let b = stack.pop().ok_or(BlockError::EndWithoutBlock { node: i32_ })?;
                partner[b as usize] = Some(i32_);
                partner[i] = Some(b);
                home[i] = stack.last().copied();
            }
            Node::CIf { then_, else_, .. } => {
                home[i] = stack.last().copied();
                // Rule 3 first: an arm already claimed by an earlier if
                for (r, which) in [(then_, Arm::Then), (else_, Arm::Else)] {
                    if let NodeRef::Node(j) = *r {
                        if let (Node::CEnd { .. }, Some((k0, _))) = (&bundle.nodes[j as usize], arm[j as usize]) {
                            return Err(BlockError::ArmEndShared { end: j, cif: k0, by: format!("if node {}", i) });
                        }
                        if matches!(bundle.nodes[j as usize], Node::CEnd { .. }) {
                            arm[j as usize] = Some((i32_, which));
                        }
                    }
                }
                // Rule 2: the arms are the blocks directly before the if
                let end_at = |j: u32| -> bool { matches!(bundle.nodes.get(j as usize), Some(Node::CEnd { .. })) };
                let ok = match (*then_, *else_) {
                    (NodeRef::Node(t), NodeRef::Node(e)) if i32_ >= 1 && e == i32_ - 1 && end_at(e) => {
                        let b = partner[e as usize].expect("a closed block");
                        b >= 1 && t == b - 1 && end_at(t)
                    }
                    (NodeRef::Node(t), e) if i32_ >= 1 && t == i32_ - 1 && end_at(t) => is_unit_pool(bundle, e),
                    _ => false,
                };
                if !ok {
                    return Err(BlockError::ArmNotBefore { cif: i32_ });
                }
            }
            _ => home[i] = stack.last().copied(),
        }
    }
    if let Some(&b) = stack.first() {
        return Err(BlockError::Unclosed { block: b });
    }
    Ok(Blocks { home, partner, arm })
}

/// The Core IR 0.6 block rules 1-5 (axis-core-ir-0.6.md, "Verifier").
pub fn verify_blocks(bundle: &CoreBundle) -> Result<Blocks, BlockError> {
    let blocks = block_structure(bundle)?;
    let n = bundle.nodes.len();
    let mut open = vec![false; n];
    // Rules 3-5 for one edge: `user` (named by `who`) reads `used` with the blocks in `open` open.
    let check = |open: &[bool], user: Option<u32>, who: &dyn Fn() -> String, used: NodeRef| -> Result<(), BlockError> {
        let NodeRef::Node(j) = used else { return Ok(()) };
        match &bundle.nodes[j as usize] {
            Node::CBlock => return Err(BlockError::BlockReferenced { user: who(), used: j }),
            Node::CEnd { .. } => {
                if let Some((k, _)) = blocks.arm[j as usize] {
                    // only its own if reads an arm's CEnd, and it does so through then/else (not checked here)
                    let _ = user;
                    return Err(BlockError::ArmEndShared { end: j, cif: k, by: who() });
                }
            }
            _ => {}
        }
        let mut at = blocks.home[j as usize];
        if at.map_or(true, |b| open[b as usize]) {
            return Ok(());
        }
        // the closed blocks around `used`, innermost first; the outermost arm among them names rule 4
        let mut outermost = at.expect("not top level");
        let mut arm: Option<(u32, Arm)> = None;
        while let Some(b) = at {
            if open[b as usize] {
                break;
            }
            outermost = b;
            if let Some(a) = blocks.arm_of_block(b) {
                arm = Some(a);
            }
            at = blocks.home[b as usize];
        }
        Err(match arm {
            Some((cif, arm)) => BlockError::UseInsideArm { user: who(), used: j, cif, arm },
            None => BlockError::OutOfScope { user: who(), used: j, block: outermost },
        })
    };
    for (u, node) in bundle.nodes.iter().enumerate() {
        let who = || format!("node {}", u);
        let me = Some(u as u32);
        match node {
            Node::CBlock => open[u] = true,
            Node::CEnd { value } => {
                // read inside the block it closes
                check(&open, me, &who, *value)?;
                let b = blocks.partner[u].expect("a matched CEnd");
                open[b as usize] = false;
            }
            Node::CCall { args, .. } => {
                for a in args {
                    check(&open, me, &who, *a)?;
                }
            }
            Node::CIf { cond, .. } => check(&open, me, &who, *cond)?,
            Node::CDeterminate => {}
        }
    }
    check(&open, None, &|| "the result".to_string(), bundle.result)?;
    Ok(blocks)
}

fn check_ref(nr: NodeRef, current_idx: u32, pool_len: u32) -> Result<(), BuilderError> {
    match nr {
        NodeRef::Node(i) => {
            if i >= current_idx {
                return Err(BuilderError::ForwardEdge { target: i, current_len: current_idx });
            }
        }
        NodeRef::Pool(i) => {
            if i >= pool_len {
                return Err(BuilderError::PoolIndexOutOfRange { index: i, pool_len });
            }
        }
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fabric::hash::content_hash;
    use crate::registry::core05::{
        Disposition, EffectClass, FnGenesis, FnKind, LeafFacts, Record, Scope,
    };

    fn make_scope_with(identity: Hash256) -> Scope {
        make_scope_with_facts(identity, EffectClass::Pure, true)
    }

    fn make_scope_with_facts(
        identity: Hash256,
        effect_class: EffectClass,
        deterministic: bool,
    ) -> Scope {
        let mut scope = Scope::new();
        scope
            .insert(Record::FnGenesis(FnGenesis {
                identity,
                inputs: vec![],
                output: Hash256::ZERO,
                kind: FnKind::Leaf,
                disposition: Disposition::Unspecified,
            variadic: false,
            }))
            .unwrap();
        scope
            .insert(Record::LeafFacts(LeafFacts {
                identity,
                effect_class,
                deterministic,
                idempotent: true,
            }))
            .unwrap();
        scope
    }

    // ── constant pool ────────────────────────────────────────────────────

    #[test]
    fn pool_deduplication() {
        let mut b = Builder::new();
        let def = content_hash(b"bool-type");
        let r1 = b.intern_constant(def, vec![0x01]);
        let r2 = b.intern_constant(def, vec![0x01]); // same → same index
        let r3 = b.intern_constant(def, vec![0x00]); // different payload → new entry
        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
        assert_eq!(b.finish(r1).unwrap().constant_pool.len(), 2);
    }

    // ── structural sharing (hash-cons) ────────────────────────────────────

    /// `if cond { then_ } else { else_ }` with both arms as blocks (Core IR 0.6).
    fn push_if(b: &mut Builder<'_>, cond: NodeRef, then_: NodeRef, else_: NodeRef) -> NodeRef {
        b.push_node(Node::CBlock).unwrap();
        let t = b.push_node(Node::CEnd { value: then_ }).unwrap();
        b.push_node(Node::CBlock).unwrap();
        let e = b.push_node(Node::CEnd { value: else_ }).unwrap();
        b.push_node(Node::CIf { cond, then_: t, else_: e }).unwrap()
    }

    #[test]
    fn an_if_is_never_shared() {
        // 0.6: a CIf is a place (its arms are the blocks just before it), so the same cond/then/else
        // pushed twice is two nodes
        let mut b = Builder::new();
        let def = content_hash(b"bool-type");
        let cond = b.intern_constant(def, vec![0x01]);
        let then_ = b.intern_constant(content_hash(b"int-type"), vec![0x02]);
        let else_ = b.intern_constant(content_hash(b"int-type"), vec![0x00]);
        let n1 = b.push_node(Node::CIf { cond, then_, else_ }).unwrap();
        let n2 = b.push_node(Node::CIf { cond, then_, else_ }).unwrap();
        assert_ne!(n1, n2);
    }

    // ── Core IR 0.6: blocks ───────────────────────────────────────────────

    fn pure_scope() -> (Scope, Hash256) {
        let id = content_hash(b"pure_fn");
        (make_scope_with_facts(id, EffectClass::Pure, true), id)
    }

    fn unit_pool(b: &mut Builder<'_>) -> NodeRef {
        b.intern_constant(crate::lowering::emit_05::unit_type_id(), vec![])
    }

    /// `if c { pure(41) } else { pure(41) }`, then `pure(41)`, as lowering writes it.
    fn sibling_shape(b: &mut Builder<'_>, id: Hash256) -> NodeRef {
        let cond = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        let arg = b.intern_constant(content_hash(b"int-type"), vec![0x29]);
        b.push_node(Node::CBlock).unwrap();
        let t = b.push_node(ccall(id, vec![arg])).unwrap();
        let te = b.push_node(Node::CEnd { value: t }).unwrap();
        b.push_node(Node::CBlock).unwrap();
        let e = b.push_node(ccall(id, vec![arg])).unwrap();
        let ee = b.push_node(Node::CEnd { value: e }).unwrap();
        let k = b.push_node(Node::CIf { cond, then_: te, else_: ee }).unwrap();
        let after = b.push_node(ccall(id, vec![arg])).unwrap();
        b.push_node(ccall(id, vec![k, after])).unwrap()
    }

    #[test]
    fn sharing_never_reaches_into_a_closed_block() {
        let (scope, id) = pure_scope();
        let mut b = Builder::new().with_scope(&scope);
        let r = sibling_shape(&mut b, id);
        let bundle = b.finish(r).unwrap();
        // pure(41) three times: in the then-block, in the else-block, after the if
        let calls = bundle.nodes.iter().filter(|n| matches!(n, Node::CCall { args, .. } if args.len() == 1)).count();
        assert_eq!(calls, 3, "{bundle}");
    }

    #[test]
    fn sharing_from_an_enclosing_block_is_kept() {
        let (scope, id) = pure_scope();
        let mut b = Builder::new().with_scope(&scope);
        let cond = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        let arg = b.intern_constant(content_hash(b"int-type"), vec![0x29]);
        let unit = unit_pool(&mut b);
        let before = b.push_node(ccall(id, vec![arg])).unwrap();
        b.push_node(Node::CBlock).unwrap();
        let t = b.push_node(ccall(id, vec![arg])).unwrap();
        assert_eq!(t, before, "a node written before the if is reused inside its arm");
        let te = b.push_node(Node::CEnd { value: t }).unwrap();
        let k = b.push_node(Node::CIf { cond, then_: te, else_: unit }).unwrap();
        let bundle = b.finish(k).unwrap();
        assert_eq!(bundle.nodes.len(), 4, "{bundle}");
    }

    fn int_pool(v: u8) -> ConstantPoolEntry {
        ConstantPoolEntry { def_hash: content_hash(b"int-type"), payload: vec![v] }
    }

    fn call(args: Vec<NodeRef>) -> Node {
        ccall(content_hash(b"f"), args)
    }

    fn end(value: NodeRef) -> Node {
        Node::CEnd { value }
    }

    fn cif(cond: NodeRef, then_: NodeRef, else_: NodeRef) -> Node {
        Node::CIf { cond, then_, else_ }
    }

    /// pool 0 = Int 1, pool 1 = Unit
    fn bundle_of(nodes: Vec<Node>, result: NodeRef) -> CoreBundle {
        let unit = ConstantPoolEntry { def_hash: crate::lowering::emit_05::unit_type_id(), payload: vec![] };
        CoreBundle { version: VERSION.to_string(), constant_pool: vec![int_pool(1), unit], nodes, result }
    }

    const N: fn(u32) -> NodeRef = NodeRef::Node;
    const P: fn(u32) -> NodeRef = NodeRef::Pool;

    /// 0 c; 1 [ 2 w 3 ]; 4 [ 5 p 6 ]; 7 if 0 then 3 else 6
    fn two_arms() -> Vec<Node> {
        vec![call(vec![P(0)]), Node::CBlock, call(vec![P(0)]), end(N(2)), Node::CBlock, call(vec![P(0)]), end(N(5)),
             cif(N(0), N(3), N(6))]
    }

    #[test]
    fn verifier_accepts_arms_nesting_one_armed_and_plain_blocks() {
        verify_blocks(&bundle_of(two_arms(), N(7))).unwrap();
        // one-armed: 0 c; 1 [ 2 w 3 ]; 4 if 0 then 3 else Unit
        verify_blocks(&bundle_of(vec![call(vec![P(0)]), Node::CBlock, call(vec![P(0)]), end(P(1)), cif(N(0), N(3), P(1))], N(4))).unwrap();
        // plain grouping: 0 [ 1 a 2 b(1) 3 ]; 4 h(3) -- uses the closed child's CEnd
        verify_blocks(&bundle_of(vec![Node::CBlock, call(vec![P(0)]), call(vec![N(1)]), end(N(2)), call(vec![N(3)])], N(4))).unwrap();
        // nested: the two-arm if inside the then-arm of another
        let mut nodes = vec![call(vec![P(0)]), Node::CBlock];
        nodes.extend(two_arms().into_iter().skip(1).map(|n| match n {
            Node::CEnd { value: NodeRef::Node(j) } => end(N(j + 1)),
            Node::CIf { cond, then_: NodeRef::Node(t), else_: NodeRef::Node(e) } => cif(cond, N(t + 1), N(e + 1)),
            Node::CCall { args, .. } => call(args),
            other => other,
        }));
        // 0 c; 1 [ 2 [ 3 w 4 ] 5 [ 6 p 7 ] 8 if 0 then 4 else 7; 9 end(8) ]; 10 if 0 then 9 else Unit
        nodes.push(end(N(8)));
        nodes.push(cif(N(0), N(9), P(1)));
        verify_blocks(&bundle_of(nodes, N(10))).unwrap();
    }

    // ── the spec's mutants (axis-core-ir-0.6.md, "Mutants") ──────────────

    #[test]
    fn mutant_a_cend_with_no_value_is_refused() {
        // canonical: the CEnd record is cut off where its value should be
        let mut buf = Vec::new();
        write_seq_len(&mut buf, 0);
        write_seq_len(&mut buf, 2);
        write_varint(&mut buf, 3); // CBlock
        write_varint(&mut buf, 4); // CEnd, then nothing
        assert!(deserialize_canonical(&buf).is_err());
        // JSON: a CEnd object without "value"
        let mut v: serde_json::Value =
            serde_json::from_str(&encode_json(&bundle_of(vec![Node::CBlock, end(P(1))], N(1)))).unwrap();
        v["nodes"][1].as_object_mut().unwrap().remove("value");
        let json = v.to_string();
        assert!(!json.contains("\"value\""), "{json}");
        assert_eq!(decode_json(&json).unwrap_err().to_string(), "missing field: value");
    }

    #[test]
    fn mutant_an_arm_cend_referenced_by_two_ifs_is_refused() {
        let mut nodes = two_arms();
        nodes.push(cif(N(0), N(3), N(6)));
        let e = verify_blocks(&bundle_of(nodes, N(8))).unwrap_err();
        assert_eq!(e, BlockError::ArmEndShared { end: 3, cif: 7, by: "if node 8".into() });
        assert_eq!(e.to_string(), "block rule 3: node 3 closes an arm of if node 7, and if node 8 references it too");
        // and an arm's CEnd used as data
        let mut nodes = two_arms();
        nodes.push(call(vec![N(6)]));
        assert_eq!(verify_blocks(&bundle_of(nodes, N(8))).unwrap_err(),
                   BlockError::ArmEndShared { end: 6, cif: 7, by: "node 8".into() });
    }

    #[test]
    fn mutant_an_arm_block_not_directly_before_its_if_is_refused() {
        // a node between the else-block and the CIf
        let mut nodes = two_arms();
        let k = nodes.pop().unwrap();
        nodes.push(call(vec![P(0)]));
        nodes.push(k);
        let e = verify_blocks(&bundle_of(nodes, N(8))).unwrap_err();
        assert_eq!(e, BlockError::ArmNotBefore { cif: 8 });
        assert!(e.to_string().starts_with("block rule 2: if node 8"), "{e}");
        // the arms swapped
        let mut nodes = two_arms();
        nodes[7] = cif(N(0), N(6), N(3));
        assert_eq!(verify_blocks(&bundle_of(nodes, N(7))).unwrap_err(), BlockError::ArmNotBefore { cif: 7 });
        // a one-armed if whose missing else is not Unit
        let nodes = vec![call(vec![P(0)]), Node::CBlock, end(P(0)), cif(N(0), N(2), P(0))];
        assert_eq!(verify_blocks(&bundle_of(nodes, N(3))).unwrap_err(), BlockError::ArmNotBefore { cif: 3 });
    }

    #[test]
    fn mutant_a_use_of_a_node_inside_an_arm_from_outside_is_refused() {
        // after the if
        let mut nodes = two_arms();
        nodes.push(call(vec![N(2)]));
        let e = verify_blocks(&bundle_of(nodes, N(8))).unwrap_err();
        assert_eq!(e.to_string(), "block rule 4: node 8 uses node 2, which is inside the then arm of if node 7");
        // from the sibling arm
        let mut nodes = two_arms();
        nodes[5] = call(vec![N(2)]);
        assert_eq!(verify_blocks(&bundle_of(nodes, N(7))).unwrap_err(),
                   BlockError::UseInsideArm { user: "node 5".into(), used: 2, cif: 7, arm: Arm::Then });
        // the result
        let nodes = two_arms();
        assert!(matches!(verify_blocks(&bundle_of(nodes, N(5))).unwrap_err(), BlockError::UseInsideArm { .. }));
    }

    #[test]
    fn mutant_unbalanced_blocks_are_refused() {
        let e = verify_blocks(&bundle_of(vec![call(vec![P(0)]), end(N(0))], N(0))).unwrap_err();
        assert_eq!(e.to_string(), "block rule 1: node 1 is a CEnd with no open block");
        let e = verify_blocks(&bundle_of(vec![Node::CBlock, call(vec![P(0)])], P(0))).unwrap_err();
        assert_eq!(e.to_string(), "block rule 1: the block opened at node 0 is never closed");
    }

    #[test]
    fn mutant_a_share_across_sibling_blocks_is_refused() {
        // what a compiler that shares across blocks writes: the else-block reuses the then-block's node,
        // and so does the node after the if
        let nodes = vec![call(vec![P(0)]), Node::CBlock, call(vec![P(0)]), end(N(2)), Node::CBlock, end(N(2)),
                         cif(N(0), N(3), N(5))];
        let e = verify_blocks(&bundle_of(nodes, N(6))).unwrap_err();
        assert_eq!(e.to_string(), "block rule 4: node 5 uses node 2, which is inside the then arm of if node 6");
        // across two plain sibling blocks: rule 5
        let nodes = vec![Node::CBlock, call(vec![P(0)]), end(N(1)), Node::CBlock, call(vec![N(1)]), end(N(4))];
        let e = verify_blocks(&bundle_of(nodes, N(5))).unwrap_err();
        assert_eq!(e.to_string(), "block rule 5: node 4 uses node 1, which is inside the closed block opened at node 0");
    }

    #[test]
    fn a_cblock_is_never_referenced() {
        let nodes = vec![Node::CBlock, end(P(0)), call(vec![N(0)])];
        assert_eq!(verify_blocks(&bundle_of(nodes, N(2))).unwrap_err().to_string(),
                   "block rule 5: node 2 references node 0, a CBlock");
    }

    #[test]
    fn mutant_a_core_ir_05_bundle_is_refused() {
        let bundle = bundle_of(two_arms(), N(7));
        let mut bytes = write_axbi(&bundle);
        assert_eq!(bytes[5], 0x06);
        bytes[5] = 0x05;
        let e = read_axbi(&bytes).unwrap_err();
        assert!(matches!(e, AxbiError::Version05));
        assert!(e.to_string().ends_with("recompile from source"), "{e}");
        let json = encode_json(&bundle).replace("\"axis_core_ir\": \"0.6\"", "\"axis_core_ir\": \"0.5\"");
        assert!(json.contains("\"0.5\""));
        assert!(decode_json(&json).unwrap_err().to_string().ends_with("recompile from source"));
    }

    #[test]
    fn loading_and_finishing_run_the_block_verifier() {
        let mut nodes = two_arms();
        nodes.push(call(vec![N(2)]));
        let b = bundle_of(nodes, N(8));
        let e = deserialize_canonical(&serialize_canonical(&b)).unwrap_err();
        assert!(e.to_string().starts_with("block rule 4: node 8 uses node 2"), "{e}");
        assert!(decode_json(&encode_json(&b)).unwrap_err().to_string().starts_with("block rule 4"));
        let mut builder = Builder::new();
        builder.push_node(Node::CBlock).unwrap();
        let r = builder.intern_constant(content_hash(b"int"), vec![1]);
        assert!(matches!(builder.finish(r), Err(BuilderError::Block(BlockError::Unclosed { block: 0 }))));
    }

    #[test]
    fn blocks_round_trip_and_show_in_json_and_display() {
        let b = bundle_of(two_arms(), N(7));
        let back = read_axbi(&write_axbi(&b)).unwrap();
        assert_eq!(back.nodes, b.nodes);
        let json = encode_json(&b);
        assert!(json.contains("\"kind\": \"CBlock\"") && json.contains("\"kind\": \"CEnd\""), "{json}");
        assert_eq!(decode_json(&json).unwrap().nodes, b.nodes);
        let shown = bundle_display(&b, None);
        for line in ["  node[1]   CBlock", "  node[2]     f(pool[0])", "  node[3]   CEnd value=node[2]",
                     "  node[7]   if node[0] then node[3] else node[6]"] {
            assert!(shown.contains(line), "missing {line:?} in\n{shown}");
        }
        // a bundle with no block keeps its 0.5 canonical bytes: tags 0-2 only
        let flat = bundle_of(vec![call(vec![P(0)])], N(0));
        assert_eq!(serialize_canonical(&flat)[..], {
            let mut buf = Vec::new();
            write_seq_len(&mut buf, 2);
            for e in &flat.constant_pool {
                write_hash(&mut buf, &e.def_hash);
                write_bytes(&mut buf, &e.payload);
            }
            write_seq_len(&mut buf, 1);
            push_node_bytes(&mut buf, &flat.nodes[0]);
            push_noderef_bytes(&mut buf, N(0));
            buf
        }[..]);
    }

    // ── effect-aware hash-cons (the effect-blind CSE fix) ─────────────────

    fn ccall(id: Hash256, args: Vec<NodeRef>) -> Node {
        Node::CCall { target_name: "f".to_string(), target_identity: id, args }
    }

    #[test]
    fn pure_deterministic_ccall_still_hash_consed() {
        let id = content_hash(b"pure-fn");
        let scope = make_scope_with_facts(id, EffectClass::Pure, true);
        let mut b = Builder::new().with_scope(&scope);
        let arg = b.intern_constant(content_hash(b"int-type"), vec![0x29]);
        let n1 = b.push_node(ccall(id, vec![arg])).unwrap();
        let n2 = b.push_node(ccall(id, vec![arg])).unwrap();
        assert_eq!(n1, n2, "pure+deterministic duplicate call must share one node");
        assert_eq!(b.finish(n1).unwrap().nodes.len(), 1);
    }

    #[test]
    fn effectful_ccall_not_hash_consed() {
        // The battle-harness bug: two identical io_println-shaped calls must
        // BOTH execute — the second's side effect was silently dropped.
        let id = content_hash(b"fullio-fn");
        let scope = make_scope_with_facts(id, EffectClass::FullIo, false);
        let mut b = Builder::new().with_scope(&scope);
        let arg = b.intern_constant(content_hash(b"int-type"), vec![0x29]);
        let n1 = b.push_node(ccall(id, vec![arg])).unwrap();
        let n2 = b.push_node(ccall(id, vec![arg])).unwrap();
        assert_ne!(n1, n2, "identical EFFECTFUL calls must emit distinct nodes");
        assert_eq!(b.finish(n2).unwrap().nodes.len(), 2);
    }

    #[test]
    fn pure_but_nondeterministic_ccall_not_hash_consed() {
        let id = content_hash(b"nondet-fn");
        let scope = make_scope_with_facts(id, EffectClass::Pure, false);
        let mut b = Builder::new().with_scope(&scope);
        let n1 = b.push_node(ccall(id, vec![])).unwrap();
        let n2 = b.push_node(ccall(id, vec![])).unwrap();
        assert_ne!(n1, n2, "nondeterministic calls must emit distinct nodes");
        assert_eq!(b.finish(n2).unwrap().nodes.len(), 2);
    }

    #[test]
    fn scopeless_ccall_not_hash_consed() {
        // No scope => purity cannot be proven => conservative: never share.
        let mut b = Builder::new();
        let id = content_hash(b"unknown-facts-fn");
        let n1 = b.push_node(ccall(id, vec![])).unwrap();
        let n2 = b.push_node(ccall(id, vec![])).unwrap();
        assert_ne!(n1, n2, "scope-less calls must emit distinct nodes");
        assert_eq!(b.finish(n2).unwrap().nodes.len(), 2);
    }

    // ── topological invariant enforcement ────────────────────────────────

    #[test]
    fn forward_node_edge_rejected() {
        let mut b = Builder::new();
        let result = b.push_node(Node::CCall {
            target_name: "fn".to_string(),
            target_identity: content_hash(b"fn"),
            args: vec![NodeRef::Node(0)], // table empty; 0 ≥ 0
        });
        assert!(matches!(result, Err(BuilderError::ForwardEdge { .. })));
    }

    #[test]
    fn pool_index_out_of_range_rejected() {
        let mut b = Builder::new();
        let result = b.push_node(Node::CCall {
            target_name: "fn".to_string(),
            target_identity: content_hash(b"fn"),
            args: vec![NodeRef::Pool(0)], // pool empty
        });
        assert!(matches!(result, Err(BuilderError::PoolIndexOutOfRange { .. })));
    }

    // ── UNKNOWN gate ─────────────────────────────────────────────────────

    #[test]
    fn unknown_target_hard_halt() {
        let id = content_hash(b"fn");
        let scope = Scope::new(); // empty — nothing resolves
        let mut b = Builder::new().with_scope(&scope);
        let result = b.push_node(Node::CCall {
            target_name: "fn".to_string(),
            target_identity: id,
            args: vec![],
        });
        assert!(matches!(result, Err(BuilderError::UnknownTarget(_))));
    }

    #[test]
    fn known_target_accepted() {
        let id = content_hash(b"fn");
        let scope = make_scope_with(id);
        let mut b = Builder::new().with_scope(&scope);
        assert!(b.push_node(Node::CCall {
            target_name: "fn".to_string(),
            target_identity: id,
            args: vec![],
        }).is_ok());
    }

    // ── canonical serialization ───────────────────────────────────────────

    #[test]
    fn canonical_roundtrip() {
        let mut b = Builder::new();
        let cond = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        let then_ = b.intern_constant(content_hash(b"int"), vec![0x07]);
        let else_ = b.intern_constant(content_hash(b"int"), vec![0x00]);
        let r = push_if(&mut b, cond, then_, else_);
        let bundle = b.finish(r).unwrap();

        let bytes = serialize_canonical(&bundle);
        let decoded = deserialize_canonical(&bytes).unwrap();
        assert_eq!(
            serialize_canonical(&decoded),
            bytes,
            "canonical bytes must be stable across decode/re-encode"
        );
        assert_eq!(bundle.constant_pool, decoded.constant_pool);
        assert_eq!(bundle.nodes, decoded.nodes);
    }

    #[test]
    fn bundle_identity_is_stable() {
        let mk = |v: u8| {
            let mut b = Builder::new();
            let r = b.intern_constant(content_hash(b"t"), vec![v]);
            let call_ref = b.push_node(Node::CCall {
                target_name: "fn".to_string(),
                target_identity: content_hash(b"fn"),
                args: vec![r],
            })
            .unwrap();
            b.finish(call_ref).unwrap()
        };
        let b1 = mk(0x01);
        let b2 = mk(0x01);
        assert_eq!(bundle_identity(&b1), bundle_identity(&b2));
        let b3 = mk(0x02);
        assert_ne!(bundle_identity(&b1), bundle_identity(&b3));
    }

    // ── axbi round-trip ─────────────────────────────────────────────────

    #[test]
    fn axbi_roundtrip() {
        let mut b = Builder::new();
        let cond = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        let then_ = b.intern_constant(content_hash(b"int"), vec![0x0A]);
        let else_ = b.intern_constant(content_hash(b"int"), vec![0x00]);
        let r = push_if(&mut b, cond, then_, else_);
        let bundle = b.finish(r).unwrap();

        let hash_before = bundle_identity(&bundle);
        let encoded = write_axbi(&bundle);
        let decoded = read_axbi(&encoded).unwrap();
        assert_eq!(
            bundle_identity(&decoded),
            hash_before,
            "identity must survive axbi round-trip"
        );
        assert_eq!(bundle.constant_pool, decoded.constant_pool);
        assert_eq!(bundle.nodes, decoded.nodes);
    }

    // ── integration: two-node bundle ─────────────────────────────────────

    #[test]
    fn two_node_graph() {
        let fn_a = content_hash(b"fn-a");
        let fn_b = content_hash(b"fn-b");
        let mut b = Builder::new();
        let cond = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        // 0.6: each branch is written in its arm block
        b.push_node(Node::CBlock).unwrap();
        let branch_a =
            b.push_node(Node::CCall { target_name: "fn-a".to_string(), target_identity: fn_a, args: vec![] }).unwrap();
        let then_ = b.push_node(Node::CEnd { value: branch_a }).unwrap();
        b.push_node(Node::CBlock).unwrap();
        let branch_b =
            b.push_node(Node::CCall { target_name: "fn-b".to_string(), target_identity: fn_b, args: vec![] }).unwrap();
        let else_ = b.push_node(Node::CEnd { value: branch_b }).unwrap();
        let r = b.push_node(Node::CIf { cond, then_, else_ }).unwrap();
        let bundle = b.finish(r).unwrap();

        assert_eq!(bundle.constant_pool.len(), 1);
        assert_eq!(bundle.nodes.len(), 7);
        println!("{}", bundle);

        // Canonical round-trip
        let bytes = serialize_canonical(&bundle);
        let decoded = deserialize_canonical(&bytes).unwrap();
        assert_eq!(serialize_canonical(&decoded), bytes);

        // axbi round-trip: full structural and identity equality now that
        // target_name is encoded in the canonical bytes.
        let axbi_bytes = write_axbi(&bundle);
        let decoded_axbi = read_axbi(&axbi_bytes).unwrap();
        assert_eq!(decoded_axbi, bundle, "axbi round-trip must preserve the full bundle");
        assert_eq!(
            bundle_identity(&decoded_axbi),
            bundle_identity(&bundle),
            "identity must survive axbi round-trip"
        );
    }

    #[test]
    fn shared_constant_same_pool_index() {
        let mut b = Builder::new();
        let shared = b.intern_constant(content_hash(b"int"), vec![0x05]);
        let node_a =
            b.push_node(Node::CCall { target_name: "f".to_string(), target_identity: content_hash(b"f"), args: vec![shared] })
                .unwrap();
        let node_g = b.push_node(Node::CCall {
            target_name: "g".to_string(),
            target_identity: content_hash(b"g"),
            args: vec![shared, node_a],
        })
        .unwrap();
        let bundle = b.finish(node_g).unwrap();

        assert_eq!(bundle.constant_pool.len(), 1);
        // Both nodes reference pool index 0 for the shared constant
        match &bundle.nodes[0] {
            Node::CCall { args, .. } => assert_eq!(args[0], NodeRef::Pool(0)),
            _ => panic!("expected CCall at index 0"),
        }
        match &bundle.nodes[1] {
            Node::CCall { args, .. } => assert_eq!(args[0], NodeRef::Pool(0)),
            _ => panic!("expected CCall at index 1"),
        }
    }

    // ── bundle_display ────────────────────────────────────────────────────

    #[test]
    fn bundle_display_includes_fn_names() {
        let mut b = Builder::new();
        let arg = b.intern_constant(content_hash(b"bool"), vec![0x01]);
        let r = b.push_node(Node::CCall {
            target_name: "my_func".to_string(),
            target_identity: content_hash(b"my_func"),
            args: vec![arg],
        })
        .unwrap();
        let bundle = b.finish(r).unwrap();
        let text = bundle_display(&bundle, None);
        assert!(text.contains("my_func"), "display must include target_name: {}", text);
        assert!(text.contains("pool[0]"), "display must reference pool[0]: {}", text);
    }
}

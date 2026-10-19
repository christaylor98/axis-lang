"""
Axis Core IR 0.5 — Binary (.axbi) and JSON (.axbi.json) reader/writer.
No dependencies beyond the Python standard library.

Formats defined in core_ir_spec/axis-core-ir-0.5.md §Wire Formats.
"""

from __future__ import annotations

import base64
import hashlib
import json
from dataclasses import dataclass, field
from typing import List, Union


# ── Magic / version ──────────────────────────────────────────────────────────

AXBI_MAGIC   = b"AXCI"
AXBI_IR_MAJOR = 0x00
AXBI_IR_MINOR = 0x05


# ── Data model ────────────────────────────────────────────────────────────────

@dataclass(frozen=True)
class PoolRef:
    """Edge into the constant pool."""
    index: int

@dataclass(frozen=True)
class NodeRef:
    """Edge into the node table (topological index < referrer)."""
    index: int

# A NodeRef in the broad sense is either a PoolRef or a NodeRef.
Edge = Union[NodeRef, PoolRef]


@dataclass
class CCall:
    target_name:     str
    target_identity: bytes   # exactly 32 bytes (SHA-256)
    args:            List[Edge]

@dataclass
class CIf:
    cond:  Edge
    then_: Edge
    else_: Edge

@dataclass
class CDeterminate:
    pass

Node = Union[CCall, CIf, CDeterminate]


@dataclass
class PoolEntry:
    def_hash: bytes   # exactly 32 bytes
    payload:  bytes

@dataclass
class CoreBundle:
    # The bundle's semantic value — the edge a consumer must emit/return.
    # Unconstrained by kind: may reference any node or a bare pool entry
    # directly. Mandatory, no default — see core_ir_spec/axis-core-ir-0.5.md
    # §Bundle Result for why "the last node" is not a safe substitute.
    result:        Edge
    constant_pool: List[PoolEntry] = field(default_factory=list)
    nodes:         List[Node]      = field(default_factory=list)
    version:       str             = "0.5"


# ── Varint (unsigned LEB128, minimal form) ────────────────────────────────────

def _write_varint(buf: bytearray, n: int) -> None:
    assert n >= 0, "varint value must be non-negative"
    while True:
        byte = n & 0x7F
        n >>= 7
        if n == 0:
            buf.append(byte)
            return
        buf.append(byte | 0x80)

def _read_varint(data: bytes | bytearray, pos: int) -> tuple[int, int]:
    """Return (value, new_pos). Rejects non-minimal and overflowing encodings."""
    result = 0
    shift  = 0
    while True:
        if pos >= len(data):
            raise ValueError("Truncated varint")
        byte = data[pos]; pos += 1
        payload = byte & 0x7F
        if shift >= 64 or (shift == 63 and payload > 1):
            raise ValueError("Varint overflow")
        result |= payload << shift
        if (byte & 0x80) == 0:
            if byte == 0 and shift != 0:
                raise ValueError("Non-minimal varint")
            return result, pos
        shift += 7


# ── NodeRef / Edge wire encoding ──────────────────────────────────────────────

def _write_edge(buf: bytearray, edge: Edge) -> None:
    """node(i) → i<<1 ; pool(i) → (i<<1)|1"""
    if isinstance(edge, PoolRef):
        _write_varint(buf, (edge.index << 1) | 1)
    else:
        _write_varint(buf, edge.index << 1)

def _read_edge(data: bytes, pos: int, node_idx: int, pool_len: int) -> tuple[Edge, int]:
    tagged, pos = _read_varint(data, pos)
    idx = tagged >> 1
    if tagged & 1:
        if idx >= pool_len:
            raise ValueError(f"Pool index {idx} out of range (pool len {pool_len})")
        return PoolRef(idx), pos
    else:
        if idx >= node_idx:
            raise ValueError(
                f"Forward edge at node {node_idx}: references node {idx} (topological violation)"
            )
        return NodeRef(idx), pos


# ── Bytes field (varint length prefix) ───────────────────────────────────────

def _write_bytes_field(buf: bytearray, data: bytes) -> None:
    _write_varint(buf, len(data))
    buf.extend(data)

def _read_bytes_field(data: bytes, pos: int) -> tuple[bytes, int]:
    length, pos = _read_varint(data, pos)
    if pos + length > len(data):
        raise ValueError(f"Truncated bytes field: need {length} bytes at pos {pos}")
    return data[pos:pos + length], pos + length


# ── Canonical binary serialisation ────────────────────────────────────────────

def serialize_canonical(bundle: CoreBundle) -> bytes:
    """
    Produce the canonical binary form of a bundle.
    This is the ONLY input to the identity hash; the .axbi header is excluded.
    """
    buf = bytearray()

    _write_varint(buf, len(bundle.constant_pool))
    for entry in bundle.constant_pool:
        if len(entry.def_hash) != 32:
            raise ValueError(f"def_hash must be 32 bytes, got {len(entry.def_hash)}")
        buf.extend(entry.def_hash)
        _write_bytes_field(buf, entry.payload)

    _write_varint(buf, len(bundle.nodes))
    for node in bundle.nodes:
        if isinstance(node, CCall):
            _write_varint(buf, 0)
            _write_bytes_field(buf, node.target_name.encode("utf-8"))
            if len(node.target_identity) != 32:
                raise ValueError("target_identity must be 32 bytes")
            buf.extend(node.target_identity)
            _write_varint(buf, len(node.args))
            for arg in node.args:
                _write_edge(buf, arg)
        elif isinstance(node, CIf):
            _write_varint(buf, 1)
            _write_edge(buf, node.cond)
            _write_edge(buf, node.then_)
            _write_edge(buf, node.else_)
        elif isinstance(node, CDeterminate):
            _write_varint(buf, 2)
        else:
            raise TypeError(f"Unknown node type: {type(node)}")

    _write_edge(buf, bundle.result)

    return bytes(buf)


def deserialize_canonical(data: bytes) -> CoreBundle:
    """Decode canonical bytes into a CoreBundle. Validates topological invariant."""
    pos = 0

    pool_count, pos = _read_varint(data, pos)
    pool: List[PoolEntry] = []
    for _ in range(pool_count):
        if pos + 32 > len(data):
            raise ValueError("Truncated def_hash")
        def_hash = data[pos:pos + 32]; pos += 32
        payload, pos = _read_bytes_field(data, pos)
        pool.append(PoolEntry(def_hash=def_hash, payload=payload))

    node_count, pos = _read_varint(data, pos)
    pool_len = len(pool)
    nodes: List[Node] = []

    for i in range(node_count):
        kind, pos = _read_varint(data, pos)
        if kind == 0:   # CCall
            name_bytes, pos = _read_bytes_field(data, pos)
            target_name = name_bytes.decode("utf-8")
            if pos + 32 > len(data):
                raise ValueError("Truncated target_identity")
            target_identity = data[pos:pos + 32]; pos += 32
            arg_count, pos = _read_varint(data, pos)
            args: List[Edge] = []
            for _ in range(arg_count):
                edge, pos = _read_edge(data, pos, i, pool_len)
                args.append(edge)
            nodes.append(CCall(target_name=target_name,
                               target_identity=target_identity,
                               args=args))
        elif kind == 1: # CIf
            cond,  pos = _read_edge(data, pos, i, pool_len)
            then_, pos = _read_edge(data, pos, i, pool_len)
            else_, pos = _read_edge(data, pos, i, pool_len)
            nodes.append(CIf(cond=cond, then_=then_, else_=else_))
        elif kind == 2: # CDeterminate
            nodes.append(CDeterminate())
        else:
            raise ValueError(f"Unknown node kind tag {kind}")

    result, pos = _read_edge(data, pos, node_count, pool_len)

    return CoreBundle(result=result, constant_pool=pool, nodes=nodes)


# ── .axbi file format (header + canonical payload) ───────────────────────────

def write_axbi(bundle: CoreBundle) -> bytes:
    """Encode to .axbi: 6-byte header (magic + IR version) + canonical payload."""
    header = AXBI_MAGIC + bytes([AXBI_IR_MAJOR, AXBI_IR_MINOR])
    return header + serialize_canonical(bundle)

def read_axbi(data: bytes) -> CoreBundle:
    """Decode .axbi bytes. Validates magic and IR version."""
    if len(data) < 6:
        raise ValueError("File too short to be .axbi")
    if data[:4] != AXBI_MAGIC:
        raise ValueError(f"Bad magic: expected b'AXCI', got {data[:4]!r}")
    major, minor = data[4], data[5]
    if (major, minor) != (AXBI_IR_MAJOR, AXBI_IR_MINOR):
        raise ValueError(f"Unsupported IR version {major}.{minor}")
    return deserialize_canonical(data[6:])


# ── JSON format ───────────────────────────────────────────────────────────────

def _edge_to_json(edge: Edge) -> dict:
    if isinstance(edge, PoolRef):
        return {"space": "pool", "index": edge.index}
    return {"space": "node", "index": edge.index}

def _edge_from_json(obj: dict, node_idx: int, pool_len: int) -> Edge:
    space = obj.get("space")
    idx   = obj.get("index")
    if space == "pool":
        if idx >= pool_len:
            raise ValueError(f"Pool index {idx} out of range")
        return PoolRef(idx)
    elif space == "node":
        if idx >= node_idx:
            raise ValueError(
                f"Forward edge at node {node_idx}: references node {idx}"
            )
        return NodeRef(idx)
    raise ValueError(f"Unknown noderef space: {space!r}")

def bundle_to_json_obj(bundle: CoreBundle) -> dict:
    pool_out = []
    for entry in bundle.constant_pool:
        pool_out.append({
            "def_hash": entry.def_hash.hex(),
            "payload":  base64.b64encode(entry.payload).decode("ascii"),
        })

    nodes_out = []
    for node in bundle.nodes:
        if isinstance(node, CCall):
            nodes_out.append({
                "kind":            "CCall",
                "target_name":     node.target_name,
                "target_identity": node.target_identity.hex(),
                "args":            [_edge_to_json(a) for a in node.args],
            })
        elif isinstance(node, CIf):
            nodes_out.append({
                "kind": "CIf",
                "cond": _edge_to_json(node.cond),
                "then": _edge_to_json(node.then_),
                "else": _edge_to_json(node.else_),
            })
        elif isinstance(node, CDeterminate):
            nodes_out.append({"kind": "CDeterminate"})

    return {
        "axis_core_ir":  bundle.version,
        "constant_pool": pool_out,
        "nodes":         nodes_out,
        "result":        _edge_to_json(bundle.result),
    }

def bundle_from_json_obj(obj: dict) -> CoreBundle:
    pool: List[PoolEntry] = []
    for entry in obj.get("constant_pool", []):
        pool.append(PoolEntry(
            def_hash=bytes.fromhex(entry["def_hash"]),
            payload =base64.b64decode(entry["payload"]),
        ))
    pool_len = len(pool)
    nodes: List[Node] = []

    for i, node_obj in enumerate(obj.get("nodes", [])):
        kind = node_obj.get("kind")
        if kind == "CCall":
            nodes.append(CCall(
                target_name     =node_obj["target_name"],
                target_identity =bytes.fromhex(node_obj["target_identity"]),
                args            =[_edge_from_json(a, i, pool_len)
                                  for a in node_obj.get("args", [])],
            ))
        elif kind == "CIf":
            nodes.append(CIf(
                cond =_edge_from_json(node_obj["cond"],  i, pool_len),
                then_=_edge_from_json(node_obj["then"],  i, pool_len),
                else_=_edge_from_json(node_obj["else"],  i, pool_len),
            ))
        elif kind == "CDeterminate":
            nodes.append(CDeterminate())
        else:
            raise ValueError(f"Unknown node kind: {kind!r}")

    if "result" not in obj:
        raise ValueError("Missing mandatory 'result' field")
    result = _edge_from_json(obj["result"], len(nodes), pool_len)

    return CoreBundle(result=result, constant_pool=pool, nodes=nodes,
                      version=obj.get("axis_core_ir", "0.5"))

def write_json(bundle: CoreBundle, *, indent: int = 2) -> str:
    """Encode to .axbi.json string."""
    return json.dumps(bundle_to_json_obj(bundle), indent=indent)

def read_json(text: str) -> CoreBundle:
    """Decode .axbi.json string."""
    return bundle_from_json_obj(json.loads(text))


# ── Identity ──────────────────────────────────────────────────────────────────

def bundle_identity(bundle: CoreBundle) -> bytes:
    """SHA-256 of the canonical encoding. This IS the bundle's identity."""
    return hashlib.sha256(serialize_canonical(bundle)).digest()

def bundle_identity_hex(bundle: CoreBundle) -> str:
    return bundle_identity(bundle).hex()


# ── Smoke test (run directly: python axis_core_ir.py) ────────────────────────

if __name__ == "__main__":
    import sys

    # Build a tiny bundle: pool[0]=Bool true, pool[1]=Int 7, node[0]=CCall "fn_a"(pool[0])
    fake_hash_a = bytes(range(32))
    fake_hash_b = bytes(range(1, 33))
    fake_id_a   = bytes(range(2, 34))

    b = CoreBundle(
        result=NodeRef(1),   # the CIf is the bundle's meaning
        constant_pool=[
            PoolEntry(def_hash=fake_hash_a, payload=b"\x01"),      # Bool true
            PoolEntry(def_hash=fake_hash_b, payload=b"\x07"),      # Int 7
        ],
        nodes=[
            CCall(target_name="fn_a", target_identity=fake_id_a,
                  args=[PoolRef(0), PoolRef(1)]),
            CIf(cond=NodeRef(0), then_=PoolRef(0), else_=PoolRef(1)),
        ],
    )

    # Round-trip canonical binary
    raw = serialize_canonical(b)
    b2  = deserialize_canonical(raw)
    assert serialize_canonical(b2) == raw, "canonical round-trip failed"

    # Round-trip .axbi
    axbi  = write_axbi(b)
    b3    = read_axbi(axbi)
    assert serialize_canonical(b3) == raw, ".axbi round-trip failed"

    # Round-trip JSON
    js   = write_json(b)
    b4   = read_json(js)
    assert serialize_canonical(b4) == raw, "JSON round-trip failed"

    print("identity:", bundle_identity_hex(b))
    print("axbi bytes:", len(axbi))
    print("json:\n" + js)
    print("\nAll round-trips OK.")
    sys.exit(0)

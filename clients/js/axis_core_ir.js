/**
 * Axis Core IR 0.5 — Binary (.axbi) and JSON (.axbi.json) reader/writer.
 *
 * Pure ES module, zero dependencies. Works in Node.js 18+ and modern browsers.
 * Formats defined in core_ir_spec/axis-core-ir-0.5.md §Wire Formats.
 */

// ── Magic / version ──────────────────────────────────────────────────────────

export const AXBI_MAGIC    = new Uint8Array([0x41, 0x58, 0x43, 0x49]); // 'AXCI'
export const AXBI_IR_MAJOR = 0x00;
export const AXBI_IR_MINOR = 0x05;


// ── Data model ────────────────────────────────────────────────────────────────

/** Edge into the constant pool. */
export class PoolRef {
  /** @param {number} index */
  constructor(index) { this.index = index; }
}

/** Edge into the node table (topological index < referrer). */
export class NodeEdge {
  /** @param {number} index */
  constructor(index) { this.index = index; }
}

export class CCall {
  /**
   * @param {string}     targetName
   * @param {Uint8Array} targetIdentity  32-byte SHA-256
   * @param {Array<PoolRef|NodeEdge>} args
   */
  constructor(targetName, targetIdentity, args) {
    this.targetName     = targetName;
    this.targetIdentity = targetIdentity;
    this.args           = args;
  }
}

export class CIf {
  /** @param {PoolRef|NodeEdge} cond @param {PoolRef|NodeEdge} then_ @param {PoolRef|NodeEdge} else_ */
  constructor(cond, then_, else_) {
    this.cond  = cond;
    this.then_ = then_;
    this.else_ = else_;
  }
}

export class CDeterminate {}

export class PoolEntry {
  /** @param {Uint8Array} defHash 32 bytes  @param {Uint8Array} payload */
  constructor(defHash, payload) {
    this.defHash = defHash;
    this.payload = payload;
  }
}

export class CoreBundle {
  /**
   * @param {PoolRef|NodeEdge} result  The bundle's semantic value — the edge
   *   a consumer must emit/return. Unconstrained by kind: may reference any
   *   node or a bare pool entry directly. Mandatory, no default — see
   *   core_ir_spec/axis-core-ir-0.5.md §Bundle Result for why "the last
   *   node" is not a safe substitute.
   * @param {PoolEntry[]} constantPool @param {Array<CCall|CIf|CDeterminate>} nodes @param {string} version
   */
  constructor(result, constantPool = [], nodes = [], version = "0.5") {
    this.result        = result;
    this.constantPool  = constantPool;
    this.nodes         = nodes;
    this.version       = version;
  }
}


// ── Varint (unsigned LEB128, minimal form) ────────────────────────────────────

/** @param {number[]} buf @param {bigint|number} value */
function writeVarint(buf, value) {
  let v = BigInt(value);
  while (true) {
    const byte = Number(v & 0x7Fn);
    v >>= 7n;
    if (v === 0n) { buf.push(byte); return; }
    buf.push(byte | 0x80);
  }
}

/** @returns {{value: bigint, pos: number}} */
function readVarint(data, pos) {
  let result = 0n, shift = 0n;
  while (true) {
    if (pos >= data.length) throw new Error("Truncated varint");
    const byte = data[pos++];
    const payload = BigInt(byte & 0x7F);
    if (shift >= 64n || (shift === 63n && payload > 1n))
      throw new Error("Varint overflow");
    result |= payload << shift;
    if ((byte & 0x80) === 0) {
      if (byte === 0 && shift !== 0n) throw new Error("Non-minimal varint");
      return { value: result, pos };
    }
    shift += 7n;
  }
}


// ── Edge wire encoding ────────────────────────────────────────────────────────

/** @param {number[]} buf @param {PoolRef|NodeEdge} edge */
function writeEdge(buf, edge) {
  if (edge instanceof PoolRef)
    writeVarint(buf, (BigInt(edge.index) << 1n) | 1n);
  else
    writeVarint(buf, BigInt(edge.index) << 1n);
}

/** @returns {{edge: PoolRef|NodeEdge, pos: number}} */
function readEdge(data, pos, nodeIdx, poolLen) {
  const { value: tagged, pos: p } = readVarint(data, pos);
  const idx = Number(tagged >> 1n);
  if (tagged & 1n) {
    if (idx >= poolLen)
      throw new Error(`Pool index ${idx} out of range (pool len ${poolLen})`);
    return { edge: new PoolRef(idx), pos: p };
  } else {
    if (idx >= nodeIdx)
      throw new Error(`Forward edge at node ${nodeIdx}: references node ${idx}`);
    return { edge: new NodeEdge(idx), pos: p };
  }
}


// ── Bytes field (varint length prefix) ───────────────────────────────────────

/** @param {number[]} buf @param {Uint8Array} data */
function writeBytesField(buf, data) {
  writeVarint(buf, data.length);
  for (const b of data) buf.push(b);
}

/** @returns {{bytes: Uint8Array, pos: number}} */
function readBytesField(data, pos) {
  const { value: len, pos: p } = readVarint(data, pos);
  const length = Number(len);
  if (p + length > data.length)
    throw new Error(`Truncated bytes field: need ${length} bytes at pos ${p}`);
  return { bytes: data.slice(p, p + length), pos: p + length };
}


// ── Canonical binary serialisation ────────────────────────────────────────────

const _enc = new TextEncoder();
const _dec = new TextDecoder();

/**
 * Produce the canonical binary form of a bundle.
 * This is the ONLY input to the identity hash; the .axbi header is excluded.
 * @param {CoreBundle} bundle
 * @returns {Uint8Array}
 */
export function serializeCanonical(bundle) {
  const buf = [];

  writeVarint(buf, bundle.constantPool.length);
  for (const entry of bundle.constantPool) {
    if (entry.defHash.length !== 32) throw new Error("defHash must be 32 bytes");
    for (const b of entry.defHash) buf.push(b);
    writeBytesField(buf, entry.payload);
  }

  writeVarint(buf, bundle.nodes.length);
  for (const node of bundle.nodes) {
    if (node instanceof CCall) {
      writeVarint(buf, 0);
      writeBytesField(buf, _enc.encode(node.targetName));
      if (node.targetIdentity.length !== 32) throw new Error("targetIdentity must be 32 bytes");
      for (const b of node.targetIdentity) buf.push(b);
      writeVarint(buf, node.args.length);
      for (const arg of node.args) writeEdge(buf, arg);
    } else if (node instanceof CIf) {
      writeVarint(buf, 1);
      writeEdge(buf, node.cond);
      writeEdge(buf, node.then_);
      writeEdge(buf, node.else_);
    } else if (node instanceof CDeterminate) {
      writeVarint(buf, 2);
    } else {
      throw new Error(`Unknown node type: ${node?.constructor?.name}`);
    }
  }

  writeEdge(buf, bundle.result);

  return new Uint8Array(buf);
}

/**
 * Decode canonical bytes into a CoreBundle. Validates topological invariant.
 * @param {Uint8Array} data
 * @returns {CoreBundle}
 */
export function deserializeCanonical(data) {
  let pos = 0;

  let r = readVarint(data, pos);
  const poolCount = Number(r.value); pos = r.pos;

  const pool = [];
  for (let i = 0; i < poolCount; i++) {
    if (pos + 32 > data.length) throw new Error("Truncated def_hash");
    const defHash = data.slice(pos, pos + 32); pos += 32;
    const { bytes: payload, pos: p } = readBytesField(data, pos); pos = p;
    pool.push(new PoolEntry(defHash, payload));
  }

  r = readVarint(data, pos);
  const nodeCount = Number(r.value); pos = r.pos;
  const poolLen = pool.length;
  const nodes = [];

  for (let i = 0; i < nodeCount; i++) {
    r = readVarint(data, pos); const kind = Number(r.value); pos = r.pos;
    if (kind === 0) {
      const { bytes: nameBytes, pos: p1 } = readBytesField(data, pos); pos = p1;
      const targetName = _dec.decode(nameBytes);
      if (pos + 32 > data.length) throw new Error("Truncated target_identity");
      const targetIdentity = data.slice(pos, pos + 32); pos += 32;
      r = readVarint(data, pos); const argCount = Number(r.value); pos = r.pos;
      const args = [];
      for (let j = 0; j < argCount; j++) {
        const { edge, pos: p } = readEdge(data, pos, i, poolLen); pos = p;
        args.push(edge);
      }
      nodes.push(new CCall(targetName, targetIdentity, args));
    } else if (kind === 1) {
      const { edge: cond,  pos: p1 } = readEdge(data, pos, i, poolLen); pos = p1;
      const { edge: then_, pos: p2 } = readEdge(data, pos, i, poolLen); pos = p2;
      const { edge: else_, pos: p3 } = readEdge(data, pos, i, poolLen); pos = p3;
      nodes.push(new CIf(cond, then_, else_));
    } else if (kind === 2) {
      nodes.push(new CDeterminate());
    } else {
      throw new Error(`Unknown node kind tag ${kind}`);
    }
  }

  const { edge: result } = readEdge(data, pos, nodeCount, poolLen);

  return new CoreBundle(result, pool, nodes);
}


// ── .axbi file format ─────────────────────────────────────────────────────────

/**
 * Encode to .axbi: 6-byte header (magic + IR version) + canonical payload.
 * @param {CoreBundle} bundle
 * @returns {Uint8Array}
 */
export function writeAxbi(bundle) {
  const payload = serializeCanonical(bundle);
  const out = new Uint8Array(6 + payload.length);
  out.set(AXBI_MAGIC, 0);
  out[4] = AXBI_IR_MAJOR;
  out[5] = AXBI_IR_MINOR;
  out.set(payload, 6);
  return out;
}

/**
 * Decode .axbi bytes. Validates magic and IR version.
 * @param {Uint8Array} data
 * @returns {CoreBundle}
 */
export function readAxbi(data) {
  if (data.length < 6) throw new Error("File too short to be .axbi");
  for (let i = 0; i < 4; i++) {
    if (data[i] !== AXBI_MAGIC[i])
      throw new Error(`Bad magic: expected AXCI, got ${String.fromCharCode(...data.slice(0,4))}`);
  }
  if (data[4] !== AXBI_IR_MAJOR || data[5] !== AXBI_IR_MINOR)
    throw new Error(`Unsupported IR version ${data[4]}.${data[5]}`);
  return deserializeCanonical(data.slice(6));
}


// ── JSON format ───────────────────────────────────────────────────────────────

function toHex(bytes) {
  return Array.from(bytes, b => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex) {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++)
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

// Base64 RFC 4648 §4 with padding — works in Node 18+ and all modern browsers.
function toBase64(bytes) {
  if (typeof Buffer !== "undefined")
    return Buffer.from(bytes).toString("base64");
  // Browser: btoa expects a binary string
  return btoa(String.fromCharCode(...bytes));
}

function fromBase64(str) {
  if (typeof Buffer !== "undefined")
    return new Uint8Array(Buffer.from(str, "base64"));
  return new Uint8Array(atob(str).split("").map(c => c.charCodeAt(0)));
}

function edgeToJson(edge) {
  if (edge instanceof PoolRef) return { space: "pool", index: edge.index };
  return { space: "node", index: edge.index };
}

function edgeFromJson(obj, nodeIdx, poolLen) {
  const { space, index } = obj;
  if (space === "pool") {
    if (index >= poolLen) throw new Error(`Pool index ${index} out of range`);
    return new PoolRef(index);
  } else if (space === "node") {
    if (index >= nodeIdx)
      throw new Error(`Forward edge at node ${nodeIdx}: references node ${index}`);
    return new NodeEdge(index);
  }
  throw new Error(`Unknown noderef space: ${space}`);
}

/**
 * Encode bundle as JSON-serialisable object.
 * @param {CoreBundle} bundle
 * @returns {object}
 */
export function bundleToJsonObj(bundle) {
  return {
    axis_core_ir: bundle.version,
    constant_pool: bundle.constantPool.map(e => ({
      def_hash: toHex(e.defHash),
      payload:  toBase64(e.payload),
    })),
    nodes: bundle.nodes.map(node => {
      if (node instanceof CCall) return {
        kind:            "CCall",
        target_name:     node.targetName,
        target_identity: toHex(node.targetIdentity),
        args:            node.args.map(edgeToJson),
      };
      if (node instanceof CIf) return {
        kind: "CIf",
        cond: edgeToJson(node.cond),
        then: edgeToJson(node.then_),
        else: edgeToJson(node.else_),
      };
      if (node instanceof CDeterminate) return { kind: "CDeterminate" };
      throw new Error("Unknown node type");
    }),
    result: edgeToJson(bundle.result),
  };
}

/**
 * Decode JSON-parsed object into a CoreBundle.
 * @param {object} obj
 * @returns {CoreBundle}
 */
export function bundleFromJsonObj(obj) {
  const pool = (obj.constant_pool || []).map(e =>
    new PoolEntry(fromHex(e.def_hash), fromBase64(e.payload))
  );
  const poolLen = pool.length;

  const nodes = (obj.nodes || []).map((nodeObj, i) => {
    if (nodeObj.kind === "CCall") return new CCall(
      nodeObj.target_name,
      fromHex(nodeObj.target_identity),
      (nodeObj.args || []).map(a => edgeFromJson(a, i, poolLen)),
    );
    if (nodeObj.kind === "CIf") return new CIf(
      edgeFromJson(nodeObj.cond, i, poolLen),
      edgeFromJson(nodeObj.then, i, poolLen),
      edgeFromJson(nodeObj.else, i, poolLen),
    );
    if (nodeObj.kind === "CDeterminate") return new CDeterminate();
    throw new Error(`Unknown node kind: ${nodeObj.kind}`);
  });

  if (obj.result === undefined) throw new Error("Missing mandatory 'result' field");
  const result = edgeFromJson(obj.result, nodes.length, poolLen);

  return new CoreBundle(result, pool, nodes, obj.axis_core_ir || "0.5");
}

/**
 * Encode bundle as a JSON string.
 * @param {CoreBundle} bundle
 * @param {number} [indent=2]
 * @returns {string}
 */
export function writeJson(bundle, indent = 2) {
  return JSON.stringify(bundleToJsonObj(bundle), null, indent);
}

/**
 * Decode a JSON string into a CoreBundle.
 * @param {string} text
 * @returns {CoreBundle}
 */
export function readJson(text) {
  return bundleFromJsonObj(JSON.parse(text));
}


// ── Identity ──────────────────────────────────────────────────────────────────

/**
 * Compute the bundle identity: SHA-256 of serialize_canonical output.
 * Returns a Promise<Uint8Array> (Web Crypto is async).
 * @param {CoreBundle} bundle
 * @returns {Promise<Uint8Array>}
 */
export async function bundleIdentity(bundle) {
  const canonical = serializeCanonical(bundle);
  const hashBuf   = await crypto.subtle.digest("SHA-256", canonical);
  return new Uint8Array(hashBuf);
}

/**
 * Identity using Node.js crypto (Node only).
 *
 * Pre-existing bug fixed incidentally while verifying BUG2_RESULT_FIELD_V1:
 * this used `await` inside a non-`async` function, a hard syntax error that
 * broke parsing of the entire module (not just this function). Dynamic
 * `import()` is unavoidably async, so despite the name this can no longer
 * promise synchronous return — it now correctly returns a Promise, same as
 * `bundleIdentity` above. Kept as a separate export (rather than removed)
 * since `node:crypto`'s `createHash` is the more common idiom in Node-only
 * code than Web Crypto's `subtle.digest`.
 * @param {CoreBundle} bundle
 * @returns {Promise<Uint8Array>}
 */
export async function bundleIdentitySync(bundle) {
  const { createHash } = await import("node:crypto").catch(() => { throw new Error("bundleIdentitySync is Node-only"); });
  return new Uint8Array(createHash("sha256").update(serializeCanonical(bundle)).digest());
}

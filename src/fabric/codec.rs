//! Canonical encoding primitives (Registry Format §1) and value payload
//! encoders (Registry Format §10) — the one serialization regime shared by
//! registry records, constant-pool entries, and the canonical bundle form.
//!
//! Every encoder here is **deterministic**: a given value has exactly one byte
//! sequence. This is mandatory because these bytes are hashed (identity = hash
//! of canonical bytes). Decoders enforce canonical form and reject anything that
//! is not minimal — a non-canonical encoding is a hard error, never tolerated.
//!
//! ## Phase 0 decisions locked here
//!
//! * **Signed integers use zig-zag then varint.** Registry Format §1 specifies
//!   unsigned LEB128; values that may be negative (the `int` primitive payload)
//!   are mapped to unsigned via zig-zag first, so the wire is always a minimal
//!   unsigned varint. See [`encode_int`].
//!
//! * **Floats use fixed 8-byte IEEE-754, big-endian.** No varint (floats have no
//!   minimal short form). NaN bit-patterns are written verbatim; surfaces are
//!   responsible for not minting non-canonical NaNs. See [`encode_float`].
//!
//! * **`NodeRef` binary tag = a single varint, low bit selects the space.**
//!   `node(i)` → `i << 1`; `pool(i)` → `(i << 1) | 1`. The two index spaces are
//!   disjoint and the tag is self-describing, so a Core IR edge is one varint in
//!   the canonical (non-capnp) form. The in-memory `NodeRef` type lands in
//!   Phase 2; the wire contract is fixed now. See [`encode_noderef`].
//!
//! * **Canonical bytes are defined by THIS codec, not by capnp framing.** A
//!   bundle's / record's identity is `content_hash` of the bytes produced here,
//!   independent of any capnp wire framing or schema version. capnp is transport
//!   only. (Locks migration-plan §5 "Capnp framing vs. canonical hashing".)

use crate::fabric::hash::{Hash256, HASH_LEN};

/// Error decoding canonical bytes. Any of these means the input is not a valid
/// canonical encoding and MUST be rejected (Registry Format §1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// Ran out of bytes mid-value.
    Truncated,
    /// A varint used redundant high bytes (not minimal form).
    NonMinimalVarint,
    /// A varint exceeded 64 bits.
    VarintOverflow,
    /// A bool byte was not 0x00 or 0x01.
    InvalidBool(u8),
    /// A length or count exceeded what the platform can address.
    LengthOverflow,
    /// Trailing bytes remained after a value that should have consumed all input.
    TrailingBytes(usize),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodecError::Truncated => write!(f, "unexpected end of input"),
            CodecError::NonMinimalVarint => write!(f, "non-minimal varint encoding"),
            CodecError::VarintOverflow => write!(f, "varint exceeds 64 bits"),
            CodecError::InvalidBool(b) => write!(f, "invalid bool byte 0x{:02x}", b),
            CodecError::LengthOverflow => write!(f, "length exceeds addressable range"),
            CodecError::TrailingBytes(n) => write!(f, "{} trailing byte(s) after value", n),
        }
    }
}

impl std::error::Error for CodecError {}

/// A cursor over canonical bytes that tracks position and enforces canonical
/// decoding rules.
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Current offset.
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Assert all input was consumed (use after decoding a whole artifact).
    pub fn expect_eof(&self) -> Result<(), CodecError> {
        if self.pos == self.buf.len() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes(self.buf.len() - self.pos))
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        if self.pos + n > self.buf.len() {
            return Err(CodecError::Truncated);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn take1(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
}

// ───────────────────────────── primitives (§1) ─────────────────────────────

/// Append an unsigned LEB128 varint in minimal form.
pub fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            break;
        } else {
            buf.push(byte | 0x80);
        }
    }
}

/// Decode a minimal-form unsigned LEB128 varint. Rejects redundant high bytes
/// and >64-bit values.
pub fn read_varint(r: &mut Reader<'_>) -> Result<u64, CodecError> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        let byte = r.take1()?;
        let payload = (byte & 0x7f) as u64;
        if shift >= 64 || (shift == 63 && payload > 1) {
            return Err(CodecError::VarintOverflow);
        }
        result |= payload << shift;
        if byte & 0x80 == 0 {
            // Minimal-form check: a non-final encoding must not end in a 0x00
            // continuation that contributed nothing (i.e. the last byte being
            // 0 while shift > 0 means redundant padding).
            if byte == 0 && shift != 0 {
                return Err(CodecError::NonMinimalVarint);
            }
            return Ok(result);
        }
        shift += 7;
    }
}

/// Append a bool as a single 0x00/0x01 byte.
pub fn write_bool(buf: &mut Vec<u8>, value: bool) {
    buf.push(if value { 0x01 } else { 0x00 });
}

/// Decode a bool; any byte other than 0x00/0x01 is an error.
pub fn read_bool(r: &mut Reader<'_>) -> Result<bool, CodecError> {
    match r.take1()? {
        0x00 => Ok(false),
        0x01 => Ok(true),
        other => Err(CodecError::InvalidBool(other)),
    }
}

/// Append a 32-byte hash (raw, no length prefix — width is fixed).
pub fn write_hash(buf: &mut Vec<u8>, h: &Hash256) {
    buf.extend_from_slice(h.as_bytes());
}

/// Decode a fixed 32-byte hash.
pub fn read_hash(r: &mut Reader<'_>) -> Result<Hash256, CodecError> {
    let s = r.take(HASH_LEN)?;
    let mut out = [0u8; HASH_LEN];
    out.copy_from_slice(s);
    Ok(Hash256::from_bytes(out))
}

/// Append length-prefixed bytes (varint length, then the bytes).
pub fn write_bytes(buf: &mut Vec<u8>, data: &[u8]) {
    write_varint(buf, data.len() as u64);
    buf.extend_from_slice(data);
}

/// Decode length-prefixed bytes, returning a borrowed slice.
pub fn read_bytes<'a>(r: &mut Reader<'a>) -> Result<&'a [u8], CodecError> {
    let len = read_varint(r)?;
    let len = usize::try_from(len).map_err(|_| CodecError::LengthOverflow)?;
    r.take(len)
}

/// Append a `seq<T>` count. The caller then writes exactly `count` elements.
pub fn write_seq_len(buf: &mut Vec<u8>, count: usize) {
    write_varint(buf, count as u64);
}

/// Decode a `seq<T>` count.
pub fn read_seq_len(r: &mut Reader<'_>) -> Result<usize, CodecError> {
    let n = read_varint(r)?;
    usize::try_from(n).map_err(|_| CodecError::LengthOverflow)
}

// ─────────────────────── value payloads (§10) ───────────────────────────────
//
// Payloads are schema-directed and positional: no embedded names, no type tags.
// A product payload is its fields concatenated in declared order; a sum payload
// is a constructor index (varint) followed by that constructor's fields; a
// primitive payload uses the primitive's canonical encoding below. The schema
// (a TypeDef, Phase 1) tells a decoder which of these to apply — the bytes
// themselves carry no discriminator.

/// Encode a signed integer payload: zig-zag, then minimal unsigned varint.
pub fn encode_int(buf: &mut Vec<u8>, value: i64) {
    let zigzag = ((value << 1) ^ (value >> 63)) as u64;
    write_varint(buf, zigzag);
}

/// Decode a zig-zag varint signed integer.
pub fn decode_int(r: &mut Reader<'_>) -> Result<i64, CodecError> {
    let zigzag = read_varint(r)?;
    Ok(((zigzag >> 1) as i64) ^ -((zigzag & 1) as i64))
}

/// Encode a float payload: fixed 8-byte IEEE-754, big-endian.
pub fn encode_float(buf: &mut Vec<u8>, value: f64) {
    buf.extend_from_slice(&value.to_bits().to_be_bytes());
}

/// Decode an 8-byte IEEE-754 big-endian float.
pub fn decode_float(r: &mut Reader<'_>) -> Result<f64, CodecError> {
    let s = r.take(8)?;
    Ok(f64::from_bits(u64::from_be_bytes(s.try_into().expect("take(8) guarantees exactly 8 bytes"))))
}

/// Encode a text payload: length-prefixed UTF-8 (== [`write_bytes`] of the
/// UTF-8 encoding).
pub fn encode_text(buf: &mut Vec<u8>, value: &str) {
    write_bytes(buf, value.as_bytes());
}

/// Decode a text payload. Validates UTF-8.
pub fn decode_text(r: &mut Reader<'_>) -> Result<String, CodecError> {
    let bytes = read_bytes(r)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| CodecError::Truncated)
}

/// Encode the constructor index of a sum payload. The caller then encodes that
/// constructor's fields, positionally. (Product payloads need no helper: encode
/// the fields in declared order with no framing.)
pub fn encode_sum_tag(buf: &mut Vec<u8>, ctor_index: u64) {
    write_varint(buf, ctor_index);
}

/// Decode a sum constructor index.
pub fn decode_sum_tag(r: &mut Reader<'_>) -> Result<u64, CodecError> {
    read_varint(r)
}

// ─────────────────────── NodeRef wire tag (Phase 2 contract) ────────────────

/// A Core IR graph edge in canonical (non-capnp) form: an index into either the
/// node table or the constant pool. The in-memory `NodeRef` type lands with the
/// Core IR data model in Phase 2; this fixes the wire contract now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeRefBin {
    /// Index into `CoreBundle.nodes` (topological invariant: < referrer index).
    Node(u32),
    /// Index into `CoreBundle.constantPool`.
    Pool(u32),
}

/// Encode a `NodeRef` as one varint: `node(i)` → `i<<1`, `pool(i)` → `(i<<1)|1`.
pub fn encode_noderef(buf: &mut Vec<u8>, r: NodeRefBin) {
    let tagged = match r {
        NodeRefBin::Node(i) => (i as u64) << 1,
        NodeRefBin::Pool(i) => ((i as u64) << 1) | 1,
    };
    write_varint(buf, tagged);
}

/// Decode a `NodeRef` tag varint.
pub fn decode_noderef(r: &mut Reader<'_>) -> Result<NodeRefBin, CodecError> {
    let tagged = read_varint(r)?;
    let index = (tagged >> 1) as u32;
    if tagged & 1 == 0 {
        Ok(NodeRefBin::Node(index))
    } else {
        Ok(NodeRefBin::Pool(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fabric::hash::content_hash;

    fn rt_varint(v: u64) {
        let mut buf = Vec::new();
        write_varint(&mut buf, v);
        let mut r = Reader::new(&buf);
        assert_eq!(read_varint(&mut r).unwrap(), v);
        r.expect_eof().unwrap();
    }

    #[test]
    fn varint_roundtrip_boundaries() {
        for v in [0u64, 1, 127, 128, 255, 256, 16383, 16384, u32::MAX as u64, u64::MAX] {
            rt_varint(v);
        }
    }

    #[test]
    fn varint_non_minimal_rejected() {
        // 0x80 0x00 encodes 0 with a redundant continuation byte.
        let bytes = [0x80u8, 0x00];
        let mut r = Reader::new(&bytes);
        assert_eq!(read_varint(&mut r), Err(CodecError::NonMinimalVarint));
    }

    #[test]
    fn varint_overflow_rejected() {
        // Ten 0xFF bytes then 0x7F would overflow 64 bits.
        let bytes = [0xFFu8; 11];
        let mut r = Reader::new(&bytes);
        assert!(matches!(
            read_varint(&mut r),
            Err(CodecError::VarintOverflow) | Err(CodecError::Truncated)
        ));
    }

    #[test]
    fn bool_roundtrip_and_reject() {
        let mut buf = Vec::new();
        write_bool(&mut buf, true);
        write_bool(&mut buf, false);
        let mut r = Reader::new(&buf);
        assert!(read_bool(&mut r).unwrap());
        assert!(!read_bool(&mut r).unwrap());

        let bad = [0x02u8];
        let mut r2 = Reader::new(&bad);
        assert_eq!(read_bool(&mut r2), Err(CodecError::InvalidBool(0x02)));
    }

    #[test]
    fn int_zigzag_roundtrip() {
        for v in [0i64, -1, 1, -2, 2, i64::MIN, i64::MAX, 1234567, -7654321] {
            let mut buf = Vec::new();
            encode_int(&mut buf, v);
            let mut r = Reader::new(&buf);
            assert_eq!(decode_int(&mut r).unwrap(), v);
            r.expect_eof().unwrap();
        }
    }

    #[test]
    fn float_roundtrip() {
        for v in [0.0f64, -0.0, 1.5, -2.25, f64::MAX, f64::MIN, f64::INFINITY] {
            let mut buf = Vec::new();
            encode_float(&mut buf, v);
            let mut r = Reader::new(&buf);
            let got = decode_float(&mut r).unwrap();
            assert_eq!(got.to_bits(), v.to_bits());
        }
    }

    #[test]
    fn text_and_bytes_roundtrip() {
        let mut buf = Vec::new();
        encode_text(&mut buf, "axis ✦ core");
        write_bytes(&mut buf, &[0xDE, 0xAD, 0xBE, 0xEF]);
        let mut r = Reader::new(&buf);
        assert_eq!(decode_text(&mut r).unwrap(), "axis ✦ core");
        assert_eq!(read_bytes(&mut r).unwrap(), &[0xDE, 0xAD, 0xBE, 0xEF]);
        r.expect_eof().unwrap();
    }

    #[test]
    fn hash_roundtrip() {
        let h = content_hash(b"payload");
        let mut buf = Vec::new();
        write_hash(&mut buf, &h);
        assert_eq!(buf.len(), HASH_LEN);
        let mut r = Reader::new(&buf);
        assert_eq!(read_hash(&mut r).unwrap(), h);
    }

    #[test]
    fn noderef_tagging() {
        for r in [
            NodeRefBin::Node(0),
            NodeRefBin::Pool(0),
            NodeRefBin::Node(1),
            NodeRefBin::Pool(7),
            NodeRefBin::Node(u32::MAX),
        ] {
            let mut buf = Vec::new();
            encode_noderef(&mut buf, r);
            let mut rd = Reader::new(&buf);
            assert_eq!(decode_noderef(&mut rd).unwrap(), r);
        }
    }

    #[test]
    fn sum_tag_roundtrip() {
        let mut buf = Vec::new();
        encode_sum_tag(&mut buf, 3);
        let mut r = Reader::new(&buf);
        assert_eq!(decode_sum_tag(&mut r).unwrap(), 3);
    }

    #[test]
    fn trailing_bytes_detected() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 5);
        buf.push(0x00); // stray
        let mut r = Reader::new(&buf);
        let _ = read_varint(&mut r).unwrap();
        assert_eq!(r.expect_eof(), Err(CodecError::TrailingBytes(1)));
    }
}

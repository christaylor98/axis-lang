//! Content hashing and 256-bit identity minting — the fabric's one hash regime.
//!
//! Phase 0 decisions (locked 2026-06-05):
//!
//! * **Content hash = SHA-256.** 32 bytes == the `Hash256` width the Core IR and
//!   Registry schemas already assume (4×u64). Deterministic, ubiquitous, and
//!   `sha2` was already a dependency. Swappable: every hash flows through
//!   [`content_hash`], so changing the function is a one-line edit here.
//!
//! * **Function identity = entropy at width.** A minted identity is 256 fresh
//!   random bits ([`mint_identity`]). No coordination channel is required for
//!   uniqueness — this is the only scheme that survives minting on disconnected
//!   systems (Registry Format §4). An optional minter/context prefix is supported
//!   ([`mint_identity_prefixed`]) for forensics/partition-safety; unused for now.
//!
//! `Hash256` is the single 256-bit token type used for all three fabric uses:
//! type definition hashes (`def_hash`), minted function identities
//! (`CCall.targetIdentity`), and axStorage content references.

use sha2::{Digest, Sha256};

/// Width of a fabric hash / identity token, in bytes.
pub const HASH_LEN: usize = 32;

/// A 256-bit content hash or minted identity token.
///
/// Canonical form is the 32-byte array. The capnp schema models the same value
/// as four big-endian 64-bit words (`w0` most-significant); [`Hash256::to_words`]
/// / [`Hash256::from_words`] bridge the two without ambiguity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash256(pub [u8; HASH_LEN]);

impl Hash256 {
    /// The all-zero token. Used as the canonical "absent reference" value
    /// (Registry Format §1: a field with no value uses its zero form).
    pub const ZERO: Hash256 = Hash256([0u8; HASH_LEN]);

    /// Raw bytes (canonical form).
    pub fn as_bytes(&self) -> &[u8; HASH_LEN] {
        &self.0
    }

    /// Construct from raw bytes.
    pub fn from_bytes(bytes: [u8; HASH_LEN]) -> Self {
        Hash256(bytes)
    }

    /// True iff this is the all-zero "absent reference" token.
    pub fn is_zero(&self) -> bool {
        self.0 == [0u8; HASH_LEN]
    }

    /// Lowercase hex (64 chars).
    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(HASH_LEN * 2);
        for b in &self.0 {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }

    /// Parse from a 64-char lowercase/uppercase hex string.
    pub fn from_hex(s: &str) -> Result<Self, HashParseError> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        if s.len() != HASH_LEN * 2 {
            return Err(HashParseError::BadLength(s.len()));
        }
        let mut out = [0u8; HASH_LEN];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = hex_val(chunk[0]).ok_or(HashParseError::BadDigit)?;
            let lo = hex_val(chunk[1]).ok_or(HashParseError::BadDigit)?;
            out[i] = (hi << 4) | lo;
        }
        Ok(Hash256(out))
    }

    /// The four big-endian 64-bit words (`w0` is the most-significant 8 bytes),
    /// matching the capnp `Hash256` struct field order.
    pub fn to_words(&self) -> (u64, u64, u64, u64) {
        let w = |i: usize| {
            u64::from_be_bytes(self.0[i * 8..i * 8 + 8].try_into().unwrap())
        };
        (w(0), w(1), w(2), w(3))
    }

    /// Build from the four big-endian words of the capnp `Hash256` struct.
    pub fn from_words(w0: u64, w1: u64, w2: u64, w3: u64) -> Self {
        let mut out = [0u8; HASH_LEN];
        out[0..8].copy_from_slice(&w0.to_be_bytes());
        out[8..16].copy_from_slice(&w1.to_be_bytes());
        out[16..24].copy_from_slice(&w2.to_be_bytes());
        out[24..32].copy_from_slice(&w3.to_be_bytes());
        Hash256(out)
    }
}

impl std::fmt::Debug for Hash256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Short form: first 8 hex chars, enough to disambiguate in logs.
        write!(f, "Hash256({}…)", &self.to_hex()[..8])
    }
}

impl std::fmt::Display for Hash256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Error parsing a hex hash string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HashParseError {
    BadLength(usize),
    BadDigit,
}

impl std::fmt::Display for HashParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HashParseError::BadLength(n) => {
                write!(f, "expected {} hex chars, got {}", HASH_LEN * 2, n)
            }
            HashParseError::BadDigit => write!(f, "invalid hex digit"),
        }
    }
}

impl std::error::Error for HashParseError {}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// The fabric content hash. Identity of any content-addressed artifact
/// (a `TypeDef`, a registry record's content hash, a Core IR bundle) is
/// `content_hash(its canonical bytes)`.
pub fn content_hash(bytes: &[u8]) -> Hash256 {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Hash256(hasher.finalize().into())
}

/// Mint a fresh 256-bit function identity from system entropy.
///
/// The function's content and signature contribute nothing to its identity —
/// this is what keeps two disconnected, same-signature functions correctly
/// distinct rather than silently fused (Registry Format §4).
///
/// Panics only if the OS entropy source is unavailable, which is unrecoverable.
pub fn mint_identity() -> Hash256 {
    let mut buf = [0u8; HASH_LEN];
    getrandom::getrandom(&mut buf).expect("OS entropy source unavailable");
    Hash256(buf)
}

/// Mint an identity whose leading bytes are an opaque minter/context prefix and
/// whose remaining bytes are entropy. The prefix is forensic only and is
/// truncated to at most `HASH_LEN` bytes. Not currently used by the pipeline;
/// provided so partition-safe minting can be enabled without a format change.
pub fn mint_identity_prefixed(prefix: &[u8]) -> Hash256 {
    let mut buf = [0u8; HASH_LEN];
    getrandom::getrandom(&mut buf).expect("OS entropy source unavailable");
    let n = prefix.len().min(HASH_LEN);
    buf[..n].copy_from_slice(&prefix[..n]);
    Hash256(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let h = content_hash(b"axis");
        let s = h.to_hex();
        assert_eq!(s.len(), 64);
        assert_eq!(Hash256::from_hex(&s).unwrap(), h);
        assert_eq!(Hash256::from_hex(&format!("0x{}", s)).unwrap(), h);
    }

    #[test]
    fn words_roundtrip() {
        let h = content_hash(b"core ir 0.5");
        let (w0, w1, w2, w3) = h.to_words();
        assert_eq!(Hash256::from_words(w0, w1, w2, w3), h);
    }

    #[test]
    fn content_hash_is_deterministic() {
        assert_eq!(content_hash(b"same"), content_hash(b"same"));
        assert_ne!(content_hash(b"a"), content_hash(b"b"));
    }

    #[test]
    fn zero_token() {
        assert!(Hash256::ZERO.is_zero());
        assert!(!content_hash(b"x").is_zero());
    }

    #[test]
    fn minted_identities_are_distinct() {
        // Two mints of "the same function" must not collide.
        assert_ne!(mint_identity(), mint_identity());
    }

    #[test]
    fn prefix_is_preserved() {
        let id = mint_identity_prefixed(&[0xAB, 0xCD]);
        assert_eq!(id.as_bytes()[0], 0xAB);
        assert_eq!(id.as_bytes()[1], 0xCD);
    }

    #[test]
    fn bad_hex_rejected() {
        assert!(matches!(
            Hash256::from_hex("zz"),
            Err(HashParseError::BadLength(_))
        ));
        let sixtyfour_zs = "z".repeat(64);
        assert_eq!(Hash256::from_hex(&sixtyfour_zs), Err(HashParseError::BadDigit));
    }
}

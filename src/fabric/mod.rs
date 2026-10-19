//! The fabric: the shared substrate primitives underneath Core IR, the Registry,
//! and (later) the Ledger.
//!
//! Three things live here, and only things every stratum shares:
//!
//! * [`hash`] — the one hash regime. `Hash256` (256-bit token), `content_hash`
//!   (SHA-256 of canonical bytes), and `mint_identity` (entropy-at-width).
//! * [`codec`] — the one canonical serialization regime. Deterministic encoding
//!   primitives (varint, hash, bool, bytes, seq) and value payload encoders,
//!   shared by registry records, constant-pool entries, and the canonical bundle.
//!
//! Design rule (locked Phase 0): **identity is the content hash of canonical
//! bytes produced by [`codec`], never of capnp-framed bytes.** capnp is a
//! transport encoding; the meaning hash is computed over the codec's output so
//! it is stable across capnp schema versions and wire framing changes.

pub mod codec;
pub mod hash;

pub use codec::{CodecError, NodeRefBin, Reader};
pub use hash::{content_hash, mint_identity, mint_identity_prefixed, Hash256, HASH_LEN};

Title: Redb-based IPS Search Index — Postcard-Standardized Binary Encodings

Author: Junie (JetBrains AI Coding Agent)
Date: 2025-11-12
Status: Planning Document (encoding revision to standardize all blobs on postcard)

0. Motivation (what and why)

- Requirement: for any encoded binary blob, use the postcard format to standardize serialization and simplify parsing in Rust.
- Scope: Update the simplified Redb index plan so that all structured values stored as redb value blobs are serialized with postcard via serde, replacing the custom ad-hoc varint framing proposed earlier.
- Benefits:
  - Consistent, well-tested serialization with minimal overhead and no_std compatibility.
  - Straightforward Rust implementation using #[derive(Serialize, Deserialize)].
  - Reduced hand-rolled codec surface area and fewer edge cases.

1. Affected schema elements (values become postcard-encoded)

- Keys remain raw UTF-8 strings as previously specified, to keep lookups simple. The change applies to values that carry structured data:
  - postings: token → postings groups with offsets (was: custom LEB128 + lengths). Now: postcard of Rust structs defined below.
  - mini_delta: token → delta postings entries (was: custom). Now: postcard.
  - fast_add and fast_remove values: retain empty (unit) or timestamp string; optionally postcard-encode a small struct for uniformity (see below).
  - meta values: previously plain u32 for schema_version. We will keep primitive numbers for trivial singletons, but allow postcard-encoded structs if/when meta grows.
  - fmri_catalog_hash value: keep as UTF-8 hex string (interoperability with external tools); alternatively add a postcard mirror key if needed later.

Schema version bump: Increment meta.schema_version from 1 → 2 to denote postcard adoption for postings and mini_delta tables.

2. Rust data model (serde-friendly, postcard-serializable)

Use serde + postcard for all structured blobs. The types below precisely mirror the hierarchical structure used by pkg5 and the simplified plan, optimized for Rust parsing.

2.1 Common types

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct OffsetList {
    // Absolute manifest offsets in strictly increasing order.
    pub offsets: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PostingPairId {
    pub fmri_id: u32,
    pub positions: OffsetList,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PostingPairStr {
    pub fmri_str: String,
    pub positions: OffsetList,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FullValueGroupId {
    pub full_value: String,
    pub pairs: Vec<PostingPairId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FullValueGroupStr {
    pub full_value: String,
    pub pairs: Vec<PostingPairStr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubtypeGroupId {
    pub subtype: String,
    pub full_values: Vec<FullValueGroupId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct SubtypeGroupStr {
    pub subtype: String,
    pub full_values: Vec<FullValueGroupStr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ActionTypeGroupId {
    pub action_type: String,
    pub subtypes: Vec<SubtypeGroupId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ActionTypeGroupStr {
    pub action_type: String,
    pub subtypes: Vec<SubtypeGroupStr>,
}

/// Postings value stored in `postings` table (token → this), using fmri_id for compactness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PostingsValueId {
    pub groups: Vec<ActionTypeGroupId>,
}

/// Postings value stored in `mini_delta` table (token → this), using fmri_str for easy fast-path writes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PostingsValueStr {
    pub groups: Vec<ActionTypeGroupStr>,
}
```

Notes:
- Offsets are absolute here for simplicity; postcard’s compact varints plus delta-at-build-time remain viable as a pre-serialization optimization if desired. If we want to retain delta semantics, we can change `OffsetList` to store deltas and normalize at read.
- We intentionally keep field names stable to benefit from serde’s default representation with postcard.

2.2 Optional uniformity for fast_add/fast_remove values

To fully standardize “any binary blob,” we can encode fast_add/remove values as postcard too, while keeping keys as the fmri string:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct FastMarker {
    // Optional metadata; empty means just a presence marker.
    pub timestamp_iso8601: Option<String>,
}
```

3. Table specifications with postcard encodings

- postings
  - Key: UTF-8 token
  - Value: postcard(PostingsValueId)

- mini_delta
  - Key: UTF-8 token
  - Value: postcard(PostingsValueStr)

- fast_add
  - Key: UTF-8 fmri
  - Value: unit (empty) OR postcard(FastMarker) if we decide to populate timestamps; readers must accept either for schema_version 2.

- fast_remove
  - Key: UTF-8 fmri
  - Value: unit (empty) OR postcard(FastMarker)

- fmri_catalog (unchanged)
  - id_to_str: key = (0x00, u32) → value = UTF-8 fmri (string is not a “binary blob” and benefits from direct storage)
  - str_to_id: key = (0x01, UTF-8 fmri) → value = u32 id

- fmri_catalog_hash (unchanged)
  - Key: 0x00
  - Value: UTF-8 lowercase hex SHA-1

- meta
  - Key: "schema_version" → Value: u32 (set to 2)
  - Future composite meta records MAY be postcard-encoded structs.

4. Dependency guidance (per project error-handling/dependency policy)

Library crates (e.g., libips):

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
postcard = { version = "1", features = ["use-std"] }
thiserror = "1.0.50"
miette = "7.6.0"
tracing = "0.1.37"
```

Application crates keep their existing miette configuration (fancy in apps only) and add postcard/serde if they interact with the index directly.

5. Read/write rules (implementation notes)

- Writers
  - Build PostingsValueId/PostingsValueStr structures in memory, serialize with postcard::to_allocvec or to_stdvec, and store as the redb value bytes.
  - Enforce invariants: strictly increasing offsets, valid fmri_id references, disjoint fast_add/fast_remove.

- Readers
  - Fetch value bytes from redb and decode with postcard::from_bytes into the matching struct.
  - Merge logic: postings[token] (Id) ∪ mini_delta[token] (Str) with joins to fmri_catalog and fast_add/remove exactly as in the simplified plan.

Performance notes:
- Postcard uses compact varint-like encodings and typically yields sizes close to hand-rolled varints without the maintenance burden. If needed, we can pre-delta-encode offsets before serialization (store deltas in OffsetList) and restore absolute on read.

6. Migration and backward compatibility

- schema_version bump: Set meta.schema_version = 2 when the index is built with postcard encodings for postings and mini_delta.
- Readers should support both versions during the transition:
  - If schema_version == 1: decode custom ad-hoc blobs (legacy path).
  - If schema_version == 2: decode postcard structs defined above.
- One-time converter: implement a small tool that opens the redb database, reads v1 blobs, converts to structs, writes v2 values in a single atomic write transaction, updates schema_version, and clears any v1-only artifacts. This can live under xtask or a dedicated migration command.

7. Error handling (aligns with project guidelines)

- Define errors like EncodeError, DecodeError, SchemaMismatch, InvalidOffsets, MissingFmriId, TxnFailure in libips with thiserror + miette::Diagnostic derives.
- Wrap postcard errors with a transparent source in DecodeError/EncodeError.
- Application crates use miette::Result for convenience.

8. Testing plan updates

- Unit tests: round-trip postcard serialization for all structs; invariants on offsets; merge correctness (Id ∪ Str).
- Property tests: generate random postings structures and assert from_bytes(to_vec(x)) == x; ensure offsets remain strictly increasing after any delta/absolute conversions.
- Back-compat tests: v1 blobs decode → v2 structs encode → re-decode equality for semantic content.
- Concurrency: unchanged (rely on redb MVCC); ensure no partial writes by using single-transaction updates.

9. Implementation roadmap deltas

- Phase A (Postcard types):
  - Add the serde types above under libips::search::index::types.
  - Implement postcard encode/decode helpers in libips::search::index::codec.
  - Gate by schema_version; keep legacy decoder behind the same module for migration.

- Phase B (Writers/Readers):
  - Update full rebuild writer to produce PostingsValueId and write as postcard.
  - Update fast path to produce PostingsValueStr.
  - Update read path to decode postcard when schema_version == 2.

- Phase C (Migration Tooling):
  - Implement an xtask subcommand to migrate v1 → v2 in-place atomically.

Appendix: Rationale for keeping some values as plain strings or integers

- fmri_catalog values and fmri_catalog_hash are single UTF-8 strings and not multi-field blobs; storing them as raw strings avoids unnecessary serde overhead and maintains interop with tools that may read them directly.
- meta.schema_version is a simple u32; postcard would not add value here. If meta grows into a multi-field record, we will switch that specific value to postcard.

Summary

This revision standardizes all structured binary blobs on postcard while preserving the simplified schema, operations, and invariants. It reduces custom codec complexity, aligns with Rust best practices (serde), and provides a clear migration path via a schema_version bump and dual-decoder support during transition.

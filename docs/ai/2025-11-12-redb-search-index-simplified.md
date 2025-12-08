Title: Redb-based IPS Search Index — Simplified Plan (MVCC-first, Rust-friendly)

Author: Junie (JetBrains AI Coding Agent)
Date: 2025-11-12
Status: Planning Document (supersedes complex/optional parts; implementation to follow)

0. What changed in this revision (why simpler)

- Remove epoch partitioning and any indirection tables. Rely on redb MVCC and atomic write transactions for consistency. No more active_epoch flips.
- Remove optional/aux tables and features for v1: no token_offsets, no fmri_offsets indirection, no string-intern tables, no chunking. Keep a single canonical encoding optimized for Rust parsing.
- Keep feature parity with pkg5 using a mandatory mini delta index for fast updates. The fast path is always supported without schema toggles.
- Keep encodings compact and deterministic: length-prefixed UTF-8 and LEB128 varints; no URL-quoting on disk.

1. Goals (unchanged intent, simplified mechanism)

- Functional equivalence with pkg5 search results and fast update behavior.
- Atomic multi-table updates via a single redb write transaction; readers get consistent snapshots automatically.
- Minimal schema that is straightforward to implement, test, and migrate.

2. Minimal Database Schema (fixed, no epochs)

All tables live in one redb database file (e.g., index.redb). Schema versioning is done via a single meta record.

- meta
  - Key: "schema_version"
  - Value: u32 (starts at 1)

- fmri_catalog
  - Purpose: dense id ↔ string mapping for fmris used in postings
  - Keys/Values:
    - id_to_str: key = (0x00, u32 id) → value = utf8 fmri
    - str_to_id: key = (0x01, utf8 fmri) → value = u32 id
  - Policy: ids are compact (0..N-1). Rebuilds may reassign ids; fast delta uses strings and is joined at query time.

- postings
  - Purpose: main inverted index token → grouped postings with per-manifest offsets
  - Key: utf8 token (exact, unescaped)
  - Value: binary blob encoded as:
    - at_group_count: varint
    - repeat at_group_count times:
      - action_type: len(varint) + utf8 bytes
      - sub_group_count: varint
      - repeat sub_group_count times:
        - subtype: len(varint) + utf8 bytes
        - fv_group_count: varint
        - repeat fv_group_count times:
          - full_value: len(varint) + utf8 bytes
          - pair_count: varint
          - repeat pair_count times:
            - fmri_id: u32 (LE)
            - offsets_count: varint
            - offsets_delta[varint; length=offsets_count] (d0=a0, di=a[i]-a[i-1])
  - Notes: strictly increasing offsets; no compression or chunking in v1.

- mini_delta
  - Purpose: mandatory mini token index for fast updates (additions only). Mirrors postings schema but values may reference fmris by string to avoid id assignment.
  - Key: utf8 token
  - Value: binary blob encoded as:
    - at/sub/fv hierarchy identical to postings
    - pair_count: varint
    - repeat pair_count times:
      - fmri_str: len(varint) + utf8 bytes
      - offsets_count: varint
      - offsets_delta[varint]
  - Rationale: keeps fast path independent of fmri_catalog churn; simplifies writer.

- fast_add
  - Purpose: set of fmri strings added since last rebuild
  - Key: utf8 fmri
  - Value: unit (empty) or timestamp string (optional)

- fast_remove
  - Purpose: set of fmri strings removed since last rebuild
  - Key: utf8 fmri
  - Value: unit (empty) or timestamp string (optional)

- fmri_catalog_hash
  - Purpose: preserve pkg5 integrity feature
  - Key: 0x00
  - Value: utf8 lowercase hex SHA-1 of sorted fmri strings currently in fmri_catalog (id_to_str)

- locks (optional but simple)
  - Key: "index_writer"
  - Value: utf8 holder + timestamp (opaque)

3. Operations (simplified)

3.1 Consistent reads
- Open a read transaction. Query tables directly; snapshot isolation ensures consistency.
- For token queries: result = postings[token] merged with mini_delta[token]; then subtract any hits whose fmri (mapped via fmri_catalog or taken from mini_delta) is present in fast_remove; finally, union results with any mini_delta entries whose fmri is in fast_add (already included by merge).
- For listing fmris: list = (all fmri_catalog ids → strings) ∪ fast_add \ fast_remove; integrity hash = fmri_catalog_hash.

3.2 Fast update (client/server small change sets)
- Start write txn.
- For each added fmri: insert fmri string into fast_add, remove from fast_remove if present. Append/merge token postings for that fmri into mini_delta at the token granularity.
- For each removed fmri: insert fmri string into fast_remove, remove from fast_add if present. Optionally prune any mini_delta entries for that fmri.
- Commit. Readers immediately see the delta via MVCC.

3.3 Full rebuild
- Start write txn.
- Recompute fmri_catalog from manifests (dense ids), fmri_catalog_hash from catalog.
- Rebuild postings from scratch (tokenize, group, encode).
- Clear mini_delta, fast_add, fast_remove.
- Commit. Done. No epoch flips necessary.

4. Rust-friendly encoding rules

- Strings: UTF-8 with varint length prefix; decode with a zero-copy slice + from_utf8.
- Integers: LEB128 varints for counts and deltas; fmri_id stored as fixed u32 LE for fast decoding.
- Hierarchy order is fixed and deterministic; all counts come before their sequences to enable single-pass decoding.
- Keys are raw UTF-8 tokens/fmris; no URL-quoting required.

5. Invariants

- postings references only valid fmri_id values present in fmri_catalog at the time of commit.
- offsets decode to strictly increasing absolute positions.
- fast_add ∩ fast_remove = ∅ (writers maintain disjointness).
- mini_delta entries reference fmri strings; when a full rebuild commits, mini_delta MUST be empty.
- fmri_catalog ids are 0..N-1 contiguous.

6. Migration (from pkg5 text index)

Step-by-step importer (single write txn):
- Build fmri_catalog from manf_list.v1; compute fmri_catalog_hash from full_fmri_list.
- Convert main_dict.ascii.v2 lines into postings values (URL-unquote tokens and full values; map pfmri_index → fmri_id; parse offsets and store delta-encoded).
- Initialize mini_delta empty.
- Copy fast_add/fast_remove lines into respective sets.
- Commit. Result is immediately queryable and functionally equivalent.

7. Error handling (per project guidelines)

- Library code (libips): define specific error enums with thiserror and miette::Diagnostic derives (no fancy feature in lib). Errors include: SchemaMismatch, DecodeError, InvalidOffsets, MissingFmriId, TxnFailure.
- Application crates: use miette::Result and attach helpful diagnostics.

8. Testing plan (focused for simplified schema)

- Unit tests: encoder/decoder for postings and mini_delta, varint/delta round-trips, fmri_catalog id assignment.
- Property tests: offsets strictly increasing after decode; random postings encode/decode equality.
- Concurrency: readers during fast update and during rebuild; verify stable snapshots.
- Migration: import sample pkg5 index; compare query results token-by-token with legacy.

9. Implementation roadmap (tight scope)

- Phase A: Define codecs and table handles in libips::search::index; implement full rebuild writer + read path.
- Phase B: Implement fast path (mini_delta + fast_add/remove) with merge logic; add pruning on removals.
- Phase C: Importer from pkg5 files; verification utilities; basic GC (mini_delta cleanup is already enforced; no epochs to GC).

Appendix: Removed/Deferred items and rationale

- Epochs: removed — redb’s atomic transactions and MVCC provide consistent multi-table updates without indirection.
- token_offsets and fmri_offsets tables: removed — premature optimization; postings are single-key fetches. Can be reconsidered if profiling shows need.
- Chunking/postings compression/interning: deferred — start with a straightforward encoding; add compression or interning only if performance data demands it.

This simplified plan keeps the same user-visible behavior as pkg5 with fewer moving parts, favoring redb’s built-in guarantees and a Rust-friendly, deterministic binary encoding.

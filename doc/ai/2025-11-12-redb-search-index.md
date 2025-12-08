Title: Redb-based IPS Search Index — Design Plan and Format Specification

Author: Junie (JetBrains AI Coding Agent)
Date: 2025-11-12
Status: Planning Document (implementation to follow)

1. Motivation and Goals

- Provide a new on-disk search index format using the redb embedded database while remaining functionally equivalent to the pkg5 search index.
- Preserve behavior and semantics of the existing client/server search pipelines (fast incremental updates vs. full rebuilds, consistent reader view, and compatibility of results).
- Improve robustness and consistency guarantees by leveraging redb’s ACID transactions and MVCC snapshots instead of ad-hoc multi-file versioning and migration.
- Keep storage efficient (delta/varint compression for offsets, dedup of common structures) and query-time fast.

Functional equivalence targets (from pkg5 search index):
- Inverted index from token → postings grouped by action type, subtype, full_value, with per-manifest offsets.
- Ability to serve consistent reads while writer updates occur.
- Fast incremental updates for small package change sets using append-only “fast add/remove” semantics without rebuilding main dictionaries.
- Full rebuild path for large changes or inconsistencies.
- A complete set of FMRIs represented by the index and an integrity hash equivalent to full_fmri_list + full_fmri_list.hash.
- Ability to map between manifest ids and fmri strings (manf_list.v1 semantics).
- Deduped representation for fmri offset sets (fmri_offsets.v1 semantics) and delta-compressed offsets.

2. Redb Background and Mapping Strategy

Redb is an embedded, crash-safe, ACID key-value store with MVCC snapshots:
- Readers operate on a consistent snapshot without blocking writers.
- Writers use transactions with atomic commit.
- Data is organized into named tables (key/value types specified) and supports zero-copy reads where possible.

We model each prior index “file” as one or more redb tables. We retain a global logical VERSION (called epoch) to preserve pkg5’s contract and for interop with external tools if needed. However, consistency is provided by redb transactions; the epoch is a semantic marker rather than the mechanism.

3. Global Concepts and Conventions

- Epoch: A monotonically increasing u64 stored in a metadata table indicating the current logical version of the index state visible to new readers. A rebuild creates a shadow epoch, fully populates it, and then atomically flips the active epoch pointer.
- Names and quoting: For tokens and full values that previously required URL-quoting, we store the unescaped UTF-8 token/value with a binary-safe key encoding. If any legacy wire format requires exact URL-quoted strings, we compute them at query time.
- Numeric encodings: Use compact varint (LEB128) for integer lists and delta-encoded offsets; redb values remain binary blobs with structured framing described below.
- Compression: Optional lightweight compression for large value blobs (e.g., lz4-frame). The initial implementation can make this configurable per-table.

4. Database Layout (Tables and Types)

4.1 meta (singleton)
- Purpose: Global metadata and pointers.
- Key: fixed strings (e.g., "active_epoch", "next_epoch").
- Value formats:
  - active_epoch: u64 (the epoch new readers should use).
  - last_full_rebuild_epoch: u64.
  - created_at / updated_at: RFC3339 timestamp strings.
  - schema_version: u32 for this redb schema.

4.2 epochs (catalog of epochs)
- Purpose: Track epoch lifecycle and allow safe garbage collection of old data.
- Key: u64 epoch.
- Value: struct:
  - state: enum { Building, Active, Retired }
  - built_from: Option<u64> (previous active epoch)
  - created_at, activated_at, retired_at: timestamps
  - stats: optional counts (tokens, postings bytes, fmris)

4.3 token_postings (partitioned by epoch)
- Purpose: Main inverted index; replaces main_dict.ascii.v2.
- Table name pattern: token_postings:e{epoch}
- Key: token_key
  - token_key encoding: a prefix byte 0x00 followed by UTF-8 token bytes. A secondary composite layout is allowed: [action_type, subtype, full_value] are not part of the key; see value layout below.
- Value: postings blob encoded as:
  struct PostingsValue {
    // One token’s postings grouped by action type and subtype and full_value.
    // varint for sizes; strings are UTF-8 length-prefixed varint.
    at_groups: Vec<AtGroup>
  }
  struct AtGroup {
    action_type: String
    sub_groups: Vec<SubGroup>
  }
  struct SubGroup {
    subtype: String
    fv_groups: Vec<FvGroup>
  }
  struct FvGroup {
    full_value: String
    // fmri_id -> offsets (delta-encoded, varint list)
    // Store as list of pairs for sparse storage
    pairs: Vec<(u32 fmri_id, Vec<u32> offsets_delta)>
  }
  // Entire blob may be compressed using lz4 if size exceeds threshold.

Notes:
- This preserves the hierarchical grouping and the per-manifest offsets from pkg5.
- For very high-cardinality tokens, we may shard across multiple rows by introducing a chunk ordinal in the key (token_key + chunk_id). The chunking strategy can be added later without breaking the base schema; record chunk_count in a side table if used.

4.4 token_offsets (optional accelerator; replaces token_byte_offset.v1)
- Purpose: Random access accelerator if token_postings values become very large. In redb we can often fetch the single value by key; thus this table is optional. If chunking is implemented, this stores per-token chunk byte ranges for fast partial reads if we adopt a file-like storage.
- Table name: token_offsets:e{epoch}
- Key: token_key
- Value: struct { total_len: u32, chunks: Vec<(chunk_id: u16, offset: u32, len: u32)> }

4.5 fast_add and fast_remove (incremental logs)
- Purpose: Client-side/apply small updates quickly without full rebuild; replaces fast_add.v1 / fast_remove.v1.
- Table names: fast_add, fast_remove (not epoch-scoped; they represent deltas relative to active epoch).
- Key: fmri string (UTF-8).
- Value: struct { when: timestamp, epoch_base: u64 } or unit (). Presence indicates membership.
Notes:
- On activation of a new epoch after full rebuild, these logs are drained/cleared.
- Queries union active epoch results with fast_add set and subtract fast_remove set when calculating current state.

4.6 fmri_catalog (full membership; replaces full_fmri_list)
- Purpose: Enumerate all FMRIs represented by the active index epoch.
- Table name: fmri_catalog:e{epoch}
- Key: u32 fmri_id (dense ids for compact postings); ids are stable within an epoch.
- Value: fmri string (UTF-8).
Auxiliary: fmri_lookup:e{epoch} mapping fmri string → u32 id for convenience during rebuild.

4.7 fmri_catalog_hash (integrity; replaces full_fmri_list.hash)
- Table name: fmri_catalog_hash:e{epoch}
- Key: const 0x00
- Value: hex lowercase SHA-1 of sorted fmri strings from fmri_catalog:e{epoch}.

4.8 fmri_offsets (dedup store; replaces fmri_offsets.v1 semantics)
- Purpose: Deduplicate common offset lists shared across FMRIs or tokens.
- Table name: fmri_offsets:e{epoch}
- Key: content hash (e.g., blake3 of absolute offsets encoding).
- Value: delta-encoded varint list of offsets; may be lz4-compressed. Optionally also store a small header with original length/first few entries to detect corruption early.
Usage: token_postings FvGroup pairs may reference either inline offsets or an indirect reference by hash if the list exceeds size threshold. To keep lookups simple initially, we will inline offsets; indirect storage is a future optimization.

4.9 locks (writer serialization)
- Table: locks
- Key: "index_writer"
- Value: struct { holder: string, since: timestamp }
Notes: While redb provides transactions, we still serialize rebuilds/updates to mirror the single-writer expectation of pkg5. On server, publishing already serializes indexing; on client, operations are serialized by image plan execution. We can also keep the historical file-based lock for external tools, but within redb, this table suffices.

5. Operations

5.1 Consistent Reads
- Readers begin a read transaction and fetch meta.active_epoch to locate epoch-scoped tables.
- Because redb is MVCC, the entire read uses a consistent snapshot of those tables; concurrent writers do not affect the transaction’s view.
- Readers also fetch fast_add and fast_remove sets to adjust responses. To avoid race conditions, readers read fast_* with the same snapshot, and can safely compute: effective_installed = (epoch state ∪ fast_add) \ fast_remove.

5.2 Fast Incremental Update
Trigger: Small number of fmris added/removed on client or small publish window on server.
Steps:
1) Start write transaction.
2) Insert fmri strings into fast_add or fast_remove tables accordingly (idempotent puts; remove from opposite table if present).
3) Optionally update a small counter or watermark (e.g., meta.fast_delta_count).
4) Commit. No changes to token_postings or fmri_catalog for fast path.

Query impact:
- During query evaluation, postings derived from the active epoch are merged with delta postings computed on the fly for the fast_* fmris if we choose to precompute quick shards. Initial implementation can simply filter the result set by membership: when returning matched manifests, subtract those in fast_remove and optionally include those in fast_add only if we maintain their postings in a small side cache. Two options:
  A. Ultra-minimal: fast_* only modifies membership for fmri listing queries; token-based matches for newly added packages require full rebuild or a small background mini-index.
  B. Practical equivalence: maintain an auxiliary mini-index table mini_token_postings built incrementally for fast_* fmris. This table mirrors token_postings structure but only for the delta set. During query, we merge results from token_postings (epoch) and mini_token_postings, then subtract any fmris in fast_remove.

Chosen approach for equivalence: Option B (mini-token index) to match pkg5 behavior that answers queries without immediate full rebuild.

5.3 Full Rebuild
Trigger: Large change set, first-time indexing, detected inconsistency, or server bulk operations.
Steps:
1) Start write transaction; allocate new epoch E = meta.active_epoch + 1. Create epochs[E] = Building.
2) Compute fmri_catalog:eE and fmri_lookup:eE from manifests; compute fmri_catalog_hash:eE.
3) Build token_postings:eE by scanning manifests, tokenizing, and aggregating postings with offsets. Apply delta-encoding and optional lz4 compression for large values.
4) Optionally create auxiliary indices like token_offsets:eE and fmri_offsets:eE if enabled.
5) Update epochs[E] = Active and set meta.active_epoch = E in the same commit; set epochs[prev] = Retired.
6) Clear mini_token_postings and fast_add/fast_remove within the same or subsequent small transaction.

Crash safety:
- If the process crashes before flipping active_epoch, readers continue to use prev epoch.
- A subsequent startup task scans epochs table; any epochs in Building with no pointer from meta.active_epoch are GC candidates.

5.4 Garbage Collection and Compaction
- Retain N recent epochs for rollback/debug (configurable). Periodically remove Retired epochs older than retention.
- Use redb’s compaction/cleanup mechanisms as recommended by upstream to reclaim space.

6. Data Encodings

6.1 Strings
- UTF-8, length-prefix varint. Deduplicate common strings (action_type, subtype) via small intern tables if hot.

6.2 Integer Lists (Offsets)
- Store offsets as increasing u32 values; encode as delta varints: d0 = abs0, di = abs[i] - abs[i-1].
- For small lists (<= 4 items), inline in FvGroup; for larger, consider optional lz4 compression.

6.3 Keys
- Use simple UTF-8 token as key. For optional chunking: key = token + 0x1F separator + u16 chunk_id (big-endian) to keep lexicographic locality.

6.4 Checksums and Hashes
- fmri_catalog_hash:eE stores SHA-1 hex of sorted fmri strings to match pkg5 semantics.
- For internal dedup/validation, use blake3 for speed where not exposed.

7. Query Semantics and Equivalence

Token search path:
1) Read meta.active_epoch = E.
2) Lookup token_postings:eE[token]. If chunked, merge all chunks.
3) Lookup mini_token_postings[token] and merge into the same structure.
4) Apply fast_remove filter by excluding any fmri ids whose fmri string appears in fast_remove.
5) If the query path needs to output fmri strings, map fmri_id → string via fmri_catalog:eE; for mini index entries that reference fmri strings directly, assign transient ids or join by string.
6) Return groupings by action type, subtype, full_value with manifest offsets just like pkg5.

Full fmri list queries:
- Return fmri_catalog:eE ∪ fast_add \ fast_remove, ensure sorted order if requested, and provide fmri_catalog_hash:eE (unchanged) for backward-compat features.

Consistency guarantees:
- Readers never block writers (MVCC) and see a consistent epoch.
- Fast updates are visible immediately after their transaction commits.
- Epoch flips are atomic from readers’ perspective.

8. Migration Plan (from existing pkg5-style on-disk index)

Inputs: Existing $ROOT/index text files as specified in pkg5 search.txt.
Steps:
1) Parse existing files using a one-time importer tool in Rust.
2) Create redb database at $ROOT/index.redb/ (new directory) or reuse $ROOT/index with a different extension.
3) Initialize meta (schema_version=1) and create epoch E=1.
4) Fill fmri_catalog:e1 and fmri_lookup:e1 from manf_list.v1.
5) Convert main_dict.ascii.v2 and token_byte_offset.v1 into token_postings:e1 values. Honor URL-unquoting/quoting rules.
6) Populate fmri_offsets:e1 if we choose indirect references (optional initially).
7) Compute fmri_catalog_hash:e1 from full_fmri_list or reconstructed catalog.
8) Populate fast_add/fast_remove by copying logs if needed; or prefer to start with empty delta on first activation.
9) Activate epoch E and mark old files as deprecated. Keep old files read-only until confidence is achieved.

9. Error Handling and Recovery

- All writer operations use miette diagnostics in application crates and specific error types in libips (per repository guidelines). Errors include: redb transaction failures, data corruption checks, invalid manifest encodings, and invariant violations.
- Validation steps:
  - Ensure fmri ids are contiguous in fmri_catalog:eE.
  - Verify SHA-1 hash matches computed value.
  - Verify postings reference valid fmri ids and that offsets lists are strictly increasing after decoding.
  - For mini_token_postings, ensure referenced fmri strings exist in fast_add or are to be installed.
- Recovery:
  - If fast update fails, roll back the transaction; retry or fall back to full rebuild.
  - If a rebuild fails mid-way, epoch stays in Building and is GC’d later; readers continue using prior epoch.

10. Concurrency and Locking Details

- Single logical writer: Maintain a process-level lock (either OS file lock at $ROOT/index.lock for compatibility, or locks table in redb) to serialize rebuilds and bulk updates.
- Readers: No locks required; rely on MVCC.
- Writer never blocks readers: Guaranteed by redb’s snapshot isolation.

11. Performance Considerations

- Posting value size thresholds: If a token’s postings exceed N bytes, enable lz4 compression for the value.
- Hot string interning: Maintain two tiny epoch-scoped tables action_types:eE and subtypes:eE mapping string → u16 id to shrink postings.
- Streaming build: Build token_postings:eE in sorted token order to improve locality; batch commits.
- Mini-index size control: Evict older entries if fast delta grows beyond threshold; trigger background full rebuild when threshold is exceeded, mirroring MAX_FAST_INDEXED_PKGS behavior.

12. Configuration Knobs

- retention_epochs: number of retired epochs to keep.
- compress_threshold_bytes: per-value threshold for lz4.
- enable_indirect_offsets: bool to use fmri_offsets:eE.
- fast_update_threshold: mirrors pkg5 MAX_FAST_INDEXED_PKGS; exceeding triggers full rebuild.

13. Invariants (must hold)

- meta.active_epoch points to an epochs entry in Active state.
- All epoch-scoped tables for active epoch exist and are self-consistent.
- fmri ids in token_postings:eE reference existing entries in fmri_catalog:eE.
- Offsets decode to strictly increasing absolute positions.
- fmri_catalog_hash:eE matches the sorted fmri list.
- fast_add ∩ fast_remove = ∅; writes maintain disjointness.

14. Testing Plan

- Unit tests for encoding/decoding of postings and delta offsets.
- Property tests: round-trip manifests → postings → decode invariants.
- Concurrency tests: readers during rebuild and fast updates; ensure stable results.
- Migration tests: import from sample pkg5 index and validate equivalence of query results.
- End-to-end tests using cargo xtask setup-test-env to seed sample images/repos, index them, and run search queries.

15. Implementation Roadmap (high level)

Phase 1
- Define redb schemas and value codecs in libips::search::index.
- Implement full rebuild writer and read-only query path.
- Implement fast_add/remove tables without mini-token index; gate features.

Phase 2
- Add mini_token_postings and merge logic for full equivalence with pkg5 fast updates.
- Add compression and string interning optimizations.

Phase 3
- Migration tool from pkg5 text index; verification utilities.
- Epoch GC and compaction tooling.

Appendix A: Mapping of pkg5 files to redb tables

- main_dict.ascii.v2 → token_postings:e{epoch}
- token_byte_offset.v1 → token_offsets:e{epoch} (optional)
- fast_add.v1 → fast_add
- fast_remove.v1 → fast_remove
- full_fmri_list → fmri_catalog:e{epoch}
- full_fmri_list.hash → fmri_catalog_hash:e{epoch}
- manf_list.v1 → fmri_catalog:e{epoch} + fmri_lookup:e{epoch}
- fmri_offsets.v1 → fmri_offsets:e{epoch} (optional, can be inlined initially)
- lock → external file lock or locks table; writer serialization via single-writer discipline

This plan keeps feature parity with pkg5 while modernizing the storage and concurrency model using redb. It preserves all key capabilities and exposes a clear path for incremental adoption and migration.

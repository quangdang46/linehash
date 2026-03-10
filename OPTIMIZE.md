# Context
The user now wants the code optimized as aggressively as practical using better algorithms and data structures, not just benchmarked. Current measurements show the main hot path is `compute_stats()` in `crates/core/document.rs`, especially on collision-heavy inputs, while `verify` also pays unnecessary overhead from string-based short hashes and a general-purpose hash map index. The current implementation models a 2-hex-digit short hash as an owned `String` and stores indices in `HashMap<String, Vec<usize>>`, which is over-generalized for a fixed 256-value domain and creates avoidable allocation, cloning, hashing, and comparison cost across load, stats, and verify.

# Recommended approach
1. **Change the internal short-hash representation from `String` to a numeric byte.**
   - In `crates/core/hash.rs`, introduce a numeric helper for the short hash (`u8` from `full_hash & 0xff`) and keep string formatting only for CLI/output boundaries.
   - In `crates/core/document.rs`, change `LineRecord.short_hash` to `u8` while keeping `full_hash: u32`.
   - Preserve exact external behavior by formatting back to the same 2-char lowercase hex string only in output, JSON serialization, and error rendering.

2. **Replace the current hash-map index with a fixed 256-bucket index.**
   - Refactor `Document::build_index()` in `crates/core/document.rs:105` to return a direct bucket structure such as `Vec<Vec<usize>>` of length 256 instead of `HashMap<String, Vec<usize>>`.
   - This removes map hashing and key allocation entirely and matches the true short-hash domain.
   - Update all resolution paths to use direct bucket lookup.

3. **Refactor `compute_stats()` to work from the bucket index and reduce redundant work.**
   - Keep one document scan for line-derived aggregates, and one bucket scan for collision-derived aggregates.
   - Compute `unique_hashes`, `collision_count`, and `collision_pairs` directly from buckets.
   - Attempt to remove `collision_pairs.sort_unstable()` if bucket traversal plus pair generation already yields deterministic order matching current behavior.
   - If sort removal would change externally observed ordering, preserve ordering correctness first and optimize surrounding overhead instead.

4. **Move anchor parsing and resolution to numeric short hashes.**
   - In `crates/core/anchor.rs`, parse the 2-char hex anchor into `u8` instead of storing a normalized `String` internally.
   - Update `Anchor`, `ResolvedLine`, and `resolve` / `resolve_range` / `resolve_all` to use the new bucket index.
   - Keep error messages and user-visible anchor formatting identical by converting numeric short hashes to strings only when rendering messages.

5. **Let `verify` benefit from the new representation without changing its UX.**
   - In `crates/core/commands/verify.rs`, keep the existing command flow (load once, build index once, resolve many anchors), but swap in the numeric anchors and fixed-bucket index.
   - Keep string construction lazy and output-facing only.

6. **Treat `watch` as a consistency update, not the primary optimization target.**
   - In `crates/core/commands/watch.rs`, switch internal short-hash comparisons and stored old/new hashes to use the numeric representation internally where practical, but do not spend time redesigning the algorithm since `diff_documents()` is already fast.

# Critical files
- `crates/core/hash.rs` — define numeric short-hash helpers and boundary formatting.
- `crates/core/document.rs` — change `LineRecord`, replace `build_index()`, and refactor `compute_stats()`.
- `crates/core/anchor.rs` — parse and resolve numeric short hashes using the 256-bucket index.
- `crates/core/commands/verify.rs` — reuse the new index and numeric anchor flow.
- `crates/core/commands/watch.rs` — align internal comparisons with the new representation.
- `crates/core/output.rs` — preserve exact 2-char lowercase hex output formatting.
- `crates/core/tests/smoke.rs` and `crates/core/tests/snapshots.rs` — verify no visible behavior changes.
- `crates/core/benches/*.rs` and `bench-results/hash_bench_2026-03-10.md` — validate before/after performance impact.

# Verification
- Run the full crate test suite first to catch any behavior drift in parsing, resolution, stats values, and output formatting.
- Run smoke and snapshot tests specifically to confirm that all CLI-visible hashes, JSON payloads, and error messages still use the same 2-char lowercase hex strings.
- Re-run the performance benchmarks, focusing on:
  - `stats_collision_heavy_10k`
  - `stats_10k_lines`
  - `verify_100_anchors`
  - `verify_mixed_100_anchors`
  - `hash_10k_lines`
- Compare the updated benchmark results against the current report in `bench-results/hash_bench_2026-03-10.md` to confirm that the optimization materially improves the real hotspots.
- If collision-pair ordering changes during optimization, either restore the old deterministic order or update tests only after confirming the new order is still stable and acceptable.

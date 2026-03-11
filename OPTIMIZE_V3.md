# Context
The user now wants a concrete optimization plan for linehash edit performance and then a benchmark rerun to verify impact. Current benchmarks show that linehash is paying for multiple whole-document passes during a single edit: document construction, index building, full metadata rebuild after mutation, and full rendering. The latest edit benchmark snapshot indicates that at 100k lines the cost is split roughly between the resolve side and the mutate/render side, so optimizing only one area will not be enough.

# Recommended approach
Implement a focused optimization pass on the current edit path, then rerun the existing benchmark suite and regenerate the markdown report.

The highest-value path is:
1. make mutation metadata updates incremental
2. make rendering cheaper
3. pre-size the short-hash index
4. avoid unnecessary full-document rendering/receipt work in the command path

## Why this shape
- The biggest benchmark costs are caused by repeated whole-document work, not by the anchor-parse logic itself.
- `replace_line()` currently triggers full-document rehashing/renumbering even for a one-line change.
- `render()` currently builds intermediate allocations and copies the whole document through a less efficient path.
- `build_index()` is rebuilt every time without pre-sizing buckets, even though the codebase already uses a better pre-count/pre-size pattern elsewhere.
- `commands/edit.rs` eagerly renders `before_bytes` and builds receipts even when the user did not ask for receipt/audit output.

# Files to modify
- `crates/core/mutation.rs` — replace whole-document metadata rebuilds with targeted updates per operation
- `crates/core/document.rs` — optimize `render()` and pre-size `build_index()`
- `crates/core/commands/edit.rs` — lazily compute before/after bytes and receipts only when needed
- `crates/core/benches/edit_bench.rs` — rerun existing benchmarks; optionally add finer-grained attribution benches only if needed after the first optimization pass
- `scripts/render_edit_bench_report.py` — reuse unchanged to regenerate the markdown report after rerun
- `bench-results/edit_bench.md` — refresh with new benchmark numbers after optimization

# Existing code to reuse
- `crates/core/mutation.rs` — current mutation semantics and tests; preserve behavior while narrowing metadata updates
- `crates/core/document.rs` — existing `count_short_hashes()` / `build_index_from_counts()` pattern can be reused for a better `build_index()`
- `crates/core/commands/edit.rs` — current command flow and receipt behavior; optimize conditionally without changing output semantics
- `crates/core/benches/edit_bench.rs` — current benchmark matrix already covers the scenarios needed to validate the optimization pass
- `scripts/render_edit_bench_report.py` — current report generator for regenerating markdown after rerun

# Implementation steps
1. Optimize `crates/core/mutation.rs` first.
   - Replace `rebuild_line_metadata(doc)` with operation-specific updates.
   - For `replace_line`, recompute hash only for the edited line and do not renumber anything.
   - For `swap_lines`, swap records and fix only the affected `number` fields; do not rehash unchanged content.
   - For `insert_line`, `delete_line`, `move_line`, and `replace_range_with_line`, renumber only the affected suffix/range and only hash newly inserted or edited lines.
   - Preserve all existing mutation semantics and newline/trailing-newline behavior.

2. Optimize `crates/core/document.rs`.
   - Rewrite `render()` to build bytes directly into one preallocated buffer instead of collecting line slices and using `join()`.
   - Update `build_index()` to reuse the existing count/pre-size pattern so the 256 short-hash buckets are allocated with useful capacity before pushing line indices.

3. Optimize `crates/core/commands/edit.rs`.
   - Keep the current command semantics, but introduce a `need_receipt` gate.
   - Only compute `before_bytes`, `after_bytes`, and receipt data when `cmd.receipt || cmd.audit_log.is_some()`.
   - Preserve the current dry-run early return so unnecessary full-document render work is skipped there.
   - Keep output and audit behavior unchanged when receipt/audit is actually requested.

4. Run correctness checks.
   - Run targeted tests for mutation/document/edit behavior first.
   - If clean, run the full crate test suite.

5. Rerun the benchmark suite and refresh the report.
   - Run `cargo bench -p linehash --bench edit_bench`
   - Run `python3 scripts/render_edit_bench_report.py`
   - Review the new `bench-results/edit_bench.md` and the new timestamped snapshot

# Expected benchmark impact
Focus on these comparisons before/after optimization:
- `edit_mutate_render_linehash_10k_single_line`
- `edit_mutate_render_linehash_100k_single_line`
- `edit_resolve_anchor_10k_exact_match`
- `edit_resolve_anchor_100k_exact_match`
- `edit_linehash_single_edit_10k_exact_match`
- `edit_linehash_single_edit_100k_exact_match`
- `edit_linehash_single_edit_10k_long_lines_exact_match`

Expected directional outcome:
- biggest gains in mutate/render-heavy benches after incremental metadata updates and direct-buffer rendering
- moderate gains in resolve-heavy benches after pre-sized index building
- end-to-end exact-match edits should improve at both 10k and 100k, with larger absolute gain at 100k
- command-layer lazy receipt work may help real CLI usage more than the current in-memory edit benchmarks, but it is still worth fixing now because it is low-risk and improves the actual product path

# Verification
Run:
- `cargo test -p linehash mutation`
- `cargo test -p linehash document`
- `cargo test -p linehash`
- `cargo bench -p linehash --bench edit_bench`
- `python3 scripts/render_edit_bench_report.py`

Expected outcome:
- all existing tests remain green
- no correctness regressions in exact-match, drift, duplicate-target, or line-shift scenarios
- `edit_mutate_render_linehash_100k_single_line` and `edit_linehash_single_edit_100k_exact_match` improve materially from the latest baseline
- the regenerated markdown report shows the new numbers clearly and can be compared directly against the previous snapshot
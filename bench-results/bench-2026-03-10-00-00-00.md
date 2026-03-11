# comprehensive benchmark results

Date: 2026-03-10
Build: current working tree after stats-index preallocation change
Command: `cargo bench --manifest-path crates/core/Cargo.toml --bench hash_bench --bench stats_bench --bench verify_bench --bench watch_bench`

## Current performance state

### Hash / load
- `hash_1k_lines`: 189.90 µs – 206.22 µs
- `hash_10k_lines`: 1.9354 ms – 2.5752 ms
- `hash_10k_long_lines`: 3.5639 ms – 3.7641 ms

### Stats
- `stats_1k_lines`: 104.70 µs – 111.54 µs
- `stats_10k_lines`: 7.1213 ms – 7.4981 ms
- `stats_collision_heavy_10k`: 38.365 ms – 40.207 ms

### Verify
- `verify_10_anchors`: 132.39 µs – 142.94 µs
- `verify_100_anchors`: 136.71 µs – 144.27 µs
- `verify_mixed_100_anchors`: 260.14 µs – 306.66 µs

### Watch diff
- `watch_diff_no_changes_10k`: 16.880 µs – 18.057 µs
- `watch_diff_single_change_10k`: 19.819 µs – 21.498 µs
- `watch_diff_append_100_lines_10k`: 31.722 µs – 32.528 µs

## Plan targets

From `PLAN.md`:
- 1k-line load-only hash target: `<1 ms` target, `<2 ms` acceptable
- 10k-line load-only hash target: `<5 ms` target, `<10 ms` acceptable
- `watch` should detect a save within 500 ms, though this benchmark intentionally validates only deterministic diff work, not OS notification latency

## Assessment

### Hash / load
- `hash_1k_lines`: still comfortably under the plan threshold, but materially slower than earlier optimized attempts.
- `hash_10k_lines`: remains comfortably under the `<5 ms` target across the measured range.
- `hash_10k_long_lines`: improved substantially in this run and remains a useful stress case beyond the plan threshold.

### Adjacent hot paths
- `stats_collision_heavy_10k` improved meaningfully after the retained `compute_stats()` change that pre-counts short-hash buckets and builds a pre-sized index.
- `stats_10k_lines` also improved and now sits in the low 7 ms range in the reconfirmation run.
- `stats_1k_lines` is effectively flat in the reconfirmation run, so the retained optimization should be understood primarily as a 10k/collision-heavy improvement.
- `verify` is comfortably sub-millisecond even for the 100-anchor batch in this benchmark setup, reflecting fast deterministic resolution on a reused document/index.
- `watch` diff computation remains comfortably sub-millisecond for the benchmarked 10k-line cases, so deterministic recomputation is not the bottleneck behind the user-facing 500 ms watch target.

## Notes

- The benchmark suite covers four deterministic layers: hashing/load, stats analysis, batched verify resolution, and watch diff computation.
- Hash/load benchmarks measure `Document::from_str(...)` on generated in-memory input and intentionally exclude file I/O and process startup.
- Verify benchmarks measure anchor parsing and resolution against one prebuilt document/index, matching the command's core hot path rather than CLI overhead.
- Watch benchmarks intentionally exclude filesystem event latency and benchmark only `diff_documents(...)`.
- This report reflects the current code after reverting parser experiments and reverting the last collision-pair micro-change, while keeping the measured `compute_stats()` improvement that pre-counts buckets and pre-sizes the short-hash index.
- The latest reconfirmation run was focused on `stats_bench` after the micro-change revert; it confirmed retained gains for `stats_10k_lines` and `stats_collision_heavy_10k`, with no significant change for `stats_1k_lines`.
- Coarse CLI-envelope regression tests remain gated behind `LINEHASH_RUN_PERF=1` so normal `cargo test` stays stable.

# comprehensive benchmark results

Date: 2026-03-10
Bead: `linehash-39q.8.4`
Command: `cargo bench --manifest-path crates/core/Cargo.toml --bench hash_bench --bench stats_bench --bench verify_bench --bench watch_bench`

## Current performance state

### Hash / load
- `hash_1k_lines`: 131.76 µs – 142.35 µs
- `hash_10k_lines`: 1.7847 ms – 2.0478 ms
- `hash_10k_long_lines`: 4.8957 ms – 5.7040 ms

### Stats
- `stats_1k_lines`: 228.96 µs – 277.03 µs
- `stats_10k_lines`: 8.2275 ms – 8.5874 ms
- `stats_collision_heavy_10k`: 40.649 ms – 44.259 ms

### Verify
- `verify_10_anchors`: 128.77 µs – 139.10 µs
- `verify_100_anchors`: 178.98 µs – 208.81 µs
- `verify_mixed_100_anchors`: 402.72 µs – 606.10 µs

### Watch diff
- `watch_diff_no_changes_10k`: 22.161 µs – 26.411 µs
- `watch_diff_single_change_10k`: 26.158 µs – 30.405 µs
- `watch_diff_append_100_lines_10k`: 46.095 µs – 52.590 µs

## Plan targets

From `PLAN.md`:
- 1k-line load-only hash target: `<1 ms` target, `<2 ms` acceptable
- 10k-line load-only hash target: `<5 ms` target, `<10 ms` acceptable
- `watch` should detect a save within 500 ms, though this benchmark intentionally validates only deterministic diff work, not OS notification latency

## Assessment

### Hash / load
- `hash_1k_lines`: comfortably passes target
- `hash_10k_lines`: now comfortably under the `<5 ms` target across the measured range
- `hash_10k_long_lines`: remains a useful stress case; slightly slower than the short-line 10k case and should still be treated as supplemental coverage rather than a plan threshold

### Adjacent hot paths
- `stats` remains more expensive than load-only hashing, especially on collision-heavy input, which is expected because it builds an index, detects collisions, estimates tokens, and computes guidance.
- `verify` is now comfortably sub-millisecond even for the 100-anchor batch in this benchmark setup, reflecting fast deterministic resolution on a reused document/index.
- `watch` diff computation remains comfortably sub-millisecond for the benchmarked 10k-line cases, so deterministic recomputation is not the bottleneck behind the user-facing 500 ms watch target.

## Notes

- The benchmark suite covers four deterministic layers: hashing/load, stats analysis, batched verify resolution, and watch diff computation.
- Hash/load benchmarks measure `Document::from_str(...)` on generated in-memory input and intentionally exclude file I/O and process startup.
- Verify benchmarks measure anchor parsing and resolution against one prebuilt document/index, matching the command's core hot path rather than CLI overhead.
- Watch benchmarks intentionally exclude filesystem event latency and benchmark only `diff_documents(...)`.
- In this run, Criterion reported improvements for nearly all tracked benchmarks relative to the prior saved baseline; `hash_10k_long_lines` was reported as no significant change.
- Coarse CLI-envelope regression tests were added separately and are gated behind `LINEHASH_RUN_PERF=1` so normal `cargo test` remains stable.

# comprehensive benchmark results

Date: 2026-03-10
Bead: `linehash-39q.8.4`
Command: `cargo bench --manifest-path crates/core/Cargo.toml --bench hash_bench --bench stats_bench --bench verify_bench --bench watch_bench`

## Current performance state

### Hash / load
- `hash_1k_lines`: 303.82 µs – 391.22 µs
- `hash_10k_lines`: 3.9189 ms – 5.0265 ms
- `hash_10k_long_lines`: 5.1561 ms – 6.0097 ms

### Stats
- `stats_1k_lines`: 239.23 µs – 259.99 µs
- `stats_10k_lines`: 15.828 ms – 19.747 ms
- `stats_collision_heavy_10k`: 73.548 ms – 87.714 ms

### Verify
- `verify_10_anchors`: 1.4158 ms – 1.7907 ms
- `verify_100_anchors`: 784.99 µs – 849.79 µs
- `verify_mixed_100_anchors`: 1.0145 ms – 1.1483 ms

### Watch diff
- `watch_diff_no_changes_10k`: 96.447 µs – 115.03 µs
- `watch_diff_single_change_10k`: 158.31 µs – 248.45 µs
- `watch_diff_append_100_lines_10k`: 152.14 µs – 218.58 µs

## Plan targets

From `PLAN.md`:
- 1k-line load-only hash target: `<1 ms` target, `<2 ms` acceptable
- 10k-line load-only hash target: `<5 ms` target, `<10 ms` acceptable
- `watch` should detect a save within 500 ms, though this benchmark intentionally validates only deterministic diff work, not OS notification latency

## Assessment

### Hash / load
- `hash_1k_lines`: passes target
- `hash_10k_lines`: near the 5 ms target ceiling at the upper end, but still within the acceptable envelope
- `hash_10k_long_lines`: useful stress case; slower than the documented short-line 10k case and should be treated as supplemental coverage rather than a plan threshold

### Adjacent hot paths
- `stats` is substantially more expensive than load-only hashing, especially on collision-heavy input, which is expected because it builds an index, detects collisions, estimates tokens, and computes guidance.
- `verify` stays sub-2 ms for the benchmarked batches because it reuses a single document/index and measures deterministic resolution work rather than process startup.
- `watch` diff computation is comfortably sub-millisecond for the benchmarked 10k-line cases, so deterministic recomputation is not the bottleneck behind the user-facing 500 ms watch target.

## Notes

- The benchmark suite now covers four deterministic layers: hashing/load, stats analysis, batched verify resolution, and watch diff computation.
- Hash/load benchmarks still measure `Document::from_str(...)` on generated in-memory input and intentionally exclude file I/O and process startup.
- Verify benchmarks measure anchor parsing and resolution against one prebuilt document/index, matching the command's core hot path rather than CLI overhead.
- Watch benchmarks intentionally exclude filesystem event latency and benchmark only `diff_documents(...)`.
- Criterion reported regressions for the existing hash benchmarks relative to the prior saved baseline. That indicates the new run is slower than the earlier benchmark history on this machine/session baseline, not that the implementation is functionally incorrect.
- Coarse CLI-envelope regression tests were added separately and are gated behind `LINEHASH_RUN_PERF=1` so normal `cargo test` remains stable.

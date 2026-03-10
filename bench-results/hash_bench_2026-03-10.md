# hash benchmark results

Date: 2026-03-10
Bead: `linehash-39q.8.4`
Command: `cargo bench --manifest-path crates/core/Cargo.toml --bench hash_bench`

## Results

- `hash_1k_lines`: 156.39 µs – 161.35 µs
- `hash_10k_lines`: 1.9271 ms – 1.9526 ms

## Plan targets

From `PLAN.md`:
- 1k-line load-only hash target: `<1 ms` target, `<2 ms` acceptable
- 10k-line load-only hash target: `<5 ms` target, `<10 ms` acceptable

## Assessment

Both measured benchmarks are comfortably within the target envelope.

- 1k lines: passes target
- 10k lines: passes target

## Notes

- Benchmark measures `Document::from_str(...)` on generated in-memory input, matching the plan's load-only hashing path.
- This intentionally excludes file I/O and process startup.
- Criterion reported no statistically significant regression between the initial and rerun measurements during this session.

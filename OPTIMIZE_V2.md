# OPTIMIZE_V2

## Goal
Push `linehash` core paths as far as practical so the remaining bottlenecks are dominated by unavoidable work rather than avoidable allocations, redundant scans, or over-generalized data structures.

This plan is intentionally aggressive: optimize for maximum throughput first, while preserving current CLI behavior, JSON shape, error wording, and deterministic outputs unless a change is explicitly approved.

## Optimization principles
1. **Optimize the true hot paths, not the easiest code to edit.**
2. **Prefer removing whole classes of work** over micro-tweaks.
3. **Keep numeric/internal representations cheap** and format strings only at output boundaries.
4. **Use fast-path / full-path splits** when expensive detail is not always required.
5. **Benchmark every step** so we stop only when gains flatten.

## Current hotspot ranking
Based on current benchmarks and code inspection:

1. `crates/core/document.rs:133` — `compute_stats()`
   - Especially `stats_collision_heavy_10k`
   - Still the clearest high-cost path
   - The measured win so far is a count-first stats path that pre-counts short-hash buckets and builds a pre-sized short-hash index before collision-pair expansion
2. `crates/core/document.rs:176` + `crates/core/document.rs:213` — document parsing / line building
   - Multiple passes over input remain expensive for hash/load benchmarks
   - A one-pass parser was attempted and then reverted because it did not produce stable net wins across `hash_*` and `stats_*` benches
3. `crates/core/anchor.rs:118` + `crates/core/anchor.rs:148`
   - Already fast, but still paying formatting/allocation overhead in mixed/error paths
4. `crates/core/commands/watch.rs:177`
   - Already very fast; only optimize if cheap and non-invasive

## What we learned from the first benchmarked pass

### Wins worth keeping
- **Stats pre-count + pre-sized index helped.**
  - Pre-counting short-hash buckets into `[usize; 256]`
  - Deriving `unique_hashes` and `collision_count` from those counts
  - Building a pre-sized `ShortHashIndex` from the counts before exact collision-pair generation
- This produced stable improvements in benchmark runs for:
  - `stats_1k_lines`
  - `stats_collision_heavy_10k`
- It kept CLI output, JSON shape, snapshots, and tests unchanged.

### Changes that did not hold up
- **One-pass parser rewrite**
  - Implemented, tested, benchmarked, then reverted.
  - It preserved correctness but did not produce stable net wins across `hash_*` and `stats_*` benchmarks.
  - Conclusion: do not continue broad parser rewrites until profiling identifies a narrower parser bottleneck.
- **Direct-to-buffer `render()` rewrite**
  - Implemented and tested, but did not provide a clear enough overall win in the measured runs when combined with the other document changes.
  - Conclusion: treat `render()` as lower priority than stats unless mutation-heavy benchmarks show it dominating.
- **Marker-gap micro-optimization in `suggest_context_n()`**
  - Safe but too small to matter compared with stats collision work.
  - Conclusion: not a primary lever.

### Updated guidance
- Keep focusing on `compute_stats()` and especially `collect_collision_pairs()`.
- Prefer benchmark-driven, slice-by-slice changes in stats over large structural rewrites.
- Treat parser and render work as secondary until stats improvements flatten.

## Phase 1 — Make stats brutally fast

### 1. Keep the stats split internal and benchmarked
Current `compute_stats()` still pays for exact collision pair materialization, but the safest path so far has been an **internal** split rather than a product-visible API split.

Recommended shape:
- count short-hash frequencies first
- derive summary metrics from counts
- build a pre-sized short-hash index from those counts
- keep exact `collision_pairs` generation unchanged for compatibility
- keep `compute_stats()` as the public compatibility path unless product behavior changes

Status:
- count-first summary plus pre-sized index construction has already shown real gains
- a full visible `compute_stats_summary()` / `compute_collision_pairs()` split should only happen if the caller surface can change without breaking JSON shape or CLI expectations

### 2. Replace bucket-of-vectors work with count-first fast path
For summary stats, do **not** build or traverse more structure than necessary.

Planned approach:
- count short-hash frequencies into a fixed `[usize; 256]`
- derive `unique_hashes` and `collision_count` from counts directly
- only allocate per-bucket position vectors if exact `collision_pairs` are requested

### 3. Avoid quadratic cost unless exact pair output is required
`collect_collision_pairs()` in `crates/core/document.rs:276` is inherently expensive on collision-heavy inputs.

Plan:
- keep exact pair generation available
- isolate it behind an explicit detailed path
- ensure the common stats path avoids O(k²) pair expansion where product behavior allows

### 4. Remove redundant work inside stats
Review and tighten:
- `collision_pairs.sort_unstable()`
- repeated scans of lines for independent aggregates
- extra temporary vectors in `suggest_context_n()`

Stretch target:
- stream marker-gap analysis without materializing all marker positions if possible

## Phase 2 — Make document parsing near one-pass

### 5. Do not continue full parser rewrites until profiling says they matter
Current parsing still scans input more than once, but a full one-pass parser rewrite was already implemented, benchmarked, and reverted.

Observed result:
- correctness held
- benchmark results did **not** show a stable enough net win across `hash_*` and `stats_*`
- an attempted LF fast path made things substantially worse and was also reverted

Updated plan:
- keep the current parser until profiling identifies a narrower hot sub-path
- if parser work resumes, change one thing at a time and benchmark immediately
- do not combine parser changes with render or stats work in the same measurement step

Constraints remain:
- preserve mixed-newline rejection behavior
- preserve trailing newline handling
- preserve CRLF correctness
- preserve file contents exactly on render round-trip

### 6. Reduce allocation pressure per line
Current line construction still creates one owned `String` per line.

Optimization options to evaluate in order:
1. keep owned `String`, but reduce surrounding temp allocations
2. store original file buffer plus line spans for read-only documents
3. use a hybrid representation:
   - borrowed/span-based for freshly loaded documents
   - owned strings only after mutation

This is the most invasive optimization in the plan, so it should be attempted only after Phase 1 and measured carefully.

### 7. Revisit `render()` cost only with mutation-heavy evidence
Current `render()` joins line strings into a new `String`.

A direct-to-buffer rewrite was attempted and kept only temporarily; it did not emerge as a clear standalone win from the broader benchmark work.

Updated plan:
- treat `render()` as secondary to stats
- only resume `render()` optimization if mutation-heavy profiling or dedicated render/write benchmarks show it is meaningfully hot
- if revisited, benchmark it independently from parser changes

This still matters most for mutation-heavy flows and large outputs.

## Phase 3 — Strip anchor and verify overhead down to the floor

### 8. Keep anchors numeric internally end-to-end
Continue using numeric short hashes as the default internal representation.

Audit for remaining avoidable formatting in:
- `crates/core/anchor.rs:124`
- `crates/core/anchor.rs:155`
- `crates/core/anchor.rs:177`
- `crates/core/anchor.rs:229`

Goal:
- format hex strings only when constructing user-visible output/errors
- never format on success paths unless the API requires it

### 9. Add a zero-allocation success path for resolve
For successful resolution:
- avoid constructing owned strings in `ResolvedLine` if they are not immediately rendered
- consider storing numeric short hash in `ResolvedLine`
- format only in output/error layers

### 10. Make parse fast path byte-based
`parse_anchor()` currently normalizes via trim + lowercase.

Plan:
- parse directly from bytes
- accept uppercase hex without allocating lowercase copies
- only allocate on error paths if the original input is needed for messages

## Phase 4 — Tighten watch and output boundaries

### 11. Keep watch diff internal data numeric until emission
In `crates/core/commands/watch.rs:177`:
- compare numeric hashes only
- store old/new short hashes numerically in intermediate diff structs if practical
- format only when writing JSON or pretty output

This is lower priority because benchmarks already show watch diff is fast.

### 12. Minimize output formatting churn
Audit all hot output paths:
- repeated `format_short_hash(...)`
- temporary `String` creation for pretty printers
- repeated `path.display().to_string()` when not needed

Goal:
- push formatting to the latest possible boundary
- reuse buffers where straightforward

## Phase 5 — Data layout and algorithmic ceiling pass

### 13. Revisit `LineRecord` layout for cache locality
Current fields:
- `number`
- `content`
- `full_hash`
- `short_hash`

Plan:
- measure whether field reordering reduces padding
- consider separating hot metadata from cold content in read-mostly paths
- avoid premature complexity unless benchmarked gains are real

### 14. Measure whether full hash storage is always necessary
If some paths only need short hashes after load:
- consider lazy full-hash computation
- or compute/store only where required

Only do this if profiling shows `full_hash` storage or use is a real cost.

### 15. Re-profile hash primitive choice only after structural wins
Do **not** start by swapping hashing algorithms.

Only after phases 1–4:
- use profiling to verify whether `xxh32` itself is now dominant
- if yes, evaluate alternatives carefully without changing external semantics unexpectedly

## Verification plan
Run after each phase, not only at the end.

### Correctness
- `cargo test --manifest-path crates/core/Cargo.toml`
- smoke tests
- snapshot tests
- targeted tests for:
  - mixed newlines
  - CRLF round-trip
  - trailing newline preservation
  - stale / ambiguous anchor errors
  - stats collision ordering behavior

### Performance
Primary benchmarks to watch:
- `hash_1k_lines`
- `hash_10k_lines`
- `hash_10k_long_lines`
- `stats_1k_lines`
- `stats_10k_lines`
- `stats_collision_heavy_10k`
- `verify_10_anchors`
- `verify_100_anchors`
- `verify_mixed_100_anchors`
- `watch_diff_no_changes_10k`
- `watch_diff_single_change_10k`
- `watch_diff_append_100_lines_10k`

### Profiling tools
Use before and after major structural changes:
- `cargo bench`
- `cargo test` perf envelope checks
- Linux `perf` if available
- `cargo flamegraph` if available

## Success criteria

### Must-have
- No CLI-visible regressions
- No JSON shape regressions
- No snapshot drift unless intentional and reviewed
- No correctness regressions in edge cases

### Performance targets
Aggressive target band:
- `hash_1k_lines`: approach low hundreds of µs consistently
- `hash_10k_lines`: stay near the low single-digit ms band, ideally closer to ~1 ms than ~3 ms
- `stats_10k_lines`: drive toward low single-digit ms
- `stats_collision_heavy_10k`: reduce as far as product requirements allow; summary path should avoid collision-pair explosion cost
- `verify_100_anchors`: keep comfortably sub-millisecond
- `watch_diff_*`: remain tens of µs to low hundreds of µs

## Stop conditions
We stop optimizing only when at least one of these becomes true:
1. Profiling shows remaining time is dominated by unavoidable work
2. Further gains require unacceptable complexity or UX changes
3. Benchmarks flatten across multiple structural improvements
4. The remaining cost is mostly output formatting / OS / I/O boundary work

## Recommended implementation order
1. Keep improving stats with count-first helpers and pre-sized data structures
2. Focus specifically on collision-pair generation cost in `collect_collision_pairs()`
3. Re-profile before touching parser or render again
4. Zero-allocation anchor parse/resolve success path
5. Watch/output cleanup
6. Parser work only if profiling later justifies it
7. Render work only if mutation-heavy profiling later justifies it
8. Data-layout / final profiling pass

## Expected outcome
If executed well, this plan should push the core much closer to its practical ceiling:
- stats becomes cheaper first, especially on collision-heavy inputs
- verify becomes close to “formatting-bound” rather than “logic-bound”
- watch remains fast enough that filesystem latency, not diff logic, is the dominant real-world factor
- parser/render work is only resumed if later profiling proves there is still worthwhile headroom there

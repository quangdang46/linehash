# edit benchmark results

Date: 2026-03-11 22:32:09
Commit: `4c69a9e`
Build: current working tree
Command: `cargo bench -p linehash --bench edit_bench`

## Current performance state

### End-to-end exact match
- `edit_linehash_single_edit_1k_exact_match`: 146.82 µs – 152.77 µs
- `edit_naive_str_replace_single_edit_1k_exact_match`: 9.48 µs – 9.81 µs
- `edit_linehash_single_edit_10k_exact_match`: 1.6554 ms – 1.9031 ms
- `edit_naive_str_replace_single_edit_10k_exact_match`: 603.65 µs – 633.96 µs
- `edit_linehash_single_edit_100k_exact_match`: 25.7132 ms – 27.3099 ms
- `edit_naive_str_replace_single_edit_100k_exact_match`: 9.9147 ms – 10.4757 ms
- `edit_linehash_single_edit_10k_long_lines_exact_match`: 5.6543 ms – 6.0300 ms
- `edit_naive_str_replace_single_edit_10k_long_lines_exact_match`: 1.3810 ms – 1.5357 ms

### Robustness scenarios
- `edit_linehash_single_edit_10k_whitespace_drift`: 1.9376 ms – 2.0636 ms
- `edit_naive_str_replace_single_edit_10k_whitespace_drift`: 73.04 µs – 76.58 µs
- `edit_linehash_single_edit_10k_target_whitespace_drift`: 1.5018 ms – 1.5644 ms
- `edit_naive_str_replace_single_edit_10k_target_whitespace_drift`: 65.98 µs – 77.49 µs
- `edit_linehash_single_edit_10k_duplicate_target`: 1.6124 ms – 1.6781 ms
- `edit_naive_str_replace_single_edit_10k_duplicate_target`: 147.94 µs – 154.58 µs
- `edit_linehash_single_edit_10k_line_shift_drift`: 1.5200 ms – 1.6105 ms
- `edit_naive_str_replace_single_edit_10k_line_shift_drift`: 110.93 µs – 116.81 µs

### Phase breakdown
- `edit_resolve_anchor_10k_exact_match`: 1.4724 ms – 1.6015 ms
- `edit_resolve_anchor_100k_exact_match`: 17.0456 ms – 18.3142 ms
- `edit_mutate_render_linehash_10k_single_line`: 1.5162 ms – 1.6209 ms
- `edit_mutate_render_linehash_100k_single_line`: 19.9164 ms – 21.5416 ms
- `edit_replace_naive_line_10k_exact_match`: 115.61 µs – 127.42 µs

## Scenario notes

- `edit_linehash_single_edit_1k_exact_match`: linehash end-to-end exact-match edit on 1k short lines
- `edit_naive_str_replace_single_edit_1k_exact_match`: naive exact-line replace on 1k short lines
- `edit_linehash_single_edit_10k_exact_match`: linehash end-to-end exact-match edit on 10k short lines
- `edit_naive_str_replace_single_edit_10k_exact_match`: naive exact-line replace on 10k short lines
- `edit_linehash_single_edit_100k_exact_match`: linehash end-to-end exact-match edit on 100k short lines
- `edit_naive_str_replace_single_edit_100k_exact_match`: naive exact-line replace on 100k short lines
- `edit_linehash_single_edit_10k_long_lines_exact_match`: linehash end-to-end exact-match edit on 10k long lines
- `edit_naive_str_replace_single_edit_10k_long_lines_exact_match`: naive exact-line replace on 10k long lines
- `edit_linehash_single_edit_10k_whitespace_drift`: surrounding-context drift: linehash still succeeds
- `edit_naive_str_replace_single_edit_10k_whitespace_drift`: stale exact block replacement under surrounding-context drift
- `edit_linehash_single_edit_10k_target_whitespace_drift`: target-line drift: linehash fails stale
- `edit_naive_str_replace_single_edit_10k_target_whitespace_drift`: target-line drift: naive exact-line replacement fails
- `edit_linehash_single_edit_10k_duplicate_target`: duplicate target text: linehash edits intended occurrence
- `edit_naive_str_replace_single_edit_10k_duplicate_target`: duplicate target text: naive edits first occurrence only
- `edit_linehash_single_edit_10k_line_shift_drift`: line inserted above target: linehash anchor becomes stale
- `edit_naive_str_replace_single_edit_10k_line_shift_drift`: line inserted above target: naive still finds matching text
- `edit_resolve_anchor_10k_exact_match`: anchor resolution only on 10k exact-match fixture
- `edit_resolve_anchor_100k_exact_match`: anchor resolution only on 100k exact-match fixture
- `edit_mutate_render_linehash_10k_single_line`: linehash mutation plus render on 10k exact-match fixture
- `edit_mutate_render_linehash_100k_single_line`: linehash mutation plus render on 100k exact-match fixture
- `edit_replace_naive_line_10k_exact_match`: naive exact-line replace only on 10k exact-match fixture

## Assessment

- Exact-match benchmarks show the raw throughput gap between linehash end-to-end edits and naive exact-line replacement at 1k, 10k, and 100k lines.
- Long-line exact-match benchmarks show how wider line content changes the tradeoff for both strategies.
- Robustness scenarios intentionally separate stale surrounding context, stale target content, duplicate target text, and line-shift drift so the report does not collapse correctness and speed into one misleading number.
- Phase-breakdown benchmarks help explain whether large-file cost on the linehash side is concentrated in resolution or in mutation+render work.

## Update instructions

1. Run `cargo bench -p linehash --bench edit_bench`.
2. Run `python3 scripts/render_edit_bench_report.py`.
3. Review the diff in `bench-results/edit_bench.md`.
4. If you want a dated snapshot, keep the generated `bench-results/bench-YYYY-MM-DD-HH-MM-SS.md` file too.


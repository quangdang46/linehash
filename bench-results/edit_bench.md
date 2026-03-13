# edit benchmark results

Date: 2026-03-11 23:13:40
Commit: `e6ec724`
Build: current working tree
Command: `cargo bench -p linehash --bench edit_bench`

## Current performance state

### End-to-end exact match
- `edit_linehash_single_edit_1k_exact_match`: 126.85 µs – 128.02 µs
- `edit_naive_str_replace_single_edit_1k_exact_match`: 8.41 µs – 8.62 µs
- `edit_linehash_single_edit_10k_exact_match`: 1.4142 ms – 1.4408 ms
- `edit_naive_str_replace_single_edit_10k_exact_match`: 443.13 µs – 448.65 µs
- `edit_linehash_single_edit_100k_exact_match`: 16.8598 ms – 17.3102 ms
- `edit_naive_str_replace_single_edit_100k_exact_match`: 6.6704 ms – 6.9663 ms
- `edit_linehash_single_edit_10k_long_lines_exact_match`: 3.0851 ms – 3.1529 ms
- `edit_naive_str_replace_single_edit_10k_long_lines_exact_match`: 749.04 µs – 781.00 µs

### Robustness scenarios
- `edit_linehash_single_edit_10k_whitespace_drift`: 1.4451 ms – 1.4948 ms
- `edit_naive_str_replace_single_edit_10k_whitespace_drift`: 66.24 µs – 68.73 µs
- `edit_linehash_single_edit_10k_target_whitespace_drift`: 1.5437 ms – 1.7076 ms
- `edit_naive_str_replace_single_edit_10k_target_whitespace_drift`: 46.37 µs – 48.93 µs
- `edit_linehash_single_edit_10k_duplicate_target`: 1.5449 ms – 1.6518 ms
- `edit_naive_str_replace_single_edit_10k_duplicate_target`: 135.45 µs – 142.31 µs
- `edit_linehash_single_edit_10k_line_shift_drift`: 1.4570 ms – 1.5733 ms
- `edit_naive_str_replace_single_edit_10k_line_shift_drift`: 117.85 µs – 120.14 µs

### Phase breakdown
- `edit_resolve_anchor_10k_exact_match`: 1.3908 ms – 1.4315 ms
- `edit_resolve_anchor_100k_exact_match`: 15.8330 ms – 16.1587 ms
- `edit_mutate_render_linehash_10k_single_line`: 1.3627 ms – 1.3816 ms
- `edit_mutate_render_linehash_100k_single_line`: 16.5162 ms – 16.9651 ms
- `edit_replace_naive_line_10k_exact_match`: 95.00 µs – 96.36 µs

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


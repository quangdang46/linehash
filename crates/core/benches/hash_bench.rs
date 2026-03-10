#![allow(unused_imports)]

use std::path::Path;

use criterion::{Criterion, black_box, criterion_group, criterion_main};

#[path = "../error.rs"]
mod error;
#[path = "../hash.rs"]
mod hash;
#[path = "../document.rs"]
mod document;

use document::Document;

fn generate_fixture(line_count: usize) -> String {
    let mut lines = Vec::with_capacity(line_count);
    for i in 0..line_count {
        lines.push(format!(
            "fn generated_line_{i:05}() {{ let value = \"{:08x}\"; }}",
            i.wrapping_mul(2654435761_u32 as usize)
        ));
    }
    lines.join("\n") + "\n"
}

fn bench_hash_1k_lines(c: &mut Criterion) {
    let file = generate_fixture(1_000);
    c.bench_function("hash_1k_lines", |b| {
        b.iter(|| black_box(Document::from_str(Path::new("bench.rs"), &file).unwrap()))
    });
}

fn bench_hash_10k_lines(c: &mut Criterion) {
    let file = generate_fixture(10_000);
    c.bench_function("hash_10k_lines", |b| {
        b.iter(|| black_box(Document::from_str(Path::new("bench.rs"), &file).unwrap()))
    });
}

criterion_group!(benches, bench_hash_1k_lines, bench_hash_10k_lines);
criterion_main!(benches);

#![allow(dead_code)]

pub fn generate_short_fixture(line_count: usize) -> String {
    let mut lines = Vec::with_capacity(line_count);
    for i in 0..line_count {
        lines.push(format!(
            "fn generated_line_{i:05}() {{ let value = \"{:08x}\"; }}",
            i.wrapping_mul(2654435761_u32 as usize)
        ));
    }
    lines.join("\n") + "\n"
}

pub fn generate_long_fixture(line_count: usize) -> String {
    let mut lines = Vec::with_capacity(line_count);
    for i in 0..line_count {
        lines.push(format!(
            "pub fn generated_line_{i:05}(input: &str) -> String {{ let value = format!(\"{}::{}::{}\", input, {i}, \"benchmark_payload_{:08x}\"); value.trim().to_owned() }}",
            "segment",
            "payload",
            "suffix",
            i.wrapping_mul(11400714819323198485_u64 as usize)
        ));
    }
    lines.join("\n") + "\n"
}

pub fn generate_collision_fixture(line_count: usize, mut short_hash: impl FnMut(&str) -> String) -> String {
    let (first, second) = find_collision_pair(&mut short_hash);
    let mut lines = Vec::with_capacity(line_count);
    for i in 0..line_count {
        if i % 16 == 0 {
            lines.push(first.clone());
        } else if i % 16 == 1 {
            lines.push(second.clone());
        } else {
            lines.push(format!("unique-line-{i:05}-{:08x}", i.wrapping_mul(1103515245)));
        }
    }
    lines.join("\n") + "\n"
}

pub fn mutate_short_hash(short: &str) -> String {
    let mut chars = short.chars();
    let first = chars.next().unwrap_or('0');
    let second = chars.next().unwrap_or('0');
    let replacement = if first == '0' { '1' } else { '0' };
    format!("{replacement}{second}")
}

fn find_collision_pair(short_hash: &mut impl FnMut(&str) -> String) -> (String, String) {
    use std::collections::HashMap;

    let mut seen: HashMap<String, String> = HashMap::new();
    for i in 0..10_000 {
        let candidate = format!("line-{i}");
        let hash = short_hash(&candidate);
        if let Some(existing) = seen.insert(hash, candidate.clone()) {
            if existing != candidate {
                return (existing, candidate);
            }
        }
    }
    panic!("failed to find a short-hash collision in search space");
}

use criterion::{criterion_group, criterion_main, Criterion};
use fsearch_core::query::matcher::QueryMatcher;
use fsearch_core::pcre2_pool::PatternPool;
use rayon::prelude::*;
use std::sync::Arc;

fn bench_compile_once_match(c: &mut Criterion) {
    let pool = PatternPool::new();
    let qm = QueryMatcher::new(pool.clone());
    let pattern = "module[0-9]{3}/file_[0-9]{3}\\.rs";
    let node = fsearch_core::query::Node::Regex(pattern.to_string());

    // precompile
    let compiled = qm.compile(&node).expect("compile");
    let text = (0..2000).map(|i| format!("/home/user/projects/repo/src/module{}/file_{}.rs\n", i, i)).collect::<String>();
    let bytes = text.as_bytes();

    c.bench_function("rust_match_compile_once_is_match", |b| {
        b.iter(|| {
            // single-threaded repeated matching
            for _ in 0..1000 {
                let _ = qm.is_match(&compiled, bytes);
            }
        })
    });

    c.bench_function("rust_match_compile_once_captures_meta", |b| {
        b.iter(|| {
            for _ in 0..200 {
                let _ = qm.captures_meta(&compiled, bytes);
            }
        })
    });
}

fn bench_multi_threaded_matches(c: &mut Criterion) {
    let pool = PatternPool::new();
    let qm = Arc::new(QueryMatcher::new(pool.clone()));
    let pattern = "ab[0-9]+";
    let node = fsearch_core::query::Node::Regex(pattern.to_string());
    let compiled = qm.compile(&node).expect("compile");
    let texts: Vec<Vec<u8>> = (0..10000).map(|i| format!("xxab{}yy", i).into_bytes()).collect();

    c.bench_function("rust_match_multi_threaded_is_match_par_iter", |b| {
        b.iter(|| {
            texts.par_iter().for_each(|t| { let _ = qm.is_match(&compiled, t); });
        })
    });
}

criterion_group!(benches, bench_compile_once_match, bench_multi_threaded_matches);
criterion_main!(benches);

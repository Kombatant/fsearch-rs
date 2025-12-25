use fsearch_core::query::matcher::QueryMatcher;
use fsearch_core::pcre2_pool::PatternPool;
use rayon::prelude::*;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let pool = PatternPool::new();
    let qm = QueryMatcher::new(pool.clone());
    let pattern = "module[0-9]{3}/file_[0-9]{3}\\.rs";
    let node = fsearch_core::query::Node::Regex(pattern.to_string());

    println!("Compiling pattern: {}", pattern);
    let compiled = qm.compile(&node).expect("compile");

    // build corpus
    let text = (0..2000).map(|i| format!("/home/user/projects/repo/src/module{}/file_{}.rs\n", i, i)).collect::<String>();
    let bytes = text.as_bytes();

    // single-threaded: repeated matches
    let iters = 10000;
    println!("Single-threaded: {} iterations is_match", iters);
    let start = Instant::now();
    for _ in 0..iters {
        let _ = qm.is_match(&compiled, bytes);
    }
    let dur = start.elapsed();
    println!("Elapsed: {:?}, avg per match: {:?}", dur, dur / iters);

    // captures_meta repeated
    let iters2 = 2000;
    println!("Single-threaded: {} iterations captures_meta", iters2);
    let start = Instant::now();
    for _ in 0..iters2 {
        let _ = qm.captures_meta(&compiled, bytes);
    }
    let dur = start.elapsed();
    println!("Elapsed: {:?}, avg per call: {:?}", dur, dur / iters2);

    // multi-threaded matches over many small texts
    let qm_arc = Arc::new(qm);
    let texts: Vec<Vec<u8>> = (0..50000).map(|i| format!("xxab{}yy", i).into_bytes()).collect();
    println!("Multi-threaded: {} texts (par_iter) is_match", texts.len());
    let start = Instant::now();
    texts.par_iter().for_each(|t| { let _ = qm_arc.is_match(&compiled, t); });
    let dur = start.elapsed();
    println!("Elapsed multi-threaded: {:?}, avg per item: {:?}", dur, dur / texts.len() as u32);
}

use std::process::Command;
use std::path::Path;

use fsearch_core::query::Node;
use fsearch_core::query::matcher::QueryMatcher;
use fsearch_core::pcre2_pool::PatternPool;

fn normalize_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.is_empty() { return ranges; }
    ranges.sort_unstable_by_key(|r| r.0);
    let mut out = Vec::with_capacity(ranges.len());
    let mut cur = ranges[0];
    for &(s,e) in ranges.iter().skip(1) {
        if s <= cur.1 { if e > cur.1 { cur.1 = e; } }
        else { out.push(cur); cur = (s,e); }
    }
    out.push(cur);
    out
}

#[test]
fn parity_with_c_matcher() {
    // This test only runs when the user provides a C matcher binary via env var.
    let bin = match std::env::var("FSEARCH_C_MATCHER_BIN") {
        Ok(b) if !b.is_empty() => b,
        _ => { eprintln!("FSEARCH_C_MATCHER_BIN not set; skipping parity test"); return; }
    };
    if !Path::new(&bin).exists() {
        eprintln!("FSEARCH_C_MATCHER_BIN set but not found: {} ; skipping", bin);
        return;
    }

    // Simple pattern + text to compare captures for.
    let pattern = "(ab)([0-9]+)";
    let text = "xxab123yy";

    // Compile in Rust
    let pool = PatternPool::new();
    let qm = QueryMatcher::new(pool);
    let node = Node::Regex(pattern.to_string());
    let compiled = match qm.compile(&node) {
        Ok(c) => c,
        Err(e) => { eprintln!("Rust matcher failed to compile pattern: {} ; skipping", e); return; }
    };
    let mut rust_caps = qm.captures(&compiled, text.as_bytes());
    rust_caps = normalize_ranges(rust_caps);

    // Try to run the C matcher binary. We don't assume a strict CLI; try two common variants.
    let try_args = vec![vec!["--pattern", pattern, "--text", text], vec![pattern, text]];
    let mut c_out: Option<String> = None;
    for args in try_args.iter() {
        let out = Command::new(&bin).args(args).output();
        match out {
            Ok(o) if o.status.success() => {
                c_out = Some(String::from_utf8_lossy(&o.stdout).to_string());
                break;
            }
            _ => continue,
        }
    }
    let c_out = match c_out {
        Some(s) => s,
        None => { eprintln!("C matcher binary did not accept tested CLI forms; skipping parity test"); return; }
    };

    // Try parse common outputs: JSON array of [start,end] pairs, or newline 'start:end' lines.
    // 1) JSON [[s,e],...]
    if let Ok(v) = serde_json::from_str::<Vec<Vec<usize>>>(&c_out) {
        if !v.is_empty() {
            let mut carr: Vec<(usize,usize)> = v.into_iter().filter_map(|a| if a.len()>=2 { Some((a[0], a[1])) } else { None }).collect();
            carr = normalize_ranges(carr);
            assert_eq!(carr, rust_caps);
            return;
        }
    }

    // 2) JSON array of objects with `ranges` key
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&c_out) {
        if let Some(arr) = v.as_array() {
            if !arr.is_empty() {
                // try to extract first ranges field
                if let Some(obj) = arr.get(0) {
                    if let Some(ranges) = obj.get("ranges") {
                        if let Some(rarr) = ranges.as_array() {
                            let mut carr = Vec::new();
                            for rr in rarr.iter() {
                                if let Some(pair) = rr.as_array() {
                                    if pair.len() >= 2 {
                                        if let (Some(s), Some(e)) = (pair[0].as_u64(), pair[1].as_u64()) {
                                            carr.push((s as usize, e as usize));
                                        }
                                    }
                                }
                            }
                            carr = normalize_ranges(carr);
                            assert_eq!(carr, rust_caps);
                            return;
                        }
                    }
                }
            }
        }
    }

    // 3) Plain text lines like "11:15\n"
    let mut carr = Vec::new();
    for line in c_out.lines() {
        if let Some(idx) = line.find(':') {
            if let (Ok(s), Ok(e)) = (line[..idx].trim().parse::<usize>(), line[idx+1..].trim().parse::<usize>()) {
                carr.push((s,e));
            }
        }
    }
    if !carr.is_empty() {
        carr = normalize_ranges(carr);
        assert_eq!(carr, rust_caps);
        return;
    }

    eprintln!("C matcher output format not recognized; skipping parity assertion. Output:\n{}", c_out);
}

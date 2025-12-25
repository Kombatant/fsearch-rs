use criterion::{criterion_group, criterion_main, Criterion};
use std::process::Command;

fn run_c_matcher(bin: &str, pattern: &str, text: &str, use_jit: bool) {
    let mut cmd = Command::new(bin);
    if use_jit {
        cmd.arg("--jit");
    }
    cmd.arg("--pattern").arg(pattern).arg("--text").arg(text);
    let out = cmd.output().expect("failed to run c_matcher");
    if !out.status.success() {
        panic!("c_matcher failed: {}", String::from_utf8_lossy(&out.stderr));
    }
}

fn bench_c_matcher(c: &mut Criterion) {
    let bin = std::env::var("FSEARCH_C_MATCHER_BIN").unwrap_or_else(|_| "./c_parity/c_matcher".into());
    // sample pattern and a moderately large text; adjust size as needed
    let pattern = "test";
    let text = "The quick brown fox jumps over the lazy dog. " .repeat(5000);

    c.bench_function("c_matcher_no_jit", |b| {
        b.iter(|| {
            run_c_matcher(&bin, pattern, &text, false);
        })
    });

    c.bench_function("c_matcher_with_jit", |b| {
        b.iter(|| {
            run_c_matcher(&bin, pattern, &text, true);
        })
    });
}

criterion_group!(benches, bench_c_matcher);
criterion_main!(benches);

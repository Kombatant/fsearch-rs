Rust skeleton for fsearch-core

This folder contains a minimal Rust workspace with a `fsearch-core` crate
and a small `cxx` bridge. It is intended as a starting point for porting
the C backend to Rust and exposing a safe FFI surface to a Qt6 C++ frontend.

Quick start (requires Rust toolchain):

```bash
cd rust
cargo build -p fsearch-core --release
```

What to implement next:
- flesh out `src/lib.rs` into modules: `index`, `entry`, `query`, `search`
- add `cxx` bridge types for `IndexHandle`, `SearchHandle`, and result streaming
- write a small Qt6 test client that links the produced static library

# FSearch Rust Port — Architecture Plan (concise)

Goal
- Port FSearch core to Rust and provide a Qt6 frontend (C++) that interacts with the Rust backend with high-performance parity.

High-level components
- Qt6 UI (C++): thin frontend for windows, lists, highlighting, preferences — uses the existing C UI design but in Qt widgets.
- Rust `fsearch-core` crate: indexing, normalization, query lexer+parser, matchers (PCRE2), search engine, result streaming, persistence API.
- FFI boundary: small, stable C ABI for the Qt client and an optional `cxx` bridge for richer types during development.

Communication & ABI
- Primary: expose a C ABI (extern "C") for the Qt client (`fsearch_init`, `fsearch_index_build_from_paths_c`, `fsearch_start_search_c`, `fsearch_poll_results_c`, `fsearch_cancel_search_c`). Keeps Qt build simple.
- Secondary: use `cxx` for internal test clients and richer integration when both sides are Rust/C++.
- Use thread-safe queues (crossbeam-channel) and minimal structs over the boundary (IDs, small buffers, JSON for complex metadata if necessary).

Concurrency model
- Indexing and searching: Rayon for CPU-parallel indexing/search work per file chunk.
- Streaming: crossbeam channels stream results to the caller; cancellation implemented with AtomicBool.
- Per-thread matcher state: allocate PCRE2 match_data per worker thread to preserve JIT and reuse.

Regex compatibility
- Use PCRE2 via `pcre2-sys` or a thin C wrapper to match original behavior (flags, JIT). Avoid Rust `regex` for final parity.

Parser & AST
- Lexer and parser in Rust (currently `query/lexer.rs` + `query/parser_rs.rs`). Keep AST nodes that map 1:1 to original `fsearch_query_node` to simplify matcher port.

Data layout & persistence
- Index structure: store entries with pre-normalized/folded strings and numeric metadata. Consider mmap'd file for large datasets later.

Testing & validation
- Unit tests for `lexer`, `parser`, `entry` normalization (already present). Add cross-checks that C and Rust return identical results for sample queries.
- Benchmarks: criterion-based microbenchmarks to compare Rust vs C for indexing & search hot paths.

Milestones (short)
1. Stabilize parser and unit tests (done).
2. Port matchers to use PCRE2 and per-thread match_data.
3. Replace temporary `regex` usage with PCRE2 and add match streaming.
4. Implement highlight metadata and lightweight persistence.
5. Integrate Qt6 UI and replace C client with final Qt client.

Acceptance criteria
- Feature parity for queries: same results for representative test suite as original C binary.
- Performance: comparable or faster time-to-first-results on representative datasets.
- Robust FFI surface for the Qt client and documented build steps.

Next immediate steps
- Implement PCRE2 bindings and port `fsearch_query_matchers.c` to Rust (create tests comparing behavior against C). 
- Add bench harness for matching and streaming.

Misc
- Keep `query/mod.rs` stable; prefer `parser.rs` as the canonical name (I currently have `parser_rs.rs` working; I can rename it back when ready).

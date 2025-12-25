# FSearch Rust/Qt6 Architecture Plan

Goal
----
Port the FSearch frontend to Qt6 and backend to Rust while preserving feature parity,
streaming low-latency search results to the UI, and providing Unicode-correct highlighting.

High level components
---------------------
- Qt6 UI (Rust-provided staticlib or C ABI): main GUI, search box, results list, item rendering.
- Rust backend `fsearch-core`: modules:
  - `entry` / `index`: entries, normalized fields, index build/serialize
  - `query` (lexer, parser, AST): produces `Node` AST for expressions, fields, modifiers
  - `matcher` / `match_engine`: compile AST to `CompiledNode` (PCRE2-backed) and match routines
  - `pcre2_pool` + `pcre2_backend`: per-thread compiled pattern cache and safe access
  - `search`: orchestrates indexing, search workers, and event-driven result delivery
  - `ffi` (C bridge): small C ABI for Qt client (index build, start/cancel search, callbacks)

Communication patterns
----------------------
- Primary: C ABI callbacks invoked by Rust worker threads -> client must marshal to GUI thread.
- Highlights: Rust produces JSON with UTF-16 code-unit ranges aligned to grapheme clusters.
- Event-driven: `fsearch_start_search_with_cb_c(query, cb, userdata)` spawns worker and calls `cb` per result.

Data flow
---------
1. UI requests index build -> `fsearch_index_build_from_paths_c(...)` -> Rust builds `Index` snapshot.
2. UI starts search -> `fsearch_start_search_with_cb_c(query, cb, userdata)` -> Rust spawns worker.
3. Worker compiles query (AST -> CompiledNode) and matches in parallel; for each match constructs
   highlight metadata (field + UTF-16 ranges) and invokes `cb(id, name, path, size, mtime, highlights_json, userdata)`.
4. UI callback posts to Qt main thread and applies highlights using `QString::mid(start, len)`.

Priorities / Milestones
-----------------------
1. Stable C ABI and event-driven callback delivery (done).
2. Correct grapheme-aware highlights across fields (done: UTF-16 ranges).
3. Complete matcher parity: implement remaining `fsearch_query_matchers.c` semantics in Rust.
4. Performance: ensure PCRE2 JIT where available, PatternPool per-thread caches, and parallel scan.
5. UI polish: custom delegate for selectable, copyable highlighted lines; search-in-progress indicators.

Risks and mitigations
---------------------
- Race conditions on userdata lifecycle: ensure worker threads are cancelled/joined before freeing userdata (mitigated by cancel/join pattern).
- Building PCRE2 on network-mounted drives can fail (os error 26): recommend building with `TMPDIR`/`CARGO_TARGET_DIR` on local tmp and linking system `pcre2-8`.
- Unicode edge-cases: use `unicode-segmentation` and ensure UTF-16 indexing for Qt; add unit tests for ZWJ, flag sequences, combining marks.

API and client guidance
----------------------
- `fsearch_start_search_with_cb_c(query, cb, userdata)` — cb receives `highlights` JSON where each object has `field` (string|null) and `ranges` (array of [start,end] UTF-16 indices).
- For field-scoped queries (e.g., `path:term`) the backend will set `field` to that field name; clients should prefer explicit `field` values.
- Clients must treat `highlights` as transient (owned by Rust until callback returns) and must not free `userdata` until the worker is stopped and joined.

Next actions
------------
- Complete the remaining matcher port and add unit tests for parity with C baseline.
- Add microbenchmarks (Criterion) to measure index + search throughput vs the C implementation.
- Improve the Qt client: visual tests and a small screenshot-based regression test harness.

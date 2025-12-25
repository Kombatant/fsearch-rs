FSearch Migration Roadmap (Rust backend + Qt6 frontend)

Summary
-------
This roadmap breaks the migration into discrete implementation tasks with time estimates, dependencies, risks and acceptance criteria. Estimates assume a developer familiar with the repo and Rust/Qt6.

Guiding principles
- Keep the C ABI stable and minimal.
- Validate Unicode correctness (grapheme-aware, UTF-16 indices for Qt).
- Ensure parity: features, modifiers, captures, and UI highlights.
- Add tests and benchmarks alongside functionality to avoid regressions.

Roadmap tasks
-------------
1) Stabilize build + CI (1-2 days)
   - Actions:
     - Add CI job that builds `rust/fsearch-core` and `rust/qt-client` on Linux (Debug+Release).
     - Ensure PCRE2 is installed in CI or use a prebuilt artifact; set TMPDIR/CARGO_TARGET_DIR where needed.
   - Risks: CI environment differences (pcre2 availability).
   - Acceptance: CI runs `cargo build --release` and `cmake`/`ninja` for Qt client.

2) Complete matcher port (3-5 days)
   - Actions:
     - Port remaining semantics from `fsearch_query_matchers.c`: anchored matching, capture groups, multi-field terms,
       proper grouping, and precedence.
     - Use `pcre2` via `pcre2-sys` + `pcre2` wrapper in `pcre2_pool`.
     - Add unit tests comparing behavior with C baseline for representative queries.
   - Dependencies: pcre2 integration, query parser parity.
   - Acceptance: Unit tests demonstrate matching parity for 20+ representative queries.

3) Field-aware captures & highlight refinement (1 day)
   - Actions:
     - Ensure `captures_meta()` sets `field` for field-scoped queries (e.g., `path:term`).
     - Ensure UTF-16 grapheme-safe ranges for all fields; add unit tests for ZWJ/flag/combining marks per field.
   - Acceptance: Tests pass and Qt client uses explicit `field` when available.

4) Event-driven notifications + lifecycle (1-2 days)
   - Actions:
     - Finalize event-driven API semantics: start, cancel, join semantics; ensure worker threads are joined before freeing userdata.
     - Add integration tests that start a search with a userdata sender and assert no cross-test callback races.
   - Acceptance: No race failures in CI; cancellation properly joins worker thread.

5) Benchmarks & performance tuning (2-4 days)
   - Actions:
     - Add Criterion microbenchmarks for index creation and search throughput (single-threaded & multi-threaded).
     - Measure PCRE2 JIT impact; tune `PatternPool` sizing.
   - Acceptance: Benchmarks run in CI (optional) and document baseline numbers.

6) Qt client polish & visual tests (2-3 days)
   - Actions:
     - Implement a custom item delegate for selectable, copyable highlighted results.
     - Add a small harness that can render and snapshot results for visual regression testing.
   - Acceptance: Visual tests produce stable snapshots; delegating formatting matches expected highlights.

7) Documentation & release prep (1 day)
   - Actions:
     - Update `rust/fsearch-core/include/fsearch_ffi.h` comments and `rust/qt-client/README.md` with API/format notes.
     - Add `ARCHITECTURE_PLAN.md` and `MIGRATION_ROADMAP.md`.
   - Acceptance: Docs updated and reviewed.

Contingency
-----------
- If PCRE2 JIT or system builds are problematic in CI, vendor a small, consistent prebuilt binary or use a container image with required native libs.
- If edge-case Unicode handling proves slow, consider precomputing UTF-16 index maps at index time for faster mapping.

How I can help next
-------------------
- Start implementing Task 2 (matcher port) with incremental PRs and unit tests.
- Add CI config skeleton (GitHub Actions) to build and run tests.


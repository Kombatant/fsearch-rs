# C -> Rust/Qt Mapping (concise)

This file lists the main C source modules in the repository and their corresponding Rust equivalents (or target crates) to help continue the migration.

- `src/fsearch_index.c` -> `rust/fsearch-core/src/index.rs`
- `src/fsearch_database*.c` -> `rust/fsearch-core/src/index.rs` / `rust/fsearch-core/src/search.rs`
- `src/fsearch_database_entry.c` -> `rust/fsearch-core/src/entry.rs`
- `src/fsearch_query_*.c` (lexer, parser, matcher, node, tree) -> `rust/fsearch-core/src/query/*` (`lexer.rs`, `parser.rs`, `matcher.rs`, `mod.rs`)
- `src/fsearch_query_matchers.c` -> `rust/fsearch-core/src/matchers.rs` + `pcre2_backend.rs` + `pcre2_pool.rs`
- `src/fsearch_query_match_data.c` -> `rust/fsearch-core/src/query/matcher.rs` (meta/capture representations)
- `src/fsearch_database_search.c` -> `rust/fsearch-core/src/search.rs`
- `src/fsearch_utf.c`, `src/fsearch_string_utils.c` -> `rust/fsearch-core` unicode handling (using `unicode-normalization`, `unicode-segmentation`, `unicode-segmentation` helpers in `entry.rs` / `search.rs`)
- `src/fsearch_thread_pool.c`, `src/fsearch_memory_pool.c` -> `rayon` / `crossbeam` usage in `rust/fsearch-core` / `pcre2_pool.rs`
- UI (GTK): `src/*.ui`, `src/fsearch_window.c`, `src/fsearch_list_view.c` -> `rust/qt-client/` (Qt6 C++ client)

Notes / Next steps
- Where behavior parity is critical (regex matching, highlighting, UTF-16 index mapping), add focused unit tests in `rust/fsearch-core/tests` (some already present).
- Continue by picking one C module, porting its behavior and tests, then opening an incremental PR on branch `rust/matcher-parity`.

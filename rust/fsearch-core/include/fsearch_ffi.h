// C header for the minimal fsearch-core FFI used by the Qt test client.
#pragma once
// C header for the minimal fsearch-core FFI used by the Qt test client.
#pragma once

// Notes on `highlights` JSON (UTF-8 encoded C string):
// - The `highlights` parameter is a JSON array of objects or an empty string.
// - Each object has the form: { "field": <string|null>, "ranges": [[start,end], ...] }
//   where `field` indicates which textual field the ranges apply to (e.g. "name", "path").
// - `start` and `end` are UTF-16 code-unit indices (half-open: [start,end)) aligned
//   to grapheme cluster boundaries. These indices are safe to apply directly to
//   Qt `QString` via `mid(start, end-start)`.
// - For queries that explicitly target a field (e.g. `path:term`), the `field`
//   value will be set to that field name ("path") for any ranges produced by
//   that term. Clients should prefer the explicit `field` value when present.

#include <stdint.h>
 
#ifdef __cplusplus
extern "C" {
#endif

typedef void (*fsearch_result_cb_t)(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata);

bool fsearch_init(void);

// Build an index from an array of C strings. Returns an opaque pointer which must be freed with fsearch_index_free.
void *fsearch_index_build_from_paths_c(const char **paths, size_t count);
void fsearch_index_free(void *idx);
void fsearch_index_list_entries_c(void *idx, fsearch_result_cb_t cb, void *userdata);

uint64_t fsearch_start_search_c(const char *query);
uint64_t fsearch_start_search_with_cb_c(const char *query, fsearch_result_cb_t cb, void *userdata);
void fsearch_poll_results_c(uint64_t handle, fsearch_result_cb_t cb, void *userdata);
void fsearch_cancel_search_c(uint64_t handle);
void fsearch_shutdown(void);

#ifdef __cplusplus
}
#endif

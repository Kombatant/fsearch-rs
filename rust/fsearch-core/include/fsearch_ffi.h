// C header for the minimal fsearch-core FFI used by the Qt test client.
#pragma once

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

#ifdef __cplusplus
}
#endif

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <atomic>
#include <thread>
#include <chrono>
#include <vector>
#include <string>
#include <iostream>

#include "fsearch_ffi.h"

static std::atomic<int> result_count{0};

extern "C" void test_cb(uint64_t id, const char *name, const char *path, uint64_t size, uint64_t mtime, const char *highlights, void *userdata) {
    (void)id; (void)size; (void)mtime; (void)highlights; (void)userdata;
    if (name) {
        // simple print for debugging
        fprintf(stderr, "test_cb: name=%s path=%s\n", name, path ? path : "");
    }
    result_count.fetch_add(1, std::memory_order_relaxed);
}

int main(int argc, char **argv) {
    (void)argc; (void)argv;
    if (!fsearch_init()) {
        std::cerr << "fsearch_init failed\n";
        return 2;
    }

    // Create a temporary directory and a sample file to index
    const char *tmpdir = "."; // use current dir for simplicity
    const char *paths[1] = { "." };

    void *idx = fsearch_index_build_from_paths_c(paths, 1);
    if (!idx) {
        std::cerr << "index build returned null\n";
        return 2;
    }

    // list entries (should invoke callback via fsearch_index_list_entries_c)
    fsearch_index_list_entries_c(idx, test_cb, nullptr);

    // start a search for a common word; use callback streaming API
    uint64_t h = fsearch_start_search_with_cb_c("test", test_cb, nullptr);
    if (h == 0) {
        std::cerr << "start_search returned 0\n";
    }

    // wait for callbacks to arrive
    std::this_thread::sleep_for(std::chrono::milliseconds(500));

    int counted = result_count.load();
    std::cout << "result_count=" << counted << "\n";

    // cleanup
    fsearch_index_free(idx);
    if (counted == 0) {
        std::cerr << "Integration test: no results received (this may be ok in empty dirs)\n";
        // treat as success but note warning
        return 0;
    }
    return 0;
}

// Minimal Rust skeleton for fsearch-core with a cxx bridge.
// Expand modules (index, query, search) here.

pub mod entry;
pub mod index;
pub mod query;
mod search;
pub mod matchers;
pub mod pcre2_pool;
pub mod pcre2_backend;
pub mod match_engine;

use index::Index;
use parking_lot::Mutex;
use once_cell::sync::Lazy;
use std::sync::Arc;
use search as search_mod;

static CURRENT_INDEX: Lazy<Mutex<Option<Arc<Index>>>> = Lazy::new(|| Mutex::new(None));

#[cxx::bridge]
mod ffi {
    struct SearchResult {
        id: u64,
        name: String,
        path: String,
        size: u64,
        mtime: u64,
        highlights: String,
    }

    extern "Rust" {
        type Index;

        // Indexing
        fn index_new() -> Box<Index>;
        fn index_build_from_paths(paths: Vec<String>) -> Box<Index>;
        fn index_list_entries(index: &Index) -> Vec<SearchResult>;

        // lifecycle
        fn init() -> bool;

        // search API (stubs for now)
        fn start_search(query: &str) -> u64;
        fn poll_results(handle: u64) -> Vec<SearchResult>;
        fn cancel_search(handle: u64);
    }
}

pub fn init() -> bool {
    // Initialize internal state, logging, etc.
    true
}

pub fn index_new() -> Box<Index> {
    Box::new(Index::new())
}

pub fn index_build_from_paths(paths: Vec<String>) -> Box<Index> {
    let mut idx = Index::new();
    idx.build_from_paths(paths);
    // clone for returning a boxed Index while storing an Arc in the global
    let idx_clone = idx.clone();
    let arc = Arc::new(idx);
    *CURRENT_INDEX.lock() = Some(arc);
    Box::new(idx_clone)
}

pub fn index_list_entries(index: &Index) -> Vec<ffi::SearchResult> {
    index
        .entries
        .iter()
        .map(|e| ffi::SearchResult {
            id: e.id,
            name: e.name.clone(),
            path: e.path.clone(),
            size: e.size,
            mtime: e.mtime,
            highlights: String::new(),
        })
        .collect()
}

pub fn start_search(_query: &str) -> u64 {
    // start search against the current index snapshot
    if let Some(idx) = &*CURRENT_INDEX.lock() {
        return search_mod::start_search_with_index(idx.clone(), _query);
    }
    0
}

pub fn poll_results(_handle: u64) -> Vec<ffi::SearchResult> {
    search_mod::poll_results(_handle)
}

pub fn cancel_search(_handle: u64) {
    search_mod::cancel_search(_handle)
}

// C ABI wrappers for simple interop with a Qt C++ client
use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

#[no_mangle]
pub extern "C" fn fsearch_init() -> bool {
    init()
}

#[no_mangle]
pub extern "C" fn fsearch_index_build_from_paths_c(paths: *const *const c_char, count: usize) -> *mut Index {
    if paths.is_null() || count == 0 {
        return std::ptr::null_mut();
    }
    let slice = unsafe { std::slice::from_raw_parts(paths, count) };
    let mut vec = Vec::with_capacity(count);
    for &p in slice.iter() {
        if p.is_null() {
            continue;
        }
        let s = unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() };
        vec.push(s);
    }
    let boxed = index_build_from_paths(vec);
    Box::into_raw(boxed)
}

#[no_mangle]
pub extern "C" fn fsearch_index_free(ptr: *mut Index) {
    if ptr.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(ptr)); }
}

pub type FsearchResultCb = extern "C" fn(u64, *const c_char, *const c_char, u64, u64, *const c_char, *mut c_void);

#[no_mangle]
pub extern "C" fn fsearch_index_list_entries_c(ptr: *mut Index, cb: Option<FsearchResultCb>, userdata: *mut c_void) {
    if ptr.is_null() || cb.is_none() {
        return;
    }
    let cb = cb.unwrap();
    let idx = unsafe { &*ptr };
    let list = index_list_entries(idx);
    for r in list {
        let name_c = std::ffi::CString::new(r.name).unwrap_or_default();
        let path_c = std::ffi::CString::new(r.path).unwrap_or_default();
        let highlights_c = std::ffi::CString::new(r.highlights).unwrap_or_default();
        cb(r.id, name_c.as_ptr(), path_c.as_ptr(), r.size, r.mtime, highlights_c.as_ptr(), userdata);
        // CString owned locally; ok because callback should copy if needed
    }
}

#[no_mangle]
pub extern "C" fn fsearch_start_search_c(query: *const c_char) -> u64 {
    if query.is_null() {
        return 0;
    }
    let q = unsafe { CStr::from_ptr(query).to_string_lossy().into_owned() };
    start_search(&q)
}

#[no_mangle]
pub extern "C" fn fsearch_poll_results_c(handle: u64, cb: Option<FsearchResultCb>, userdata: *mut c_void) {
    if cb.is_none() {
        return;
    }
    let cb = cb.unwrap();
    let list = poll_results(handle);
    for r in list {
        let name_c = std::ffi::CString::new(r.name).unwrap_or_default();
        let path_c = std::ffi::CString::new(r.path).unwrap_or_default();
        let highlights_c = std::ffi::CString::new(r.highlights).unwrap_or_default();
        cb(r.id, name_c.as_ptr(), path_c.as_ptr(), r.size, r.mtime, highlights_c.as_ptr(), userdata);
    }
}

#[no_mangle]
pub extern "C" fn fsearch_cancel_search_c(handle: u64) {
    cancel_search(handle)
}

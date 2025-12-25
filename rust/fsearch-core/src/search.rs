use crate::ffi::SearchResult as FfiSearchResult;
use crate::query::Parser;
use crate::query::QueryMatcher;
use crate::pcre2_pool::PatternPool;
use crate::index::Index;
use crossbeam_channel::{unbounded, Receiver, Sender};
use unicode_segmentation::UnicodeSegmentation;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

type HandleId = u64;

pub struct SearchContext {
    pub receiver: Receiver<FfiSearchResult>,
    pub cancel_flag: Arc<AtomicBool>,
}

static HANDLE_MAP: Lazy<Mutex<HashMap<HandleId, SearchContext>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn next_handle_id() -> u64 {
    use std::sync::atomic::AtomicU64;
    static H: Lazy<AtomicU64> = Lazy::new(|| AtomicU64::new(1));
    H.fetch_add(1, Ordering::SeqCst)
}

/// Start a search over the provided index (snapshot) using a simple query language.
/// For now: plain substring (case-insensitive) or regex if query starts with `re:`.
pub fn start_search_with_index(idx: Arc<Index>, query: &str) -> u64 {
    let (s, r): (Sender<FfiSearchResult>, Receiver<FfiSearchResult>) = unbounded();
    let cancel = Arc::new(AtomicBool::new(false));

    let q = query.to_string();
    let cancel_clone = cancel.clone();
    // spawn a thread to run the search and stream results into sender
    std::thread::spawn(move || {
        if idx.entries.is_empty() {
            drop(s);
            return;
        }

        // Try to parse query into AST; if successful compile via QueryMatcher
        let mut compiled_opt: Option<crate::query::matcher::CompiledNode> = None;
        let pool = PatternPool::new();
        if let Ok(mut parser) = std::panic::catch_unwind(|| Parser::new(&q)) {
            if let Some(node) = parser.parse() {
                if let Ok(comp) = QueryMatcher::new(pool.clone()).compile(&node) {
                    compiled_opt = Some(comp);
                }
            }
        }

        // prepare legacy regex/substring fallback
        let is_regex = q.starts_with("re:");
        let pattern = if is_regex { q[3..].to_string() } else { q.clone() };
        let regex: Option<Regex> = if is_regex { Regex::new(&pattern).ok() } else { None };
        let lower_pat = pattern.to_lowercase();

        // parallel iterate entries
        idx.entries.par_iter().for_each(|e| {
            if cancel_clone.load(Ordering::SeqCst) {
                return;
            }
            // If we have a compiled query, use it for matching and metadata
            if let Some(compiled) = &compiled_opt {
                let text = format!("{}\n{}", e.name, e.path);
                let bytes = text.as_bytes();
                if QueryMatcher::new(pool.clone()).is_match(compiled, bytes) {
                    let metas = QueryMatcher::new(pool.clone()).captures_meta(compiled, bytes);
                    // serialize metas to simple JSON array
                    let mut parts = Vec::new();
                    for m in metas {
                        let mut ranges_parts = Vec::new();
                        for (a,b) in m.ranges {
                            ranges_parts.push(format!("[{},{}]", a, b));
                        }
                        let field_json = match m.field { Some(f) => format!("\"{}\"", f), None => "null".to_string() };
                        parts.push(format!("{{\"field\":{},\"ranges\":[{}]}}", field_json, ranges_parts.join(",")));
                    }
                    let highlights = format!("[{}]", parts.join(","));
                    let res = FfiSearchResult { id: e.id, name: e.name.clone(), path: e.path.clone(), size: e.size, mtime: e.mtime, highlights };
                    let _ = s.send(res);
                }
            } else {
                let matched = if let Some(re) = &regex {
                    re.is_match(&e.path) || re.is_match(&e.name)
                } else {
                    // case-insensitive substring search on normalized fields
                    e.normalized.contains(&lower_pat) || e.path.to_lowercase().contains(&lower_pat)
                };
                if matched {
                    let res = FfiSearchResult { id: e.id, name: e.name.clone(), path: e.path.clone(), size: e.size, mtime: e.mtime, highlights: String::new() };
                    let _ = s.send(res);
                }
            }
        });

        // finished
        drop(s);
    });

    let id = next_handle_id();
    let ctx = SearchContext { receiver: r, cancel_flag: cancel };
    HANDLE_MAP.lock().insert(id, ctx);
    id
}

/// Convert a byte-range (start..end) inside `text` into UTF-16 code-unit
/// [start,end) indices that align to grapheme cluster boundaries. This makes
/// the resulting indices safe to apply directly to Qt `QString` (which uses
/// UTF-16 code units for indexing).
fn byte_range_to_utf16_bounds(text: &str, start: usize, end: usize) -> (usize, usize) {
    // Collect grapheme start byte indices
    let mut starts: Vec<usize> = Vec::new();
    for (byte_idx, _) in text.grapheme_indices(true) {
        starts.push(byte_idx);
    }
    // Ensure final boundary at text.len()
    if starts.last().copied().unwrap_or(0) != text.len() {
        starts.push(text.len());
    }

    // find grapheme that contains start
    let mut gstart = 0usize;
    for i in 0..starts.len()-1 {
        if start >= starts[i] && start < starts[i+1] {
            gstart = starts[i];
            break;
        }
    }
    // find grapheme boundary that contains end -> take next boundary
    let mut gend = text.len();
    for i in 0..starts.len()-1 {
        if end > starts[i] && end <= starts[i+1] {
            gend = starts[i+1];
            break;
        }
    }

    let start_units = text[..gstart].encode_utf16().count();
    let end_units = text[..gend].encode_utf16().count();
    (start_units, end_units)
}

/// Start a search and invoke the provided C callback for each matching result.
/// This is event-driven: results are delivered by Rust calling the callback as
/// they are found. Note: callers should ensure the callback is thread-safe or
/// marshal GUI updates to the main thread (Qt client does this).
pub fn start_search_with_index_and_cb(idx: Arc<Index>, query: &str, cb: extern "C" fn(u64, *const std::os::raw::c_char, *const std::os::raw::c_char, u64, u64, *const std::os::raw::c_char, *mut std::os::raw::c_void), userdata: *mut std::os::raw::c_void) -> u64 {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let q = query.to_string();
    let id = next_handle_id();
    let userdata_usize = userdata as usize;

    // Spawn worker thread that calls cb directly for matches.
    std::thread::spawn(move || {
        if idx.entries.is_empty() { return; }

        let mut compiled_opt: Option<crate::query::matcher::CompiledNode> = None;
        let pool = PatternPool::new();
        if let Ok(mut parser) = std::panic::catch_unwind(|| Parser::new(&q)) {
            if let Some(node) = parser.parse() {
                if let Ok(comp) = QueryMatcher::new(pool.clone()).compile(&node) {
                    compiled_opt = Some(comp);
                }
            }
        }

        let is_regex = q.starts_with("re:");
        let pattern = if is_regex { q[3..].to_string() } else { q.clone() };
        let regex: Option<regex::Regex> = if is_regex { regex::Regex::new(&pattern).ok() } else { None };
        let lower_pat = pattern.to_lowercase();

        for e in idx.entries.iter() {
            if cancel_clone.load(Ordering::SeqCst) { break; }
            if let Some(compiled) = &compiled_opt {
                let text = format!("{}\n{}", e.name, e.path);
                let bytes = text.as_bytes();
                if QueryMatcher::new(pool.clone()).is_match(compiled, bytes) {
                    let metas = QueryMatcher::new(pool.clone()).captures_meta(compiled, bytes);
                    // Build highlights JSON using UTF-16 grapheme-safe boundaries
                    let mut parts = Vec::new();
                    for m in metas {
                        let mut ranges_parts = Vec::new();
                        for (a,b) in m.ranges {
                            let s = a;
                            let e_b = b;
                            let (su, eu) = byte_range_to_utf16_bounds(&text, s, e_b);
                            ranges_parts.push(format!("[{},{}]", su, eu));
                        }
                        let field_json = match m.field { Some(f) => format!("\"{}\"", f), None => "null".to_string() };
                        parts.push(format!("{{\"field\":{},\"ranges\":[{}]}}", field_json, ranges_parts.join(",")));
                    }
                    let highlights = format!("[{}]", parts.join(","));
                    // call callback
                    let name_c = std::ffi::CString::new(e.name.clone()).unwrap_or_default();
                    let path_c = std::ffi::CString::new(e.path.clone()).unwrap_or_default();
                    let highlights_c = std::ffi::CString::new(highlights).unwrap_or_default();
                    let ud = userdata_usize as *mut std::os::raw::c_void;
                    cb(e.id, name_c.as_ptr(), path_c.as_ptr(), e.size, e.mtime, highlights_c.as_ptr(), ud);
                }
            } else {
                let matched = if let Some(re) = &regex {
                    re.is_match(&e.path) || re.is_match(&e.name)
                } else {
                    e.normalized.contains(&lower_pat) || e.path.to_lowercase().contains(&lower_pat)
                };
                if matched {
                    let name_c = std::ffi::CString::new(e.name.clone()).unwrap_or_default();
                    let path_c = std::ffi::CString::new(e.path.clone()).unwrap_or_default();
                    let highlights_c = std::ffi::CString::new("".to_string()).unwrap_or_default();
                    let ud = userdata_usize as *mut std::os::raw::c_void;
                    cb(e.id, name_c.as_ptr(), path_c.as_ptr(), e.size, e.mtime, highlights_c.as_ptr(), ud);
                }
            }
        }
    });

    let ctx = SearchContext { receiver: crossbeam_channel::unbounded().1 /*unused*/, cancel_flag: cancel };
    HANDLE_MAP.lock().insert(id, ctx);
    id
}

pub fn poll_results(handle: u64) -> Vec<FfiSearchResult> {
    let mut out = Vec::new();
    let map = HANDLE_MAP.lock();
    if let Some(ctx) = map.get(&handle) {
        use crossbeam_channel::TryRecvError;
        loop {
            match ctx.receiver.try_recv() {
                Ok(item) => out.push(item),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // channel closed and drained; remove handle
                    drop(map);
                    HANDLE_MAP.lock().remove(&handle);
                    break;
                }
            }
        }
    }
    out
}

pub fn cancel_search(handle: u64) {
    let map = HANDLE_MAP.lock();
    if let Some(ctx) = map.get(&handle) {
        ctx.cancel_flag.store(true, Ordering::SeqCst);
    }
    // don't remove immediately; let poll_results clean up
}

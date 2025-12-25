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
    pub join_handle: Option<std::thread::JoinHandle<()>>,
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
    let join = std::thread::spawn(move || {
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
                    // prefer compiled node field when metas don't specify one
                    let compiled_field = match compiled {
                        crate::query::matcher::CompiledNode::Leaf { field, .. } => field.clone(),
                        crate::query::matcher::CompiledNode::Compare { field, .. } => field.clone(),
                        crate::query::matcher::CompiledNode::Range { field, .. } => field.clone(),
                        _ => None,
                    };
                    // serialize metas to simple JSON array
                    let mut parts = Vec::new();
                    for mut m in metas {
                        if m.field.is_none() {
                            m.field = compiled_field.clone();
                        }
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
    let ctx = SearchContext { receiver: r, cancel_flag: cancel, join_handle: Some(join) };
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ffi::CStr;
    // no direct c_void import needed; use std::os::raw types inline
    use std::time::Duration;
    use std::sync::mpsc;
    use tempfile::tempdir;
    use std::fs::File;
    use std::io::Write;
    use serde_json::Value;

    #[test]
    fn utf16_mapping_flag_emoji() {
        let s = "a🇺🇸b"; // '🇺🇸' is a flag composed of two regional indicators
        let flag = "🇺🇸";
        let start = s.find(flag).expect("flag present");
        let end = start + flag.len();
        let (su, eu) = byte_range_to_utf16_bounds(s, start, end);
        let expected_su = s[..start].encode_utf16().count();
        let expected_eu = s[..end].encode_utf16().count();
        assert_eq!((su, eu), (expected_su, expected_eu));
    }

    #[test]
    fn utf16_mapping_combining_mark() {
        // 'e' + combining acute accent (U+0301) should be a single grapheme
        let s = "x e\u{301} y"; // string contains 'é' composed
        // find the composed sequence by searching for 'e' and then include combining mark
        let idx_e = s.find('e').expect("e present");
        let start = idx_e;
        // end should include the combining mark byte length
        let end = start + s[start..].chars().next().unwrap().len_utf8() + "\u{301}".len();
        let (su, eu) = byte_range_to_utf16_bounds(s, start, end);
        let expected_su = s[..start].encode_utf16().count();
        let expected_eu = s[..end].encode_utf16().count();
        assert_eq!((su, eu), (expected_su, expected_eu));
    }

    #[test]
    fn utf16_mapping_zwj_sequence() {
        // family emoji is often a ZWJ sequence forming one grapheme cluster
        let family = "👩‍👩‍👧‍👦";
        let s = format!("A{}Z", family);
        let start = s.find(family).expect("family present");
        let end = start + family.len();
        let (su, eu) = byte_range_to_utf16_bounds(&s, start, end);
        let expected_su = s[..start].encode_utf16().count();
        let expected_eu = s[..end].encode_utf16().count();
        assert_eq!((su, eu), (expected_su, expected_eu));
    }

    #[test]
    fn full_pipeline_event_driven_highlight() {
        // create temp dir and files with multibyte/grapheme names
        let dir = tempdir().expect("tempdir");
        let p = dir.path();
        let f1 = p.join("alpha_🇺🇸_test.txt");
        let f2 = p.join("beta_e\u{301}_sample.txt");
        File::create(&f1).unwrap().write_all(b"x").unwrap();
        File::create(&f2).unwrap().write_all(b"y").unwrap();

        // build index from temp dir (this sets CURRENT_INDEX)
        let paths = vec![p.to_string_lossy().to_string()];
        let _boxed = crate::index_build_from_paths(paths);

        // setup channel to receive highlight JSON from callback
        let (tx, rx) = mpsc::channel::<String>();
        let tx_box = Box::into_raw(Box::new(tx));

        extern "C" fn cb(_id: u64, name: *const std::os::raw::c_char, _path: *const std::os::raw::c_char, _size: u64, _mtime: u64, highlights: *const std::os::raw::c_char, userdata: *mut std::os::raw::c_void) {
            unsafe {
                let hl = if highlights.is_null() { String::new() } else { CStr::from_ptr(highlights).to_string_lossy().into_owned() };
                let nm = if name.is_null() { String::new() } else { CStr::from_ptr(name).to_string_lossy().into_owned() };
                let obj = serde_json::json!({"name": nm, "highlights": hl});
                let s = obj.to_string();
                let tx: &std::sync::mpsc::Sender<String> = &*(userdata as *mut std::sync::mpsc::Sender<String>);
                let _ = tx.send(s);
            }
        }

        let q = CString::new("test").unwrap();
        let handle = crate::fsearch_start_search_with_cb_c(q.as_ptr(), Some(cb), tx_box as *mut std::os::raw::c_void);
        assert!(handle != 0);

        // wait for at least one highlight message
        let msg = rx.recv_timeout(Duration::from_secs(5)).expect("got highlight JSON");
        let wrapper: Value = serde_json::from_str(&msg).expect("valid wrapper json");
        let name = wrapper.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let highlights_str = wrapper.get("highlights").and_then(|v| v.as_str()).unwrap_or("");
        let v: Value = serde_json::from_str(highlights_str).expect("valid highlights json");
        assert!(v.is_array());
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());
        // find a name-field entry with a non-empty ranges array
        let mut found = false;
        for it in arr.iter() {
            let field_opt = it.get("field").and_then(|f| f.as_str());
            if field_opt == Some("name") || field_opt.is_none() {
                if let Some(ranges) = it.get("ranges") {
                    if ranges.is_array() && !ranges.as_array().unwrap().is_empty() {
                        // validate that the first range extracts substring "test" from `name` when using UTF-16 indices
                        let r = ranges.as_array().unwrap()[0].as_array().unwrap();
                        let s_idx = r[0].as_u64().unwrap() as usize;
                        let e_idx = r[1].as_u64().unwrap() as usize;
                        // compute expected UTF-16 indices for substring "test" in name
                        if let Some(pos) = name.find("test") {
                            let expected_s = name[..pos].encode_utf16().count();
                            let expected_e = name[..pos+"test".len()].encode_utf16().count();
                            assert_eq!(s_idx, expected_s, "start index matches");
                            assert_eq!(e_idx, expected_e, "end index matches");
                            found = true;
                            break;
                        }
                    }
                }
            }
        }
        if !found {
            eprintln!("DEBUG name='{}' highlights_json='{}' parsed_arr='{}'", name, highlights_str, serde_json::to_string_pretty(&v).unwrap_or_default());
        }
        assert!(found, "expected a name-field ranges entry matching 'test'");

        // cancel search worker and allow it to exit before dropping userdata
        crate::cancel_search(handle);
        std::thread::sleep(Duration::from_millis(50));
        // cleanup boxed sender
        unsafe { drop(Box::from_raw(tx_box)); }
    }

    #[test]
    fn full_pipeline_event_driven_highlight_path() {
        // create temp dir with a subdirectory that contains 'test' in its name
        let dir = tempdir().expect("tempdir");
        let p = dir.path();
        let sub = p.join("sub_testdir");
        std::fs::create_dir_all(&sub).unwrap();
        let f = sub.join("file.txt");
        File::create(&f).unwrap().write_all(b"content").unwrap();

        // build index from temp dir
        let paths = vec![p.to_string_lossy().to_string()];
        let _boxed = crate::index_build_from_paths(paths);

        // setup channel to receive highlight JSON from callback (send name+path+highlights)
        let (tx, rx) = mpsc::channel::<String>();
        let tx_box = Box::into_raw(Box::new(tx));

        extern "C" fn cb(_id: u64, name: *const std::os::raw::c_char, path: *const std::os::raw::c_char, _size: u64, _mtime: u64, highlights: *const std::os::raw::c_char, userdata: *mut std::os::raw::c_void) {
            unsafe {
                let hl = if highlights.is_null() { String::new() } else { CStr::from_ptr(highlights).to_string_lossy().into_owned() };
                let p = if path.is_null() { String::new() } else { CStr::from_ptr(path).to_string_lossy().into_owned() };
                let n = if name.is_null() { String::new() } else { CStr::from_ptr(name).to_string_lossy().into_owned() };
                let obj = serde_json::json!({"name": n, "path": p, "highlights": hl});
                let s = obj.to_string();
                let tx: &std::sync::mpsc::Sender<String> = &*(userdata as *mut std::sync::mpsc::Sender<String>);
                let _ = tx.send(s);
            }
        }

        let q = CString::new("path:test").unwrap();
        let handle = crate::fsearch_start_search_with_cb_c(q.as_ptr(), Some(cb), tx_box as *mut std::os::raw::c_void);
        assert!(handle != 0);

        // wait for a callback
        let msg = rx.recv_timeout(Duration::from_secs(5)).expect("got highlight JSON");
        let wrapper: Value = serde_json::from_str(&msg).expect("valid wrapper json");
        let path = wrapper.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let highlights_str = wrapper.get("highlights").and_then(|v| v.as_str()).unwrap_or("");
        let v: Value = serde_json::from_str(highlights_str).expect("valid highlights json");
        assert!(v.is_array());
        let arr = v.as_array().unwrap();
        assert!(!arr.is_empty());

        // Look for a ranges entry that maps to substring "test" in `path`
        // and ensure the metadata explicitly marks `field` == "path"
        let mut found = false;
        for it in arr.iter() {
            let field_opt = it.get("field").and_then(|f| f.as_str());
            if field_opt == Some("path") {
                if let Some(ranges) = it.get("ranges") {
                    if ranges.is_array() && !ranges.as_array().unwrap().is_empty() {
                        let r = ranges.as_array().unwrap()[0].as_array().unwrap();
                        let s_idx = r[0].as_u64().unwrap() as usize;
                        let e_idx = r[1].as_u64().unwrap() as usize;
                        // compute combined text UTF-16 indices mapping: name + '\n' + path
                        let name = wrapper.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let name_units = name.encode_utf16().count();
                        // the worker code constructs text = format!("{}\n{}", e.name, e.path)
                        // so path UTF-16 indices start at name_units + 1 (the newline)
                        let combined_start = name_units + 1;
                        if s_idx >= combined_start {
                            // slice relative to path
                            let rel_start = s_idx - combined_start;
                            let rel_end = e_idx - combined_start;
                            let s_utf16: Vec<u16> = path.encode_utf16().collect();
                            if rel_end <= s_utf16.len() && rel_start < rel_end {
                                let slice = String::from_utf16(&s_utf16[rel_start..rel_end]).unwrap_or_default();
                                if slice.to_lowercase().contains("test") {
                                    found = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // cancel search worker and allow it to exit before dropping userdata
        crate::cancel_search(handle);
        std::thread::sleep(Duration::from_millis(50));
        // cleanup boxed sender
        unsafe { drop(Box::from_raw(tx_box)); }

        assert!(found, "didn't find a path-field highlight covering 'test'");
    }
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
    let join = std::thread::spawn(move || {
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
                    // prefer compiled node field when metas don't specify one
                    let compiled_field = match compiled {
                        crate::query::matcher::CompiledNode::Leaf { field, .. } => field.clone(),
                        crate::query::matcher::CompiledNode::Compare { field, .. } => field.clone(),
                        crate::query::matcher::CompiledNode::Range { field, .. } => field.clone(),
                        _ => None,
                    };
                    // Build highlights JSON using UTF-16 grapheme-safe boundaries
                    let mut parts = Vec::new();
                    for mut m in metas {
                        if m.field.is_none() {
                            m.field = compiled_field.clone();
                        }
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

    let ctx = SearchContext { receiver: crossbeam_channel::unbounded().1 /*unused*/, cancel_flag: cancel, join_handle: Some(join) };
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
    // Set cancel flag and join the worker thread if present to ensure it exits
    let mut map = HANDLE_MAP.lock();
    if let Some(ctx) = map.remove(&handle) {
        ctx.cancel_flag.store(true, Ordering::SeqCst);
        if let Some(join) = ctx.join_handle {
            // drop lock while joining
            drop(map);
            let _ = join.join();
        }
    }
}

/// Cancel and join all active searches. Safe to call multiple times.
pub fn shutdown_all() {
    // collect handles first to avoid holding lock while joining
    let handles: Vec<u64> = {
        let map = HANDLE_MAP.lock();
        map.keys().copied().collect()
    };
    for h in handles {
        cancel_search(h);
    }
}

use crate::ffi::SearchResult as FfiSearchResult;
use crate::index::Index;
use crossbeam_channel::{unbounded, Receiver, Sender};
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

        // prepare matcher
        let is_regex = q.starts_with("re:");
        let pattern = if is_regex { q[3..].to_string() } else { q.clone() };
        let regex: Option<Regex> = if is_regex {
            Regex::new(&pattern).ok()
        } else {
            None
        };

        // prepare lowercase pattern once
        let lower_pat = pattern.to_lowercase();

        // parallel iterate entries
        idx.entries.par_iter().for_each(|e| {
            if cancel_clone.load(Ordering::SeqCst) {
                return;
            }
            let matched = if let Some(re) = &regex {
                re.is_match(&e.path) || re.is_match(&e.name)
            } else {
                // case-insensitive substring search on normalized fields
                e.normalized.contains(&lower_pat) || e.path.to_lowercase().contains(&lower_pat)
            };
            if matched {
                let res = FfiSearchResult {
                    id: e.id,
                    name: e.name.clone(),
                    path: e.path.clone(),
                    size: e.size,
                    mtime: e.mtime,
                };
                // ignore send errors (receiver closed)
                let _ = s.send(res);
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

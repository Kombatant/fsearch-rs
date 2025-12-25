use crate::matchers::Matcher;
use crate::pcre2_backend::Pcre2Compiled;
use pcre2::Error as Pcre2Error;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Trait describing a compiled pattern and matching operations.
pub trait CompiledPattern: Send + Sync {
    fn is_match(&self, text: &[u8]) -> bool;
    fn captures_ranges(&self, text: &[u8]) -> Option<Vec<(usize, usize)>>;
}

impl CompiledPattern for Matcher {
    fn is_match(&self, text: &[u8]) -> bool { self.is_match(text) }
    fn captures_ranges(&self, text: &[u8]) -> Option<Vec<(usize, usize)>> { self.captures_ranges(text) }
}

impl CompiledPattern for Pcre2Compiled {
    fn is_match(&self, text: &[u8]) -> bool { self.is_match(text) }
    fn captures_ranges(&self, text: &[u8]) -> Option<Vec<(usize, usize)>> { self.captures_ranges(text) }
}

/// A per-thread pool of compiled patterns. In the final implementation,
/// this will manage PCRE2 compiled regexes and per-thread match_data.
#[derive(Clone)]
pub struct PatternPool {
    inner: Arc<Mutex<VecDeque<Arc<dyn CompiledPattern>>>>,
}

impl PatternPool {
    pub fn new() -> Self {
        PatternPool { inner: Arc::new(Mutex::new(VecDeque::new())) }
    }

    /// Acquire a compiled pattern for use. If the pool has a cached one,
    /// return it; otherwise compile a new one with the provided factory.
    pub fn acquire<F>(&self, factory: F) -> Arc<dyn CompiledPattern>
    where
        F: FnOnce() -> Arc<dyn CompiledPattern>,
    {
        if let Some(p) = self.inner.lock().unwrap().pop_front() {
            p
        } else {
            factory()
        }
    }

    /// Convenience: acquire a PCRE2-compiled pattern for `pattern`.
    /// Compiles a new `Pcre2Compiled` if the pool is empty.
    pub fn acquire_pcre2(&self, pattern: &str) -> Result<Arc<dyn CompiledPattern>, Pcre2Error> {
        // try to pop a cached compiled pattern of any type
        if let Some(p) = self.inner.lock().unwrap().pop_front() {
            return Ok(p);
        }
        // otherwise compile a new PCRE2 pattern
        let pc = Pcre2Compiled::new(pattern)?;
        Ok(Arc::new(pc))
    }

    /// Return a compiled pattern to the pool for reuse.
    pub fn release(&self, pat: Arc<dyn CompiledPattern>) {
        let mut q = self.inner.lock().unwrap();
        q.push_back(pat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn pool_acquire_release() {
        let pool = PatternPool::new();
        let factory = || Arc::new(Matcher::new("foo", false).unwrap()) as Arc<dyn CompiledPattern>;
        let p = pool.acquire(factory);
        assert!(p.is_match(b"this is foo"));
        pool.release(p);
        // acquire again
        let p2 = pool.acquire(factory);
        assert!(p2.is_match(b"foo bar"));
    }

    #[test]
    fn pool_multi_threaded() {
        let pool = PatternPool::new();
        let factory = || Arc::new(Matcher::new("ab[0-9]+", true).unwrap()) as Arc<dyn CompiledPattern>;
        let mut handles = vec![];
        for _ in 0..4 {
            let pool_c = pool.clone();
            let h = thread::spawn(move || {
                let p = pool_c.acquire(factory);
                assert!(p.is_match(b"xxab123yy"));
                pool_c.release(p);
            });
            handles.push(h);
        }
        for h in handles { h.join().unwrap(); }
    }
}

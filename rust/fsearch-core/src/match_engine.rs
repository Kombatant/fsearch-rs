use crate::pcre2_pool::PatternPool;
use pcre2::Error as Pcre2Error;

/// Match text using a pattern from the pool. If `is_regex` is true,
/// the `pattern` is treated as a regex; otherwise it is treated as a
/// literal substring (escaped for PCRE2).
pub fn match_text_pcre2(pool: &PatternPool, pattern: &str, text: &[u8], is_regex: bool) -> Result<Option<Vec<(usize, usize)>>, Pcre2Error> {
    if is_regex {
        let pat = pool.acquire_pcre2(pattern)?;
        Ok(pat.captures_ranges(text))
    } else {
        let escaped = regex::escape(pattern);
        let pat = pool.acquire_pcre2(&escaped)?;
        Ok(pat.captures_ranges(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcre2_pool::PatternPool;

    #[test]
    fn engine_literal_match() {
        let pool = PatternPool::new();
        let res = match_text_pcre2(&pool, "foo", b"this is foo bar", false).unwrap();
        assert!(res.is_some());
        let caps = res.unwrap();
        assert_eq!(caps[0], (8, 11));
    }

    #[test]
    fn engine_regex_match() {
        let pool = PatternPool::new();
        let res = match_text_pcre2(&pool, "ab([0-9]+)", b"xxab123yy", true).unwrap();
        assert!(res.is_some());
        let caps = res.unwrap();
        assert_eq!(caps[0], (2, 7));
        assert_eq!(caps[1], (4, 7));
    }
}

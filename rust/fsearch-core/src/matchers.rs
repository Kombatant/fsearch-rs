use regex::bytes::Regex;

/// A small matcher abstraction. Currently uses Rust `regex` crate.
/// It is structured so we can swap the internals to PCRE2 later.
pub struct Matcher {
    re: Regex,
}

impl Matcher {
    /// Compile a pattern. The caller is responsible for deciding
    /// whether the input is a regex literal (e.g. `/pat/`) or a plain substring.
    pub fn new(pattern: &str, is_regex: bool) -> Result<Self, regex::Error> {
        let pat = if is_regex {
            pattern.to_string()
        } else {
            // escape plain text to build a regex that matches literal substrings
            regex::escape(pattern)
        };
        let re = Regex::new(&pat)?;
        Ok(Matcher { re })
    }

    /// Test whether `text` matches the pattern.
    pub fn is_match(&self, text: &[u8]) -> bool {
        self.re.is_match(text)
    }

    /// Return capture ranges for the first match (start, end) per capture group.
    /// Group 0 is the whole match.
    pub fn captures_ranges(&self, text: &[u8]) -> Option<Vec<(usize, usize)>> {
        self.re.captures(text).map(|caps| {
            caps.iter()
                .map(|m| m.map(|m| (m.start(), m.end())).unwrap_or((0,0)))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_plain_substring() {
        let m = Matcher::new("foo", false).unwrap();
        assert!(m.is_match(b"this is foo bar"));
        let ranges = m.captures_ranges(b"this is foo bar").unwrap();
        assert_eq!(ranges[0], (8, 11));
    }

    #[test]
    fn match_regex_with_group() {
        let m = Matcher::new(r"(ab)([0-9]+)", true).unwrap();
        assert!(m.is_match(b"xxab123yy"));
        let ranges = m.captures_ranges(b"xxab123yy").unwrap();
        assert_eq!(ranges[0], (2, 7)); // whole match
        assert_eq!(ranges[1], (2, 4)); // group 1
        assert_eq!(ranges[2], (4, 7)); // group 2
    }
}

use pcre2::bytes::Regex as PcreRegex;

pub struct Pcre2Compiled {
    re: PcreRegex,
}

impl Pcre2Compiled {
    pub fn new(pattern: &str) -> Result<Self, pcre2::Error> {
        let re = PcreRegex::new(pattern)?;
        Ok(Pcre2Compiled { re })
    }

    pub fn is_match(&self, text: &[u8]) -> bool {
        match self.re.is_match(text) {
            Ok(b) => b,
            Err(_) => false,
        }
    }

    pub fn captures_ranges(&self, text: &[u8]) -> Option<Vec<(usize, usize)>> {
        match self.re.captures(text) {
            Ok(Some(caps)) => {
                let mut out = Vec::new();
                for i in 0..caps.len() {
                    if let Some(m) = caps.get(i) {
                        out.push((m.start(), m.end()));
                    } else {
                        out.push((0, 0));
                    }
                }
                Some(out)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcre2_match_simple() {
        let c = Pcre2Compiled::new("ab[0-9]+").unwrap();
        assert!(c.is_match(b"xxab123yy"));
        let caps = c.captures_ranges(b"xxab123yy").unwrap();
        assert_eq!(caps[0], (2, 7));
    }
}

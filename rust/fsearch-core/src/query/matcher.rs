use crate::query::Node;
use crate::pcre2_pool::PatternPool;
use crate::pcre2_pool::CompiledPattern;
use std::sync::Arc;

/// A compiled representation of a query node. This is intentionally
/// lightweight and keeps compiled PCRE2 patterns (or other compiled
/// patterns) inside `Arc<dyn CompiledPattern>` so matching is cheap.
#[derive(Clone)]
pub enum CompiledNode {
    Leaf {
        pat: Arc<dyn CompiledPattern>,
        negated: bool,
        field: Option<String>,
        mods: Vec<String>,
    },
    And(Box<CompiledNode>, Box<CompiledNode>),
    Or(Box<CompiledNode>, Box<CompiledNode>),
    Not(Box<CompiledNode>),
}

pub struct QueryMatcher {
    pool: PatternPool,
}

impl QueryMatcher {
    pub fn new(pool: PatternPool) -> Self {
        QueryMatcher { pool }
    }

    /// Compile a `Node` into a `CompiledNode` using PCRE2 for both
    /// regex and literal terms. Literals are escaped and treated as
    /// substring matches.
    pub fn compile(&self, node: &Node) -> Result<CompiledNode, pcre2::Error> {
        match node {
            Node::Word(s) => {
                let empty: Vec<String> = Vec::new();
                let pattern = build_pattern_for_literal(s, &empty);
                let arc = self.pool.acquire_pcre2(&pattern)?;
                Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: vec![] })
            }
            Node::Regex(pat) => {
                let empty: Vec<String> = Vec::new();
                let pattern = build_pattern_for_regex(pat, &empty);
                let arc = self.pool.acquire_pcre2(&pattern)?;
                Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: vec![] })
            }
            Node::Not(inner) => {
                let c = self.compile(inner)?;
                Ok(CompiledNode::Not(Box::new(c)))
            }
            Node::And(a, b) => Ok(CompiledNode::And(Box::new(self.compile(a)?), Box::new(self.compile(b)?))),
            Node::Or(a, b) => Ok(CompiledNode::Or(Box::new(self.compile(a)?), Box::new(self.compile(b)?))),
            Node::Group(inner) => Ok(self.compile(inner)?),
            Node::Field(name, term) => {
                if term.len() >= 2 && term.starts_with('/') && term.ends_with('/') {
                    let pat = term[1..term.len()-1].to_string();
                    let empty: Vec<String> = Vec::new();
                    let pattern = build_pattern_for_regex(&pat, &empty);
                    let arc = self.pool.acquire_pcre2(&pattern)?;
                    Ok(CompiledNode::Leaf { pat: arc, negated: false, field: Some(name.clone()), mods: vec![] })
                } else {
                    let empty: Vec<String> = Vec::new();
                    let pattern = build_pattern_for_literal(term, &empty);
                    let arc = self.pool.acquire_pcre2(&pattern)?;
                    Ok(CompiledNode::Leaf { pat: arc, negated: false, field: Some(name.clone()), mods: vec![] })
                }
            }
            Node::Modified(inner, mods) => {
                // Apply modifiers to the inner term when compiling.
                match &**inner {
                    Node::Word(s) => {
                        let pattern = build_pattern_for_literal(s, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: mods.clone() })
                    }
                    Node::Regex(pat) => {
                        let pattern = build_pattern_for_regex(pat, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: mods.clone() })
                    }
                    Node::Field(name, term) => {
                        if term.len() >= 2 && term.starts_with('/') && term.ends_with('/') {
                            let pat = term[1..term.len()-1].to_string();
                            let pattern = build_pattern_for_regex(&pat, mods);
                            let arc = self.pool.acquire_pcre2(&pattern)?;
                            Ok(CompiledNode::Leaf { pat: arc, negated: false, field: Some(name.clone()), mods: mods.clone() })
                        } else {
                            let pattern = build_pattern_for_literal(term, mods);
                            let arc = self.pool.acquire_pcre2(&pattern)?;
                            Ok(CompiledNode::Leaf { pat: arc, negated: false, field: Some(name.clone()), mods: mods.clone() })
                        }
                    }
                    other => {
                        let s = format!("{:?}", other);
                        let pattern = build_pattern_for_literal(&s, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: mods.clone() })
                    }
                }
            }
            other => {
                // Fallback: try to stringify and match as literal
                let s = format!("{:?}", other);
                let empty: Vec<String> = Vec::new();
                let pattern = build_pattern_for_literal(&s, &empty);
                let arc = self.pool.acquire_pcre2(&pattern)?;
                Ok(CompiledNode::Leaf { pat: arc, negated: false, field: None, mods: vec![] })
            }
        }
    }

    /// Test whether `compiled` matches `text` (normalized bytes).
    pub fn is_match(&self, compiled: &CompiledNode, text: &[u8]) -> bool {
        match compiled {
            CompiledNode::Leaf { pat, negated, .. } => {
                let res = pat.is_match(text);
                if *negated { !res } else { res }
            }
            CompiledNode::Not(inner) => !self.is_match(inner, text),
            CompiledNode::And(a, b) => self.is_match(a, text) && self.is_match(b, text),
            CompiledNode::Or(a, b) => self.is_match(a, text) || self.is_match(b, text),
        }
    }

    /// Return first capture ranges for leaf matches (empty vector when none).
    pub fn captures(&self, compiled: &CompiledNode, text: &[u8]) -> Vec<(usize, usize)> {
        match compiled {
            CompiledNode::Leaf { pat, .. } => pat.captures_ranges(text).unwrap_or_default(),
            CompiledNode::Not(inner) => self.captures(inner, text),
            CompiledNode::And(a, b) => {
                let mut a_caps = self.captures(a, text);
                let b_caps = self.captures(b, text);
                a_caps.extend(b_caps);
                a_caps
            }
            CompiledNode::Or(a, b) => {
                let a_caps = self.captures(a, text);
                if !a_caps.is_empty() { return a_caps; }
                self.captures(b, text)
            }
        }
    }
}

fn escape_literal(s: &str) -> String {
    // Very small escaping for PCRE2 special chars. Wrap in a non-capturing
    // group so the literal can be used as substring search.
    let mut out = String::with_capacity(s.len()*2 + 10);
    out.push_str("(?:");
    for ch in s.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' => { out.push('\\'); out.push(ch); }
            _ => out.push(ch),
        }
    }
    out.push(')');
    out
}

fn build_pattern_for_literal(s: &str, mods: &Vec<String>) -> String {
    let mut pat = String::new();
    // case-insensitive
    if mods.iter().any(|m| m.eq_ignore_ascii_case("i") || m.eq_ignore_ascii_case("icase") || m.eq_ignore_ascii_case("ignorecase")) {
        pat.push_str("(?i)");
    }
    let inner = escape_literal(s);
    pat.push_str(&inner);
    // anchored/exact
    if mods.iter().any(|m| m.starts_with("anch") || m.eq_ignore_ascii_case("exact")) {
        pat = format!("^{}$", pat);
    }
    pat
}

fn build_pattern_for_regex(s: &str, mods: &Vec<String>) -> String {
    let mut pat = String::new();
    if mods.iter().any(|m| m.eq_ignore_ascii_case("i") || m.eq_ignore_ascii_case("icase") || m.eq_ignore_ascii_case("ignorecase")) {
        pat.push_str("(?i)");
    }
    pat.push_str(s);
    if mods.iter().any(|m| m.starts_with("anch") || m.eq_ignore_ascii_case("exact")) {
        pat = format!("^{}$", pat);
    }
    pat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcre2_pool::PatternPool;

    #[test]
    fn matcher_word_literal() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        let node = Node::Word("foo".to_string());
        let compiled = qm.compile(&node).unwrap();
        assert!(qm.is_match(&compiled, b"this is foo"));
        assert!(!qm.is_match(&compiled, b"nope"));
    }

    #[test]
    fn matcher_regex() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        let node = Node::Regex("ab[0-9]+".to_string());
        let compiled = qm.compile(&node).unwrap();
        assert!(qm.is_match(&compiled, b"xxab123yy"));
        let caps = qm.captures(&compiled, b"xxab123yy");
        assert!(caps.len() >= 1);
    }
}

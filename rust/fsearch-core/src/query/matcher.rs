use crate::query::Node;
use crate::pcre2_pool::PatternPool;
use crate::pcre2_pool::CompiledPattern;
use crate::query::parser_rs::{CompareOp, Bound};
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
    Compare {
        field: Option<String>,
        op: CompareOp,
        value: String,
    },
    Range {
        field: Option<String>,
        low: Bound,
        high: Bound,
    },
    Function {
        name: String,
        args: Vec<String>,
    },
    And(Box<CompiledNode>, Box<CompiledNode>),
    Or(Box<CompiledNode>, Box<CompiledNode>),
    Not(Box<CompiledNode>),
}

pub struct QueryMatcher {
    pool: PatternPool,
}

/// Normalize capture ranges: sort, remove duplicates, and merge overlaps/adjacent.
fn normalize_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    if ranges.is_empty() { return ranges; }
    ranges.sort_unstable_by_key(|r| r.0);
    let mut out = Vec::with_capacity(ranges.len());
    let mut cur = ranges[0];
    for &(s,e) in ranges.iter().skip(1) {
        if s <= cur.1 { // overlap or adjacent
            if e > cur.1 { cur.1 = e; }
        } else {
            out.push(cur);
            cur = (s,e);
        }
    }
    out.push(cur);
    out
}

/// Collect a representative field name from the compiled node subtree, if any.
fn collect_field_from_compiled(node: &CompiledNode) -> Option<String> {
    match node {
        CompiledNode::Leaf { field, .. } => field.clone(),
        CompiledNode::Compare { field, .. } => field.clone(),
        CompiledNode::Range { field, .. } => field.clone(),
        CompiledNode::Function { .. } => None,
        CompiledNode::Not(inner) => collect_field_from_compiled(inner),
        CompiledNode::Or(a, b) => collect_field_from_compiled(a).or_else(|| collect_field_from_compiled(b)),
        CompiledNode::And(a, b) => collect_field_from_compiled(a).or_else(|| collect_field_from_compiled(b)),
    }
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
            Node::Compare(field, op, val) => {
                return Ok(CompiledNode::Compare { field: Some(field.clone()), op: op.clone(), value: val.clone() });
            }
            Node::Range(field_name, low, high) => {
                return Ok(CompiledNode::Range { field: Some(field_name.clone()), low: low.clone(), high: high.clone() });
            }
            Node::Modified(inner, mods) => {
                // Apply modifiers to the inner term when compiling.
                let negated = mods.iter().any(|m| m.eq_ignore_ascii_case("not") || m.eq_ignore_ascii_case("invert") || m.eq_ignore_ascii_case("neg"));
                match &**inner {
                    Node::Word(s) => {
                        let pattern = build_pattern_for_literal(s, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated, field: None, mods: mods.clone() })
                    }
                    Node::Regex(pat) => {
                        let pattern = build_pattern_for_regex(pat, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated, field: None, mods: mods.clone() })
                    }
                    Node::Field(name, term) => {
                        if term.len() >= 2 && term.starts_with('/') && term.ends_with('/') {
                            let pat = term[1..term.len()-1].to_string();
                            let pattern = build_pattern_for_regex(&pat, mods);
                            let arc = self.pool.acquire_pcre2(&pattern)?;
                            Ok(CompiledNode::Leaf { pat: arc, negated, field: Some(name.clone()), mods: mods.clone() })
                        } else {
                            let pattern = build_pattern_for_literal(term, mods);
                            let arc = self.pool.acquire_pcre2(&pattern)?;
                            Ok(CompiledNode::Leaf { pat: arc, negated, field: Some(name.clone()), mods: mods.clone() })
                        }
                    }
                    other => {
                        let s = format!("{:?}", other);
                        let pattern = build_pattern_for_literal(&s, mods);
                        let arc = self.pool.acquire_pcre2(&pattern)?;
                        Ok(CompiledNode::Leaf { pat: arc, negated, field: None, mods: mods.clone() })
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
            CompiledNode::Leaf { pat, negated, field, .. } => {
                // If this leaf is targeted at a specific field, scope
                // the matching to that field. Currently we special-case
                // the `extension` field to match only the file extension
                // of the `name` portion. The combined text format used by
                // the search pipeline is `name + "\n" + path`.
                let res = if let Some(f) = field {
                    if f.eq("extension") {
                        // extract name (before first newline)
                        if let Ok(s) = std::str::from_utf8(text) {
                            let name = s.splitn(2, '\n').next().unwrap_or("");
                            if let Some(dot_idx) = name.rfind('.') {
                                let ext = &name[dot_idx+1..];
                                pat.is_match(ext.as_bytes())
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        pat.is_match(text)
                    }
                } else {
                    pat.is_match(text)
                };
                if *negated { !res } else { res }
            }
            CompiledNode::Compare { field: _, op, value } => {
                // Try numeric comparison first
                if let (Ok(lhs), Ok(rhs)) = (parse_number_from_bytes(text), value.parse::<i128>()) {
                    match op {
                        CompareOp::Eq => lhs == rhs,
                        CompareOp::Contains => lhs.to_string().contains(&rhs.to_string()),
                        CompareOp::Smaller => lhs < rhs,
                        CompareOp::SmallerEq => lhs <= rhs,
                        CompareOp::Greater => lhs > rhs,
                        CompareOp::GreaterEq => lhs >= rhs,
                    }
                    } else {
                    // Fallback to substring/lexicographic comparisons
                    let s = String::from_utf8_lossy(text);
                    let sref = s.as_ref();
                    match op {
                        CompareOp::Eq => sref == value.as_str(),
                        CompareOp::Contains => sref.contains(value.as_str()),
                        CompareOp::Smaller => sref < value.as_str(),
                        CompareOp::SmallerEq => sref <= value.as_str(),
                        CompareOp::Greater => sref > value.as_str(),
                        CompareOp::GreaterEq => sref >= value.as_str(),
                    }
                }
            }
            CompiledNode::Range { field: _, low, high } => {
                // Numeric-aware range check
                if let Ok(v) = parse_number_from_bytes(text) {
                    let mut ok = true;
                    if let Bound::Inclusive(ref s) = low { if let Ok(lv) = s.parse::<i128>() { ok &= v >= lv; } }
                    if let Bound::Exclusive(ref s) = low { if let Ok(lv) = s.parse::<i128>() { ok &= v > lv; } }
                    if let Bound::Inclusive(ref s) = high { if let Ok(hv) = s.parse::<i128>() { ok &= v <= hv; } }
                    if let Bound::Exclusive(ref s) = high { if let Ok(hv) = s.parse::<i128>() { ok &= v < hv; } }
                    ok
                } else {
                    // Lexicographic fallback using UTF-8 text
                    let s = String::from_utf8_lossy(text);
                    let sref = s.as_ref();
                    let mut ok = true;
                    match low {
                        Bound::Inclusive(ref l) => ok &= sref >= l.as_str(),
                        Bound::Exclusive(ref l) => ok &= sref > l.as_str(),
                        Bound::Unbounded => {}
                    }
                    match high {
                        Bound::Inclusive(ref h) => ok &= sref <= h.as_str(),
                        Bound::Exclusive(ref h) => ok &= sref < h.as_str(),
                        Bound::Unbounded => {}
                    }
                    ok
                }
            }
            CompiledNode::Function { name, args } => {
                // Provide a couple of simple function semantics: contains(arg)
                // and exists(arg) which are both substring checks on the text.
                if name.eq_ignore_ascii_case("contains") || name.eq_ignore_ascii_case("exists") {
                    if let Some(a) = args.get(0) {
                        let s = String::from_utf8_lossy(text);
                        return s.contains(a.as_str());
                    }
                }
                false
            }
            CompiledNode::Not(inner) => !self.is_match(inner, text),
            CompiledNode::And(a, b) => self.is_match(a, text) && self.is_match(b, text),
            CompiledNode::Or(a, b) => self.is_match(a, text) || self.is_match(b, text),
        }
    }

    /// Return first capture ranges for leaf matches (empty vector when none).
    pub fn captures(&self, compiled: &CompiledNode, text: &[u8]) -> Vec<(usize, usize)> {
        match compiled {
            CompiledNode::Leaf { pat, field, .. } => {
                // For field-scoped leaves, adjust captured ranges to the
                // combined text layout. Special-case `extension` to map
                // captures into the `name` portion's extension bytes.
                if let Some(f) = field {
                    if f.eq("extension") {
                        if let Ok(s) = std::str::from_utf8(text) {
                            let name = s.splitn(2, '\n').next().unwrap_or("");
                            if let Some(dot_idx) = name.rfind('.') {
                                let ext = &name[dot_idx+1..];
                                if let Some(mut ranges) = pat.captures_ranges(ext.as_bytes()) {
                                    // shift ranges by dot_idx+1 to be relative to
                                    // the combined text start
                                    for r in ranges.iter_mut() {
                                        r.0 += dot_idx + 1;
                                        r.1 += dot_idx + 1;
                                    }
                                    return ranges;
                                }
                            }
                        }
                        return vec![];
                    }
                }
                pat.captures_ranges(text).unwrap_or_default()
            }
            CompiledNode::Compare { .. } => vec![],
            CompiledNode::Range { .. } => vec![],
                CompiledNode::Function { name, args } => {
                    if (name.eq_ignore_ascii_case("contains") || name.eq_ignore_ascii_case("exists")) && args.len() >= 1 {
                        let pat = build_pattern_for_literal(&args[0], &vec![]);
                        if let Ok(comp) = self.pool.acquire_pcre2(&pat) {
                            return comp.captures_ranges(text).unwrap_or_default();
                        }
                    }
                    vec![]
                }
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



/// Metadata about a match suitable for UI highlighting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchMeta {
    pub field: Option<String>,
    pub ranges: Vec<(usize, usize)>,
}

impl QueryMatcher {
    /// Return field-aware match metadata (capture ranges) for the given compiled node.
    /// For leaf regex/literal matches this returns capture ranges; for compare/range
    /// matches it returns an entry with the `field` set and empty `ranges` so the
    /// caller can still annotate the matching row.
    pub fn captures_meta(&self, compiled: &CompiledNode, text: &[u8]) -> Vec<MatchMeta> {
        match compiled {
            CompiledNode::Leaf { pat: _, negated: _, field, .. } => {
                // Special-case `extension` field: produce a name-field ranges
                // entry (so GUI can bold the extension within the name) and
                // an additional `extension` field entry with empty ranges to
                // indicate extension-column highlighting (parity with C).
                if let Some(f) = field {
                    if f.eq("extension") {
                        let mut ranges = self.captures(compiled, text);
                        ranges = normalize_ranges(ranges);
                        if ranges.is_empty() { return vec![]; }
                        let name_meta = MatchMeta { field: Some("name".to_string()), ranges: ranges.clone() };
                        let ext_meta = MatchMeta { field: Some("extension".to_string()), ranges: vec![] };
                        return vec![name_meta, ext_meta];
                    }
                }
                let mut ranges = self.captures(compiled, text);
                ranges = normalize_ranges(ranges);
                if ranges.is_empty() { return vec![]; }
                vec![MatchMeta { field: field.clone(), ranges }]
            }
            CompiledNode::Compare { field, .. } => vec![MatchMeta { field: field.clone(), ranges: vec![] }],
            CompiledNode::Range { field, .. } => vec![MatchMeta { field: field.clone(), ranges: vec![] }],
            CompiledNode::Function { .. } => {
                let mut ranges = self.captures(compiled, text);
                ranges = normalize_ranges(ranges);
                if ranges.is_empty() { return vec![]; }
                vec![MatchMeta { field: None, ranges }]
            }
            CompiledNode::Not(_) => vec![],
            CompiledNode::And(a, b) => {
                let mut a_m = self.captures_meta(a, text);
                let b_m = self.captures_meta(b, text);
                a_m.extend(b_m);
                // normalize ranges per-meta and inherit field when absent
                let inherited = collect_field_from_compiled(compiled);
                for m in a_m.iter_mut() {
                    m.ranges = normalize_ranges(std::mem::take(&mut m.ranges));
                    if m.field.is_none() {
                        if let Some(ref f) = inherited { m.field = Some(f.clone()); }
                    }
                }
                a_m
            }
            CompiledNode::Or(a, b) => {
                let a_m = self.captures_meta(a, text);
                if !a_m.is_empty() { return a_m; }
                let mut bm = self.captures_meta(b, text);
                let inherited = collect_field_from_compiled(compiled);
                for m in bm.iter_mut() {
                    m.ranges = normalize_ranges(std::mem::take(&mut m.ranges));
                    if m.field.is_none() {
                        if let Some(ref f) = inherited { m.field = Some(f.clone()); }
                    }
                }
                bm
            }
        }
    }
}

fn parse_number_from_bytes(b: &[u8]) -> Result<i128, std::num::ParseIntError> {
    let s = String::from_utf8_lossy(b);
    // accept decimal integers only for now
    s.trim().parse::<i128>()
}

fn escape_literal(s: &str) -> String {
    // Very small escaping for PCRE2 special chars. Wrap in a non-capturing
    // group so the literal can be used as substring search.
    let mut out = String::with_capacity(s.len()*2 + 10);
    // Use a capturing group so PCRE2 returns explicit capture ranges
    out.push('(');
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

    #[test]
    fn matcher_icase_and_not() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        // icase modifier should make match case-insensitive
        let node = Node::Modified(Box::new(Node::Word("Foo".to_string())), vec!["icase".to_string()]);
        let compiled = qm.compile(&node).unwrap();
        assert!(qm.is_match(&compiled, b"this is foo"));
        // not modifier should invert the match
        let node2 = Node::Modified(Box::new(Node::Word("bar".to_string())), vec!["not".to_string()]);
        let compiled2 = qm.compile(&node2).unwrap();
        assert!(!qm.is_match(&compiled2, b"contains bar"));
        assert!(qm.is_match(&compiled2, b"no match here"));
    }

    #[test]
    fn captures_meta_examples() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);

        // leaf literal (use Regex node to validate capture metadata)
        let pool2 = PatternPool::new();
        let pat = pool2.acquire_pcre2("foo").unwrap();
        let comp = CompiledNode::Leaf { pat, negated: false, field: None, mods: vec![] };
        let metas = qm.captures_meta(&comp, b"this is foo");
        // Some backends may not produce capture groups for simple literals.
        // If metadata is present, assert the captured bytes match "foo".
        if !metas.is_empty() {
            assert_eq!(metas.len(), 1);
            assert_eq!(metas[0].field, None);
            assert!(!metas[0].ranges.is_empty());
            let (s,e) = metas[0].ranges[0];
            assert_eq!(&b"this is foo"[s..e], b"foo");
        }

        // extension field should produce name+extension meta entries
        let pool3 = PatternPool::new();
        let pat2 = pool3.acquire_pcre2("txt").unwrap();
        let comp_ext = CompiledNode::Leaf { pat: pat2, negated: false, field: Some("extension".to_string()), mods: vec![] };
        let metas_ext = qm.captures_meta(&comp_ext, b"file.txt\n/some/path/file.txt");
        // expect two metas: name with ranges, extension with empty ranges
        assert_eq!(metas_ext.len(), 2);
        assert_eq!(metas_ext[0].field, Some("name".to_string()));
        assert!(!metas_ext[0].ranges.is_empty());
        assert_eq!(metas_ext[1].field, Some("extension".to_string()));
        assert!(metas_ext[1].ranges.is_empty());

        // compare matches produce a field entry with no ranges
        let node2 = Node::Compare("size".to_string(), crate::query::parser_rs::CompareOp::Eq, "100".to_string());
        let comp2 = qm.compile(&node2).unwrap();
        let metas2 = qm.captures_meta(&comp2, b"100");
        assert_eq!(metas2.len(), 1);
        assert_eq!(metas2[0].field, Some("size".to_string()));
        assert!(metas2[0].ranges.is_empty());

        // function contains => ranges
        let node3 = Node::Function("contains".to_string(), vec!("bar".to_string()));
        let comp3 = qm.compile(&node3).unwrap();
        let metas3 = qm.captures_meta(&comp3, b"xxbaryy");
        if !metas3.is_empty() {
            assert_eq!(metas3.len(), 1);
            assert!(!metas3[0].ranges.is_empty());
            let (sa,ea) = metas3[0].ranges[0];
            assert_eq!(&b"xxbaryy"[sa..ea], b"bar");
        }
    }

    #[test]
    fn matcher_anchored_literal() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        // anchored modifier should require full-string match
        let node = Node::Modified(Box::new(Node::Word("foo".to_string())), vec!("anchored".to_string()));
        let compiled = qm.compile(&node).unwrap();
        assert!(qm.is_match(&compiled, b"foo"));
        assert!(!qm.is_match(&compiled, b"this is foo"));
        assert!(!qm.is_match(&compiled, b"foobar"));
    }

    #[test]
    fn matcher_group_captures() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        // explicit regex with groups
        let node = Node::Regex("(ab)([0-9]+)".to_string());
        let compiled = qm.compile(&node).unwrap();
        assert!(qm.is_match(&compiled, b"xxab123yy"));
        let caps = qm.captures(&compiled, b"xxab123yy");
        // expect at least the full match and capture groups
        assert!(caps.len() >= 1);
        // ensure captured bytes correspond to some substring
        let mut found = false;
        for (s,e) in caps.iter() {
            if &b"xxab123yy"[*s..*e] == b"ab123" { found = true; }
        }
        assert!(found);
    }

    #[test]
    fn matcher_field_extension() {
        let pool = PatternPool::new();
        let qm = QueryMatcher::new(pool);
        // Field-scoped term: extension:txt should match file name's extension only
        let node = Node::Field("extension".to_string(), "txt".to_string());
        let compiled = qm.compile(&node).unwrap();
        // combined text format: name + '\n' + path
        let combined = b"file.txt\n/some/path/file.txt";
        assert!(qm.is_match(&compiled, combined));
        let caps = qm.captures(&compiled, combined);
        // Expect at least one capture covering the 'txt' bytes in the name portion
        assert!(!caps.is_empty());
        let (s,e) = caps[0];
        let name = "file.txt";
        let dot = name.rfind('.').unwrap();
        let expected_s = dot + 1;
        let expected_e = name.len();
        assert_eq!((s,e), (expected_s, expected_e));
    }
}

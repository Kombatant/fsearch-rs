use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub mtime: u64,
    // normalized: NFKC + lowercase folded representation used for fast comparisons
    pub normalized: String,
}

impl Entry {
    pub fn new(id: u64, path: PathBuf) -> Self {
        let path_str = path.to_string_lossy().into_owned();
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| String::from(""));

        let metadata = std::fs::metadata(&path);
        let (size, mtime) = match metadata {
            Ok(m) => {
                let size = m.len();
                let mtime = m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                (size, mtime)
            }
            Err(_) => (0u64, 0u64),
        };

        // Normalize to NFKC then case-fold via to_lowercase()
        // This approximates Unicode case folding; for full case-fold semantics consider
        // adding a dedicated case-folding crate later.
        let normalized = name.nfkc().collect::<String>().to_lowercase();

        Entry {
            id,
            name,
            path: path_str,
            size,
            mtime,
            normalized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Entry;
    use std::path::PathBuf;

    #[test]
    fn nfkc_and_lowercase_normalization() {
        // 'ﬁ' ligature (U+FB01) should normalize to 'fi' under NFKC
        let tmp = PathBuf::from("/tmp/\u{FB01}file.txt");
        let e = Entry::new(1, tmp);
        assert!(e.normalized.contains("fi"), "normalized='{}'", e.normalized);
    }

    #[test]
    fn case_fold_example() {
        // 'Å' should fold to lowercase 'å'
        let tmp = PathBuf::from("/tmp/Åfile.txt");
        let e = Entry::new(2, tmp);
        assert!(e.normalized.contains("å") || e.normalized.contains("åfile") || e.normalized.contains("åfile.txt"), "normalized='{}'", e.normalized);
    }
}

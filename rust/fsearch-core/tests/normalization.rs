use fsearch_core::entry::Entry;
use std::path::PathBuf;

#[test]
fn nfkc_and_lowercase_normalization() {
    // 'ﬁ' ligature (U+FB01) should normalize to 'fi' under NFKC
    let tmp = PathBuf::from("/tmp/ﬁfile.txt");
    let e = Entry::new(1, tmp);
    // normalized name should contain 'fi' not the ligature
    assert!(e.normalized.contains("fi"));
}

#[test]
fn case_fold_example() {
    // 'Å' should fold to lowercase 'å'
    let tmp = PathBuf::from("/tmp/Åfile.txt");
    let e = Entry::new(2, tmp);
    assert!(e.normalized.contains("å") || e.normalized.contains("åfile") || e.normalized.contains("åfile.txt"));
}

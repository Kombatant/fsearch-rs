use crate::entry::Entry;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Index {
    pub entries: Vec<Entry>,
    next_id: u64,
}

impl Index {
    pub fn new() -> Self {
        Index {
            entries: Vec::new(),
            next_id: 1,
        }
    }

    pub fn build_from_paths(&mut self, paths: Vec<String>) {
        for p in paths.into_iter() {
            let pb = PathBuf::from(p);
            self.visit_path(pb);
        }
    }

    fn visit_path(&mut self, path: PathBuf) {
        if let Ok(md) = std::fs::metadata(&path) {
            if md.is_dir() {
                // iterate directory
                if let Ok(mut iter) = std::fs::read_dir(&path) {
                    while let Some(Ok(entry)) = iter.next() {
                        self.visit_path(entry.path());
                    }
                }
            } else {
                let id = self.next_id;
                self.next_id += 1;
                let e = Entry::new(id, path);
                self.entries.push(e);
            }
        }
    }
}

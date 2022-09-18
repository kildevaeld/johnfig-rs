use std::path::PathBuf;

use super::{BoxIterator, Locator};

pub struct DirLocator(pub PathBuf);

impl Locator for DirLocator {
    type Error = std::io::Error;

    fn root(&self) -> &PathBuf {
        todo!()
    }

    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> Result<BoxIterator<'a>, Self::Error> {
        let iter = DirLocatorIter {
            root: &self.0,
            iter: std::fs::read_dir(&self.0)?,
            patterns: search_names,
        };

        Ok(Box::new(iter.flatten()))
    }
}

pub struct DirLocatorIter<'a> {
    root: &'a PathBuf,
    iter: std::fs::ReadDir,
    patterns: &'a [glob::Pattern],
}

impl<'a> Iterator for DirLocatorIter<'a> {
    type Item = BoxIterator<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let next = match self.iter.next() {
                Some(ret) => ret,
                None => return None,
            };

            let next = match next {
                Ok(ret) => ret,
                Err(_) => continue,
            };

            let meta = match next.metadata() {
                Ok(ret) => ret,
                Err(_) => continue,
            };

            if meta.is_dir() {
                continue;
            }

            let path = next.path();

            let filename = match pathdiff::diff_paths(&path, &self.root) {
                Some(path) => path,
                None => {
                    continue;
                }
            };

            let iter = self.patterns.iter().filter_map(move |pattern| {
                if pattern.matches_path(&filename) {
                    Some(next.path())
                } else {
                    None
                }
            });

            return Some(Box::new(iter));
        }
    }
}

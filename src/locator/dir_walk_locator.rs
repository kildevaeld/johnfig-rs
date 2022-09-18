use crate::Locator;
use std::path::{Path, PathBuf};

pub struct DirWalkLocator {
    root: PathBuf,
    depth: usize,
}

impl DirWalkLocator {
    pub fn new(root: PathBuf, depth: usize) -> std::io::Result<DirWalkLocator> {
        let root = std::fs::canonicalize(root)?;
        Ok(DirWalkLocator { root, depth })
    }
}

impl Locator for DirWalkLocator {
    type Error = std::io::Error;

    fn root(&self) -> &PathBuf {
        &self.root
    }

    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> Result<super::BoxIterator<'a>, Self::Error> {
        let iter = walkdir::WalkDir::new(&self.root).max_depth(self.depth);

        let iter = iter
            .into_iter()
            .filter_map(|ret| ret.ok())
            .filter_map(|item| match item.metadata() {
                Ok(ret) => {
                    if ret.is_file() {
                        Some(item.path().to_path_buf())
                    } else {
                        None
                    }
                }
                Err(_) => None,
            })
            .filter_map(move |path| {
                let file = match path.file_name().map(Path::new) {
                    Some(ret) => ret,
                    None => return None,
                };

                for pattern in search_names {
                    if pattern.matches_path(&file) {
                        return Some(path);
                    }
                }

                None
            });

        Ok(Box::new(iter))
    }
}

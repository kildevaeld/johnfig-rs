use super::error::Error;
use async_trait::async_trait;
use futures::{future::BoxFuture, FutureExt, TryStreamExt};
use log::trace;
use std::path::PathBuf;

#[async_trait]
pub trait Locator: Send + Sync {
    async fn locate(&self, search_names: &[String]) -> Result<Vec<PathBuf>, Error>;
}

pub struct DirLocator(pub PathBuf);

#[async_trait]
impl Locator for DirLocator {
    async fn locate(&self, search_names: &[String]) -> Result<Vec<PathBuf>, Error> {
        Ok(search_names
            .iter()
            .filter_map(|name| {
                let path = self.0.join(name);
                if path.exists() && !path.is_dir() {
                    Some(path)
                } else {
                    None
                }
            })
            .collect())
    }
}

pub struct WalkDirLocator {
    root: PathBuf,
    depth: usize,
}

impl WalkDirLocator {
    pub fn new(root: impl Into<PathBuf>) -> Result<WalkDirLocator, Error> {
        let mut path = root.into();
        if !path.is_absolute() {
            path = path.canonicalize()?;
        }

        Ok(WalkDirLocator {
            root: path,
            depth: 0,
        })
    }

    pub fn depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }
}

impl WalkDirLocator {
    fn read_dir<'a>(
        &'a self,
        path: &'a PathBuf,
        search_names: &'a [String],
        output: &'a mut Vec<PathBuf>,
        depth: usize,
    ) -> BoxFuture<'a, Result<(), Error>> {
        async move {
            trace!("enter directory: {:?}", path);
            for search_name in search_names {
                let p = path.join(search_name);
                trace!("trying {:?}", p);

                let p = blocking::unblock(move || {
                    if p.exists() && !p.is_dir() {
                        Some(p)
                    } else {
                        None
                    }
                })
                .await;

                if let Some(p) = p {
                    output.push(p);
                }
            }
            if self.depth == 0 || depth < self.depth {
                let mut read_dir = async_fs::read_dir(path).await?;

                while let Some(next) = read_dir.try_next().await? {
                    let sub_path = next.path();
                    if let Some(sub_path) = blocking::unblock(move || {
                        if sub_path.is_dir() {
                            Some(sub_path)
                        } else {
                            None
                        }
                    })
                    .await
                    {
                        self.read_dir(&sub_path, search_names, output, depth + 1)
                            .await?;
                    }
                }
            }
            trace!("leave directory: {:?}", path);
            Ok(())
        }
        .boxed()
    }
}

#[async_trait]
impl Locator for WalkDirLocator {
    async fn locate(&self, search_names: &[String]) -> Result<Vec<PathBuf>, Error> {
        let mut rets = Vec::new();

        self.read_dir(&self.root, search_names, &mut rets, 0)
            .await?;

        Ok(rets)
    }
}

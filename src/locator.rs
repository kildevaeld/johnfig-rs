use super::error::Error;
use async_stream::try_stream;
use async_trait::async_trait;
use futures::{stream::BoxStream, StreamExt, TryStreamExt};
use std::path::{Path, PathBuf};

pub trait Locator: Send + Sync {
    fn root(&self) -> &PathBuf;
    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> BoxStream<'a, Result<PathBuf, Error>>;
}

pub struct DirLocator(pub PathBuf);

#[async_trait]
impl Locator for DirLocator {
    fn root(&self) -> &PathBuf {
        &self.0
    }
    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> BoxStream<'a, Result<PathBuf, Error>> {
        try_stream! {
            let mut readir = async_fs::read_dir(&self.0)
            .await?;

            while let Some(next) = readir.try_next().await? {

                let path = next.path();

                for pat in search_names {

                    let filename = path.file_name().unwrap();
                    if pat.matches_path(Path::new(filename)) {
                        yield path;
                        break
                    }
                }
            }

        }
        .boxed()

        // unimplemented!()
    }
}

/*
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

// #[async_trait]
// impl Locator for WalkDirLocator {
//     async fn locate(&self, search_names: &[String]) -> Result<Vec<PathBuf>, Error> {
//         let mut rets = Vec::new();

//         self.read_dir(&self.root, search_names, &mut rets, 0)
//             .await?;

//         Ok(rets)
//     }
// }

*/

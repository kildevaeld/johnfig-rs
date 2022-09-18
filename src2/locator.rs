use super::error::Error;
use async_stream::try_stream;
use async_trait::async_trait;
use brunson::{Backend, BoxStream, DirEntry, Runtime, FS};
use futures_lite::{pin, StreamExt};
use std::path::{Path, PathBuf};

pub trait Locator<B: Backend>: Send + Sync {
    fn root(&self) -> &PathBuf;
    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> BoxStream<'a, Result<PathBuf, Error>>;
}

pub struct DirLocator(pub PathBuf);

#[async_trait]
impl<B: Backend> Locator<B> for DirLocator {
    fn root(&self) -> &PathBuf {
        &self.0
    }
    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> BoxStream<'a, Result<PathBuf, Error>> {
        try_stream! {
            let readir = B::FS::read_dir(&self.0)
            .await?;

            pin!(readir);

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
    fn read_dir<'a, B: Backend>(
        &'a self,
        path: &'a PathBuf,
        search_names: &'a [glob::Pattern],
        depth: usize,
    ) -> BoxStream<'a, Result<PathBuf, Error>>
    where
        <<B as Backend>::Runtime as Runtime>::Error: Send,
    {
        try_stream! {

            let locator = DirLocator(path.clone());

            while let Some(next) = <DirLocator as Locator<B>>::locate(&locator,search_names).try_next().await? {
                yield next;
            }

            if self.depth == 0 || depth < self.depth {
                let read_dir = B::FS::read_dir(path).await?;
                pin!(read_dir);
                while let Some(Ok(next)) = read_dir.next().await {
                    let sub_path = next.path();
                    if let Ok(Some(sub_path)) = B::runtime()
                        .unblock(move || {
                            if sub_path.is_dir() {
                                Some(sub_path)
                            } else {
                                None
                            }
                        })
                        .await
                    {
                        while let Some(next) = self.read_dir::<B>(&sub_path, search_names, depth + 1).try_next().await? {
                            yield next
                        }
                    }
                }
            }


        }
        .boxed()
        // let future = async move {
        //     log::trace!("enter directory: {:?}", path);
        //     // for search_name in search_names {
        //     //     let p = path.join(search_name);
        //     //     log::trace!("trying {:?}", p);

        //     //     let p = B::runtime()
        //     //         .unblock(move || {
        //     //             if p.exists() && !p.is_dir() {
        //     //                 Some(p)
        //     //             } else {
        //     //                 None
        //     //             }
        //     //         })
        //     //         .await;

        //     //     if let Ok(Some(p)) = p {
        //     //         output.push(p);
        //     //     }
        //     // }

        // if self.depth == 0 || depth < self.depth {
        //     let mut read_dir = B::FS::read_dir(path).await?;
        //     pin!(read_dir);
        //     while let Some(Ok(next)) = read_dir.next().await {
        //         let sub_path = next.path();
        //         if let Ok(Some(sub_path)) = B::runtime()
        //             .unblock(move || {
        //                 if sub_path.is_dir() {
        //                     Some(sub_path)
        //                 } else {
        //                     None
        //                 }
        //             })
        //             .await
        //         {
        //             self.read_dir::<B>(&sub_path, search_names, output, depth + 1)
        //                 .await?;
        //         }
        //     }
        // }
        //     log::trace!("leave directory: {:?}", path);
        //     Ok(())
        // };

        // Box::pin(future)
        // todo!()
    }
}

#[async_trait]
impl<B: Backend> Locator<B> for WalkDirLocator
where
    <<B as Backend>::Runtime as Runtime>::Error: Send,
{
    fn root(&self) -> &PathBuf {
        &self.root
    }

    fn locate<'a>(
        &'a self,
        search_names: &'a [glob::Pattern],
    ) -> BoxStream<'a, Result<PathBuf, Error>> {
        self.read_dir::<B>(&self.root, search_names, 0)
    }
}

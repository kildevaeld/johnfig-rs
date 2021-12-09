use super::{
    encoder::{Encoder, Loader, LoaderBuilder},
    error::*,
    locator::*,
};
use futures::{Stream, StreamExt, TryStreamExt};
use serde::Serialize;
use std::{cmp::Ordering, collections::HashSet, path::PathBuf};

#[derive(Serialize)]
struct Context<'a> {
    name: &'a str,
    ext: &'a str,
}

pub struct ConfigFinderBuilder {
    pub(crate) search_paths: Vec<Box<dyn Locator>>,
    pub(crate) search_names: Vec<String>,
}

impl ConfigFinderBuilder {
    pub fn new() -> ConfigFinderBuilder {
        ConfigFinderBuilder {
            search_paths: Vec::new(),
            search_names: Vec::new(),
        }
    }

    pub fn with_name(mut self, name: impl ToString) -> Self {
        self.search_names.push(name.to_string());
        self
    }

    pub fn with_current_path(self) -> Result<Self, Error> {
        let cwd = std::env::current_dir()?;
        self.with_search_path(cwd)
    }

    pub fn with_search_path(self, path: impl Into<PathBuf>) -> Result<Self, Error> {
        let mut path = path.into();

        if !path.is_absolute() {
            path = path.canonicalize()?;
        }

        Ok(self.with_locator(DirLocator(path)))
    }

    pub fn with_locator<L: Locator + 'static>(mut self, locator: L) -> Self {
        self.search_paths.push(Box::new(locator));
        self
    }

    pub fn build(self) -> Result<ConfigFinder, Error> {
        Ok(ConfigFinder {
            search_paths: self.search_paths,
            search_names: self.search_names,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ConfigFile<T> {
    pub config: T,
    pub path: PathBuf,
}

impl<T> std::ops::Deref for ConfigFile<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.config
    }
}

impl<T> std::ops::DerefMut for ConfigFile<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.config
    }
}

pub struct ConfigFinder {
    search_names: Vec<String>,
    search_paths: Vec<Box<dyn Locator>>,
}

impl ConfigFinder {
    pub fn files<'a>(&'a self) -> impl Stream<Item = Result<std::path::PathBuf, Error>> + 'a {
        let mut seen = HashSet::<PathBuf>::default();
        futures::stream::iter(self.search_paths.iter())
            .then(move |search_path| async move {
                let paths = search_path.locate(&self.search_names).await?;

                Result::<_, Error>::Ok(futures::stream::iter(
                    paths.into_iter().map(Result::<_, Error>::Ok),
                ))
            })
            .try_flatten()
            .try_filter_map(move |val| {
                if seen.contains(&val) {
                    futures::future::ok(None)
                } else {
                    seen.insert(val.clone());
                    futures::future::ok(Some(val))
                }
            })
    }
}

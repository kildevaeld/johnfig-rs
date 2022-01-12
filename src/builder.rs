use super::config::Config;
use crate::{locator::Locator, DirLocator, Error};
use brunson::{Backend, FS};
use futures_lite::{pin, Stream, StreamExt};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use toback::{Encoder, Toback, TobackBuilder};

use value::{merge, Map, Value};

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

#[derive(serde::Serialize)]
struct Context {
    ext: String,
}

pub struct ConfigBuilder<B: Backend> {
    loader: TobackBuilder<Map>,
    search_paths: Vec<Box<dyn Locator<B>>>,
    search_names: Vec<String>,
    sort: Option<Box<dyn FnMut(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    filter: Option<Box<dyn Fn(&PathBuf) -> bool + Send + Sync>>,
}

impl<B: Backend + 'static> ConfigBuilder<B> {
    pub fn new() -> ConfigBuilder<B> {
        ConfigBuilder {
            loader: TobackBuilder::new(),
            search_paths: Vec::default(),
            search_names: Vec::default(),
            sort: None,
            filter: None,
        }
    }

    pub fn with_name_pattern(mut self, pattern: impl ToString) -> Self {
        self.search_names.push(pattern.to_string());
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

    pub fn with_locator<L: Locator<B> + 'static>(mut self, locator: L) -> Self {
        self.search_paths.push(Box::new(locator));
        self
    }

    pub fn with_encoder<L: Encoder<Map> + 'static>(mut self, encoder: L) -> Self {
        self.loader = self.loader.with_encoder(encoder);
        self
    }

    pub fn with_sorting<F: 'static + FnMut(&PathBuf, &PathBuf) -> Ordering + Send + Sync>(
        mut self,
        sort: F,
    ) -> Self {
        self.sort = Some(Box::new(sort));
        self
    }

    pub fn with_filter<F: 'static + Fn(&PathBuf) -> bool + Send + Sync>(
        mut self,
        filter: F,
    ) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    pub async fn build_config(self) -> Result<Config, Error> {
        self.build()?.config().await
    }

    pub fn build(self) -> Result<ConfigFinder<B>, Error> {
        self.build_with(|ext| Context {
            ext: ext.to_string(),
        })
    }

    pub fn build_with<C: Serialize, F: Fn(&str) -> C>(
        self,
        create_ctx: F,
    ) -> Result<ConfigFinder<B>, Error> {
        let mut templates = tinytemplate::TinyTemplate::new();

        let search_names = self.search_names;

        for search_name in &search_names {
            templates
                .add_template(&search_name, &search_name)
                .expect("");
        }

        let loader = Arc::new(self.loader.build());

        let search_names = loader
            .extensions()
            .iter()
            .map(|ext| {
                let ctx = create_ctx(ext);
                search_names
                    .iter()
                    .map(|m| {
                        templates
                            .render(m, &ctx)
                            .map_err(|err| Error::Unknown(Box::new(err)))
                    })
                    .collect::<Vec<_>>()
            })
            .flatten()
            .collect::<Result<Vec<_>, Error>>()?;

        let patterns = search_names
            .iter()
            .map(|p| glob::Pattern::new(p).unwrap())
            .collect::<Vec<_>>();

        Ok(ConfigFinder(Arc::new(ConfigFinderInner {
            patterns,
            locators: self.search_paths,
            loader,
            filter: self.filter,
        })))
    }
}

pub(crate) struct ConfigFinderInner<B: Backend> {
    patterns: Vec<glob::Pattern>,
    pub locators: Vec<Box<dyn Locator<B>>>,
    loader: Arc<Toback<Map>>,
    filter: Option<Box<dyn Fn(&PathBuf) -> bool + Send + Sync>>,
}

pub struct ConfigFinder<B: Backend>(pub(crate) Arc<ConfigFinderInner<B>>);

impl<B: Backend> Clone for ConfigFinder<B> {
    fn clone(&self) -> Self {
        ConfigFinder(self.0.clone())
    }
}

impl<B: Backend + 'static> ConfigFinder<B> {
    pub fn files<'a>(&'a self) -> impl Stream<Item = PathBuf> + 'a {
        find_files(&self.0.locators, &self.0.patterns).filter_map(|ret| ret.ok())
    }

    pub(crate) fn config_files<'a>(
        &'a self,
    ) -> impl Stream<Item = Result<ConfigFile<Map>, Error>> + 'a + Send {
        self.files()
            .filter_map(|search_path| {
                if let Some(filter) = &self.0.filter {
                    if filter(&search_path) {
                        Some(search_path)
                    } else {
                        None
                    }
                } else {
                    Some(search_path)
                }
            })
            .then(move |search_path| async move {
                let ext = match search_path.extension() {
                    Some(ext) => ext.to_string_lossy(),
                    None => {
                        println!("no extension");
                        "json".into()
                    }
                };

                let data = B::FS::read(&search_path).await?;

                let out = self.0.loader.load(data, &ext)?;

                Result::<_, Error>::Ok(ConfigFile {
                    config: out,
                    path: search_path,
                })
            })
    }

    pub async fn config(&self) -> Result<Config, Error> {
        // let mut configs = self.config_files().collect::<Vec<_>>().await?;
        let mut configs = Vec::default();

        let stream = self.config_files();
        pin!(stream);

        while let Some(config) = stream.next().await {
            configs.push(config?);
        }

        configs.sort_by(|a, b| a.path.cmp(&b.path));

        let files = configs.iter().map(|m| m.path.clone()).collect();

        Ok(Config {
            inner: merge_config(configs),
            files,
        })
    }

    pub fn matches(&self, path: &Path) -> bool {
        let path = path.file_name().unwrap();
        for pattern in &self.0.patterns {
            if pattern.matches_path(Path::new(path)) {
                return true;
            }
        }
        false
    }

    pub fn matche_any(&self, paths: &[PathBuf]) -> bool {
        for path in paths {
            if self.matches(path) {
                return true;
            }
        }
        false
    }

    #[cfg(feature = "watch")]
    pub fn watch(&self) -> impl Stream<Item = Result<Config, Error>> + Send {
        use crate::watch::watch;
        watch(self.clone())
    }

    #[cfg(feature = "watch")]
    pub async fn watchable_config<R: brunson::Runtime>(
        &self,
        runtime: R,
    ) -> crate::watch::WatchableConfig<B> {
        crate::watch::WatchableConfig::<B>::new(runtime, self.clone()).await
    }
}

fn merge_config(files: Vec<ConfigFile<Map>>) -> BTreeMap<String, Value> {
    let mut config = BTreeMap::default();

    for file in files.into_iter() {
        for (key, value) in file.config.into_iter() {
            if !config.contains_key(&key) {
                config.insert(key, value);
            } else {
                let mut prev = config.get_mut(&key).unwrap();
                merge(&mut prev, value);
            }
        }
    }

    config
}

pub fn find_files<'a, B: Backend>(
    locators: &'a [Box<dyn Locator<B>>],
    patterns: &'a [glob::Pattern],
) -> impl Stream<Item = Result<std::path::PathBuf, Error>> + 'a {
    let mut seen = HashSet::<PathBuf>::default();
    futures_lite::stream::iter(locators.iter())
        .then(move |search_path| async move { search_path.locate(patterns) })
        .flatten()
        .filter_map(move |val| {
            let val = match val {
                Ok(val) => val,
                Err(err) => return Some(Err(err)),
            };

            if seen.contains(&val) {
                None
            } else {
                seen.insert(val.clone());
                Some(Ok(val))
            }
        })
}

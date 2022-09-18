use crate::config::Config;
use crate::locator::locatorbox;
use crate::{
    locator::{BoxLocator, DirLocator, Locator},
    Error,
};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use toback::{Encoder, Toback, TobackBuilder};

use value::{merge, Map, Value};

use super::config_file::ConfigFile;

#[derive(serde::Serialize)]
struct Context {
    ext: String,
}

pub struct ConfigBuilder {
    loader: TobackBuilder<Map>,
    search_paths: Vec<BoxLocator>,
    search_names: Vec<String>,
    sort: Option<Box<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    filter: Option<Box<dyn Fn(&PathBuf) -> bool + Send + Sync>>,
}

impl ConfigBuilder {
    pub fn new() -> ConfigBuilder {
        ConfigBuilder {
            loader: TobackBuilder::default(),
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

    pub fn with_locator<L: Locator + 'static>(mut self, locator: L) -> Self
    where
        L::Error: std::error::Error + 'static,
        L: Send + Sync,
    {
        self.search_paths.push(locatorbox(locator));
        self
    }

    pub fn with_encoder<L: Encoder<Map> + 'static>(mut self, encoder: L) -> Self {
        self.loader = self.loader.encoder(encoder);
        self
    }

    pub fn with_sorting<F: 'static + Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>(
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
        self.build()?.config()
    }

    pub fn build(self) -> Result<ConfigFinder, Error> {
        self.build_with(|ext| Context {
            ext: ext.to_string(),
        })
    }

    pub fn build_with<C: Serialize, F: Fn(&str) -> C>(
        self,
        create_ctx: F,
    ) -> Result<ConfigFinder, Error> {
        let mut templates = tinytemplate::TinyTemplate::new();

        let search_names = self.search_names;

        for search_name in &search_names {
            templates
                .add_template(&search_name, &search_name)
                .expect("");
        }

        let loader = Arc::new(self.loader.build());

        log::debug!("loaders registered: {:?}", loader.extensions());

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

        log::debug!("using search names: {:?}", search_names);

        let patterns = search_names
            .iter()
            .map(|p| glob::Pattern::new(p).unwrap())
            .collect::<Vec<_>>();

        Ok(ConfigFinder(Arc::new(ConfigFinderInner {
            patterns,
            locators: self.search_paths,
            loader,
            filter: self.filter,
            sorter: self.sort,
        })))
    }
}

pub(crate) struct ConfigFinderInner {
    patterns: Vec<glob::Pattern>,
    pub locators: Vec<BoxLocator>,
    loader: Arc<Toback<Map>>,
    filter: Option<Box<dyn Fn(&PathBuf) -> bool + Send + Sync>>,
    sorter: Option<Box<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
}

#[derive(Clone)]
pub struct ConfigFinder(pub(crate) Arc<ConfigFinderInner>);

impl ConfigFinder {
    pub fn files<'a>(&'a self) -> impl Iterator<Item = PathBuf> + 'a {
        find_files(&self.0.locators, &self.0.patterns)
    }

    pub fn config_files<'a>(&'a self) -> impl Iterator<Item = Result<ConfigFile<Map>, Error>> + 'a {
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
            .map(move |search_path| {
                let ext = match search_path.extension() {
                    Some(ext) => ext.to_string_lossy(),
                    None => {
                        println!("no extension");
                        "json".into()
                    }
                };

                let data = std::fs::read(&search_path)?;

                let out = self.0.loader.load(&data, &ext)?;

                log::trace!("found path: {:?}", search_path);

                Result::<_, Error>::Ok(ConfigFile {
                    config: out,
                    path: search_path,
                })
            })
    }

    pub fn config(&self) -> Result<Config, Error> {
        let mut configs = Vec::default();

        let mut stream = self.config_files();

        while let Some(config) = stream.next() {
            configs.push(config?);
        }

        if let Some(sorter) = &self.0.sorter {
            configs.sort_by(|a, b| sorter(&a.path, &b.path));
        } else {
            configs.sort_by(|a, b| a.path.cmp(&b.path));
        }

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

pub fn find_files<'a>(
    locators: &'a [BoxLocator],
    patterns: &'a [glob::Pattern],
) -> impl Iterator<Item = std::path::PathBuf> + 'a {
    let mut seen = HashSet::<PathBuf>::default();
    locators
        .iter()
        .map(move |search_path| search_path.locate(patterns))
        .filter_map(|item| match item {
            Ok(ret) => Some(ret),
            Err(_) => {
                println!("Got ERRRR");
                None
            }
        })
        .flatten()
        .filter_map(move |val| {
            if seen.contains(&val) {
                None
            } else {
                seen.insert(val.clone());
                Some(val)
            }
        })
}
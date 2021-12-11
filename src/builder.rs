use super::config::Config;
use crate::{locator::Locator, DirLocator, Encoder, Error, Loader, LoaderBuilder};
use futures::{Stream, StreamExt, TryStreamExt};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use value::{merge, Value};

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

pub struct ConfigBuilder {
    loader: LoaderBuilder<BTreeMap<String, Value>>,
    search_paths: Vec<Box<dyn Locator>>,
    search_names: Vec<String>,
    sort: Option<Box<dyn FnMut(&PathBuf, &PathBuf) -> Ordering>>,
}

impl ConfigBuilder {
    pub fn new() -> ConfigBuilder {
        ConfigBuilder {
            loader: LoaderBuilder::new(),
            search_paths: Vec::default(),
            search_names: Vec::default(),
            sort: None,
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

    pub fn with_locator<L: Locator + 'static>(mut self, locator: L) -> Self {
        self.search_paths.push(Box::new(locator));
        self
    }

    pub fn with_encoder<L: Encoder<BTreeMap<String, Value>> + 'static>(
        mut self,
        encoder: L,
    ) -> Self {
        self.loader = self.loader.with_encoder(encoder);
        self
    }

    pub fn with_sorting<F: 'static + FnMut(&PathBuf, &PathBuf) -> Ordering>(
        mut self,
        sort: F,
    ) -> Self {
        self.sort = Some(Box::new(sort));
        self
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
        })))
    }
}

struct ConfigFinderInner {
    patterns: Vec<glob::Pattern>,
    locators: Vec<Box<dyn Locator>>,
    loader: Arc<Loader<BTreeMap<String, Value>>>,
}

#[derive(Clone)]
pub struct ConfigFinder(Arc<ConfigFinderInner>);

impl ConfigFinder {
    pub fn files<'a>(&'a self) -> impl Stream<Item = PathBuf> + 'a {
        find_files(&self.0.locators, &self.0.patterns).filter_map(|ret| async move { ret.ok() })
    }

    pub(crate) fn config_files<'a>(
        &'a self,
    ) -> impl Stream<Item = Result<ConfigFile<BTreeMap<String, Value>>, Error>> + 'a {
        self.files().then(move |search_path| async move {
            let ext = match search_path.extension() {
                Some(ext) => ext.to_string_lossy(),
                None => {
                    println!("no extension");
                    "json".into()
                }
            };

            let data = async_fs::read(&search_path).await?;

            let out = self.0.loader.load(data, &ext)?;

            Result::<_, Error>::Ok(ConfigFile {
                config: out,
                path: search_path,
            })
        })
    }

    pub async fn config(&self) -> Result<Config, Error> {
        let mut configs = self.config_files().try_collect::<Vec<_>>().await?;

        configs.sort_by(|a, b| a.path.cmp(&b.path));

        let files = configs.iter().map(|m| m.path.clone()).collect();

        Ok(Config {
            inner: merge_config(configs),
            files,
        })
    }
}

fn merge_config(files: Vec<ConfigFile<BTreeMap<String, Value>>>) -> BTreeMap<String, Value> {
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
    locators: &'a [Box<dyn Locator>],
    patterns: &'a [glob::Pattern],
) -> impl Stream<Item = Result<std::path::PathBuf, Error>> + 'a {
    let mut seen = HashSet::<PathBuf>::default();
    futures::stream::iter(locators.iter())
        .then(move |search_path| async move { search_path.locate(patterns) })
        .flatten()
        .try_filter_map(move |val| {
            if seen.contains(&val) {
                futures::future::ok(None)
            } else {
                seen.insert(val.clone());
                futures::future::ok(Some(val))
            }
        })
}

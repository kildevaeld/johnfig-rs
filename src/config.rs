use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use futures::{Stream, StreamExt, TryStreamExt};
use serde::Serialize;

use crate::{
    find,
    locator::Locator,
    value::{merge, Value},
    DirLocator, Encoder, Error, Loader, LoaderBuilder,
};

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

    pub async fn build(self) -> Result<Config, Error> {
        self.build_with(|ext| Context {
            ext: ext.to_string(),
        })
        .await
    }

    pub async fn build_with<C: Serialize, F: Fn(&str) -> C>(
        self,
        create_ctx: F,
    ) -> Result<Config, Error> {
        let mut templates = tinytemplate::TinyTemplate::new();

        let search_names = self.search_names;

        for search_name in &search_names {
            templates
                .add_template(&search_name, &search_name)
                .expect("");
        }

        let loader = self.loader.build();

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

        let files = find::ConfigFinderBuilder {
            search_names,
            search_paths: self.search_paths,
        }
        .build()?;

        let mut configs = load_configs(&loader, &files)
            .try_collect::<Vec<_>>()
            .await?;

        if let Some(mut sort) = self.sort {
            configs.sort_by(|a, b| sort(&a.path, &b.path));
        }

        let files = configs.iter().map(|m| m.path.clone()).collect();

        Ok(Config {
            inner: merge_config(configs),
            files,
        })
    }
}

fn load_configs<'a>(
    loader: &'a Loader<BTreeMap<String, Value>>,
    config_finder: &'a find::ConfigFinder,
) -> impl Stream<Item = Result<ConfigFile<BTreeMap<String, Value>>, Error>> + 'a {
    config_finder
        .files()
        .and_then(move |search_path| async move {
            let ext = match search_path.extension() {
                Some(ext) => ext.to_string_lossy(),
                None => {
                    println!("no extension");
                    "json".into()
                }
            };

            let data = async_fs::read(&search_path).await?;

            let out = loader.load(data, &ext)?;

            Result::<_, Error>::Ok(ConfigFile {
                config: out,
                path: search_path,
            })
        })
}
#[derive(Debug, Default, Clone)]
pub struct Config {
    inner: BTreeMap<String, Value>,
    files: Vec<PathBuf>,
}

impl Config {
    pub fn get<K>(&self, name: impl AsRef<str>) -> Option<&Value> {
        self.inner.get(name.as_ref())
    }

    pub fn get_mut<K>(&mut self, name: impl AsRef<str>) -> Option<&mut Value> {
        self.inner.get_mut(name.as_ref())
    }

    pub fn set(&mut self) -> Option<&Value> {
        None
    }
}

impl<S: AsRef<str>> std::ops::Index<S> for Config {
    type Output = Value;
    fn index(&self, idx: S) -> &Self::Output {
        static NULL: Value = Value::Option(None);
        self.inner.get(idx.as_ref()).unwrap_or(&NULL)
    }
}

impl<S: AsRef<str>> std::ops::IndexMut<S> for Config {
    fn index_mut(&mut self, idx: S) -> &mut Self::Output {
        if !self.inner.contains_key(idx.as_ref()) {
            self.inner
                .insert(idx.as_ref().to_owned(), Value::Option(None));
        }

        self.inner.get_mut(idx.as_ref()).unwrap()
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

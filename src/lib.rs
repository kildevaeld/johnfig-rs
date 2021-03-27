mod encoder;
mod error;
mod locator;
use futures::{Stream, StreamExt, TryStreamExt};
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

pub use self::{
    encoder::{Encoder, Loader, LoaderBuilder},
    error::*,
    locator::*,
};

#[derive(Serialize)]
struct Context<'a> {
    name: &'a str,
    ext: &'a str,
}

pub struct ConfigBuilder<T: Serialize + DeserializeOwned> {
    loader: LoaderBuilder<T>,
    search_paths: Vec<Box<dyn Locator>>,
    search_names: Vec<String>,
    name: String,
}

impl<T: Serialize + DeserializeOwned> ConfigBuilder<T> {
    pub fn new(name: impl ToString) -> ConfigBuilder<T> {
        ConfigBuilder {
            loader: LoaderBuilder::new(),
            search_paths: Vec::new(),
            name: name.to_string(),
            search_names: Vec::new(),
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

    pub fn with_home_path(self) -> Result<Self, Error> {
        if let Some(home) = dirs::home_dir() {
            self.with_search_path(home)
        } else {
            Ok(self)
        }
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

    pub fn with_encoder<L: Encoder<T> + 'static>(mut self, encoder: L) -> Self {
        self.loader = self.loader.with_encoder(encoder);
        self
    }

    pub fn build(mut self) -> Result<Config<T>, Error> {
        self.search_names.extend(vec![
            ".{name}rc.{ext}".to_string(),
            "{name}.config.{ext}".to_string(),
        ]);

        let mut templates = tinytemplate::TinyTemplate::new();

        let search_names = self.search_names;
        let name = self.name;

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
                let ctx = Context {
                    name: &name,
                    ext: &ext,
                };
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

        Ok(Config {
            inner: Arc::new(ConfigInner {
                loader: loader,
                search_paths: self.search_paths,
                search_names,
            }),
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

struct ConfigInner<T: Serialize + DeserializeOwned> {
    loader: Loader<T>,
    search_names: Vec<String>,
    search_paths: Vec<Box<dyn Locator>>,
}

#[derive(Clone)]
pub struct Config<T: Serialize + DeserializeOwned> {
    inner: Arc<ConfigInner<T>>,
}

impl<T: Serialize + DeserializeOwned> Config<T> {
    fn load_all2<'a>(&'a self) -> impl Stream<Item = Result<ConfigFile<T>, Error>> + 'a {
        futures::stream::iter(self.inner.search_paths.iter())
            .then(move |search_path| async move {
                let paths = search_path.locate(&self.inner.search_names).await?;

                Result::<_, Error>::Ok(futures::stream::iter(paths.into_iter()).then(
                    move |path| async move {
                        let ext = match path.extension() {
                            Some(ext) => ext.to_string_lossy(),
                            None => {
                                println!("no extension");
                                "json".into()
                            }
                        };

                        let data = async_fs::read(&path).await?;

                        let out = self.inner.loader.load(data, &ext)?;

                        return Result::<_, Error>::Ok(ConfigFile {
                            config: out,
                            path: path,
                        });
                    },
                ))
            })
            .try_flatten()
    }

    pub async fn load_all(&self, ignore_errors: bool) -> Result<Vec<ConfigFile<T>>, Error> {
        if ignore_errors {
            Ok(self
                .load_all2()
                .filter_map(|m| async move {
                    match m {
                        Ok(ret) => Some(ret),
                        Err(_) => None,
                    }
                })
                .collect()
                .await)
        } else {
            Ok(self.load_all2().try_collect().await?)
        }
    }

    pub async fn load(&self) -> Result<ConfigFile<T>, Error> {
        let mut found: Vec<_> = self.load_all2().take(1).try_collect().await?;

        match found.pop() {
            Some(found) => Ok(found),
            None => Err(Error::NotFound),
        }
    }
}

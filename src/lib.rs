use async_trait::async_trait;
use futures::{future::BoxFuture, FutureExt, Stream, StreamExt, TryStreamExt};
use log::trace;
use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error as ThisError;

pub trait Loader<T: Serialize + DeserializeOwned> {
    fn extensions(&self) -> &[&str];
    fn load(&self, path: &Path, content: Vec<u8>) -> Result<T, Error>;
    fn save(&self, content: &T) -> Result<Vec<u8>, Error>;
}

#[async_trait]
pub trait Locator {
    async fn locate(&self, search_names: &[String]) -> Result<Vec<PathBuf>, Error>;
}

#[derive(Serialize)]
struct Context<'a> {
    name: &'a str,
    ext: &'a str,
}

pub struct ConfigBuilder<T: Serialize + DeserializeOwned> {
    loaders: Vec<Box<dyn Loader<T>>>,
    search_paths: Vec<Box<dyn Locator>>,
    search_names: Vec<String>,
    name: String,
}

impl<T: Serialize + DeserializeOwned> ConfigBuilder<T> {
    pub fn new(name: impl ToString) -> ConfigBuilder<T> {
        ConfigBuilder {
            loaders: Vec::new(),
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

    pub fn with_loader<L: Loader<T> + 'static>(mut self, loader: L) -> Self {
        self.loaders.push(Box::new(loader));
        self
    }

    pub fn build(mut self) -> Result<Config<T>, Error> {
        self.loaders.push(Box::new(JsonLoader));
        #[cfg(feature = "yaml")]
        self.loaders.push(Box::new(YamlLoader));
        #[cfg(feature = "toml")]
        self.loaders.push(Box::new(TomlLoader));
        #[cfg(feature = "ron")]
        self.loaders.push(Box::new(RonLoader));

        self.search_names.extend(vec![
            ".{name}rc.{ext}".to_string(),
            "{name}.config.{ext}".to_string(),
        ]);

        let mut templates = tinytemplate::TinyTemplate::new();

        for search_name in &self.search_names {
            templates
                .add_template(&search_name, &search_name)
                .expect("");
        }

        let search_names = self
            .loaders
            .iter()
            .map(|loader| {
                loader
                    .extensions()
                    .iter()
                    .map(|ext| {
                        let ctx = Context {
                            name: &self.name,
                            ext: &ext,
                        };
                        self.search_names
                            .iter()
                            .map(|m| {
                                templates
                                    .render(m, &ctx)
                                    .map_err(|err| Error::Unknown(Box::new(err)))
                            })
                            .collect::<Vec<_>>()
                    })
                    .flatten()
            })
            .flatten()
            .collect::<Result<Vec<_>, Error>>()?;

        Ok(Config {
            inner: Arc::new(ConfigInner {
                loaders: self.loaders,
                search_paths: self.search_paths,
                search_names,
            }),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ConfigFile<T> {
    config: T,
    path: PathBuf,
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
    loaders: Vec<Box<dyn Loader<T>>>,
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

                Result::<_, Error>::Ok(
                    futures::stream::iter(paths.into_iter().filter_map(move |path| {
                        let ext = match path.extension() {
                            Some(ext) => ext.to_string_lossy(),
                            None => {
                                println!("no extension");
                                return None;
                            }
                        };

                        let loader = self
                            .inner
                            .loaders
                            .iter()
                            .find(|loader| loader.extensions().contains(&ext.as_ref()))
                            .expect("loader");

                        Some((path, loader))
                    }))
                    .then(|(path, loader)| async move {
                        //
                        let data = async_fs::read(&path).await?;

                        let out = loader.load(&path, data)?;

                        return Result::<_, Error>::Ok(ConfigFile {
                            config: out,
                            path: path,
                        });
                    }),
                )
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

#[derive(ThisError, Debug)]
pub enum Error {
    #[error("unknown format")]
    UnknownFormat(String),
    #[error("io")]
    Io(#[from] std::io::Error),
    #[error("config not found")]
    NotFound,
    #[error("serialize")]
    Serialize(#[from] SerializeError),
    #[error("unknonw error")]
    Unknown(Box<dyn std::error::Error>),
}

#[derive(ThisError, Debug)]
#[non_exhaustive]
pub enum SerializeError {
    #[error("json")]
    Json(#[from] serde_json::Error),
    #[cfg(feature = "yaml")]
    #[error("yaml")]
    Yaml(#[from] serde_yaml::Error),
    #[cfg(feature = "toml")]
    #[error("toml")]
    Toml(#[from] TomlError),
    #[cfg(feature = "ron")]
    #[error("ron")]
    Ron(#[from] ron::Error),
}

struct JsonLoader;

impl<T: Serialize + DeserializeOwned> Loader<T> for JsonLoader {
    fn extensions(&self) -> &[&str] {
        &["json"]
    }
    fn load(&self, _path: &Path, content: Vec<u8>) -> Result<T, Error> {
        Ok(serde_json::from_slice::<T>(&content).map_err(SerializeError::Json)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(serde_json::to_vec_pretty(content).map_err(SerializeError::Json)?)
    }
}

#[cfg(feature = "yaml")]
struct YamlLoader;

#[cfg(feature = "yaml")]
impl<T: Serialize + DeserializeOwned> Loader<T> for YamlLoader {
    fn extensions(&self) -> &[&str] {
        &["yaml", "yml"]
    }
    fn load(&self, _path: &Path, content: Vec<u8>) -> Result<T, Error> {
        Ok(serde_yaml::from_slice::<T>(&content).map_err(SerializeError::Yaml)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(serde_yaml::to_vec(content).map_err(SerializeError::Yaml)?)
    }
}

#[cfg(feature = "toml")]
struct TomlLoader;

#[cfg(feature = "toml")]
#[derive(ThisError, Debug)]
pub enum TomlError {
    #[error("serialize")]
    Serialize(toml::ser::Error),
    #[error("deserialize")]
    Deserialize(toml::de::Error),
}

#[cfg(feature = "toml")]
impl<T: Serialize + DeserializeOwned> Loader<T> for TomlLoader {
    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
    fn load(&self, _path: &Path, content: Vec<u8>) -> Result<T, Error> {
        Ok(toml::from_slice::<T>(&content)
            .map_err(TomlError::Deserialize)
            .map_err(SerializeError::Toml)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(toml::to_vec(content)
            .map_err(TomlError::Serialize)
            .map_err(SerializeError::Toml)?)
    }
}

#[cfg(feature = "ron")]
struct RonLoader;

#[cfg(feature = "ron")]
impl<T: Serialize + DeserializeOwned> Loader<T> for RonLoader {
    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
    fn load(&self, _path: &Path, content: Vec<u8>) -> Result<T, Error> {
        let content = String::from_utf8(content).map_err(|err| Error::Unknown(Box::new(err)))?;

        Ok(ron::from_str::<T>(&content).map_err(SerializeError::Ron)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(ron::to_string(content)
            .map(Vec::from)
            .map_err(SerializeError::Ron)?)
    }
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

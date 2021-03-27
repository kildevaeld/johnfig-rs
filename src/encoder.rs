#[cfg(feature = "toml")]
use super::error::TomlError;
use super::error::{Error, SerializeError};
use serde::{de::DeserializeOwned, Serialize};

pub struct LoaderBuilder<T: Serialize + DeserializeOwned> {
    encoders: Vec<Box<dyn Encoder<T>>>,
}

impl<T: Serialize + DeserializeOwned> LoaderBuilder<T> {
    pub fn new() -> LoaderBuilder<T> {
        let mut encoders: Vec<Box<dyn Encoder<T>>> = Vec::new();

        encoders.push(Box::new(JsonEncoder));
        #[cfg(feature = "yaml")]
        encoders.push(Box::new(YamlEncoder));
        #[cfg(feature = "toml")]
        encoders.push(Box::new(TomlEncoder));
        #[cfg(feature = "ron")]
        encoders.push(Box::new(RonEncoder));

        LoaderBuilder { encoders }
    }

    pub fn with_encoder<E: Encoder<T> + 'static>(mut self, encoder: E) -> Self {
        self.encoders.push(Box::new(encoder));
        self
    }

    pub fn build(self) -> Loader<T> {
        Loader {
            encoders: self.encoders,
        }
    }
}

pub struct Loader<T: Serialize + DeserializeOwned> {
    encoders: Vec<Box<dyn Encoder<T>>>,
}

impl<T: Serialize + DeserializeOwned> Loader<T> {
    pub fn new() -> Loader<T> {
        LoaderBuilder::new().build()
    }

    pub fn build() -> LoaderBuilder<T> {
        LoaderBuilder::new()
    }

    pub fn extensions(&self) -> Vec<&str> {
        self.encoders
            .iter()
            .map(|m| m.extensions())
            .flatten()
            .map(|m| *m)
            .collect()
    }

    pub fn load(&self, content: Vec<u8>, ext: &str) -> Result<T, Error> {
        let encoder = self
            .encoders
            .iter()
            .find(|loader| loader.extensions().contains(&ext))
            .expect("loader");

        encoder.load(content)
    }
}

pub trait Encoder<T: Serialize + DeserializeOwned>: Send + Sync {
    fn extensions(&self) -> &[&str];
    fn load(&self, content: Vec<u8>) -> Result<T, Error>;
    fn save(&self, content: &T) -> Result<Vec<u8>, Error>;
}

#[derive(Clone, Copy)]
pub(crate) struct JsonEncoder;

impl<T: Serialize + DeserializeOwned> Encoder<T> for JsonEncoder {
    fn extensions(&self) -> &[&str] {
        &["json"]
    }
    fn load(&self, content: Vec<u8>) -> Result<T, Error> {
        Ok(serde_json::from_slice::<T>(&content).map_err(SerializeError::Json)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(serde_json::to_vec_pretty(content).map_err(SerializeError::Json)?)
    }
}

#[cfg(feature = "yaml")]
#[derive(Clone, Copy)]
pub(crate) struct YamlEncoder;

#[cfg(feature = "yaml")]
impl<T: Serialize + DeserializeOwned> Encoder<T> for YamlEncoder {
    fn extensions(&self) -> &[&str] {
        &["yaml", "yml"]
    }
    fn load(&self, content: Vec<u8>) -> Result<T, Error> {
        Ok(serde_yaml::from_slice::<T>(&content).map_err(SerializeError::Yaml)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(serde_yaml::to_vec(content).map_err(SerializeError::Yaml)?)
    }
}

#[cfg(feature = "toml")]
#[derive(Clone, Copy)]
pub(crate) struct TomlEncoder;

#[cfg(feature = "toml")]
impl<T: Serialize + DeserializeOwned> Encoder<T> for TomlEncoder {
    fn extensions(&self) -> &[&str] {
        &["toml"]
    }
    fn load(&self, content: Vec<u8>) -> Result<T, Error> {
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
#[derive(Clone, Copy)]
pub(crate) struct RonEncoder;

#[cfg(feature = "ron")]
impl<T: Serialize + DeserializeOwned> Encoder<T> for RonEncoder {
    fn extensions(&self) -> &[&str] {
        &["ron"]
    }
    fn load(&self, content: Vec<u8>) -> Result<T, Error> {
        let content = String::from_utf8(content).map_err(|err| Error::Unknown(Box::new(err)))?;

        Ok(ron::from_str::<T>(&content).map_err(SerializeError::Ron)?)
    }
    fn save(&self, content: &T) -> Result<Vec<u8>, Error> {
        Ok(ron::to_string(content)
            .map(Vec::from)
            .map_err(SerializeError::Ron)?)
    }
}

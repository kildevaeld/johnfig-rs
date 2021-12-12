use std::{collections::BTreeMap, path::PathBuf};
use value::{de::DeserializerError, Value};

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub(crate) inner: BTreeMap<String, Value>,
    pub(crate) files: Vec<PathBuf>,
}

impl Config {
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    pub fn get(&self, name: impl AsRef<str>) -> Option<&Value> {
        self.inner.get(name.as_ref())
    }

    pub fn get_mut<K>(&mut self, name: impl AsRef<str>) -> Option<&mut Value> {
        self.inner.get_mut(name.as_ref())
    }

    pub fn try_get<'a, S: serde::Deserialize<'a>>(
        &self,
        name: &str,
    ) -> Result<S, DeserializerError> {
        self.inner[name].clone().try_into()
    }

    pub fn set(&mut self, name: impl ToString, value: impl Into<Value>) -> Option<Value> {
        self.inner.insert(name.to_string(), value.into())
    }

    pub fn contains(&self, name: impl AsRef<str>) -> bool {
        self.inner.contains_key(name.as_ref())
    }
}

impl<S: AsRef<str>> std::ops::Index<S> for Config {
    type Output = Value;
    fn index(&self, idx: S) -> &Self::Output {
        static NULL: Value = Value::None;
        self.inner.get(idx.as_ref()).unwrap_or(&NULL)
    }
}

impl<S: AsRef<str>> std::ops::IndexMut<S> for Config {
    fn index_mut(&mut self, idx: S) -> &mut Self::Output {
        if !self.inner.contains_key(idx.as_ref()) {
            self.inner.insert(idx.as_ref().to_owned(), Value::None);
        }

        self.inner.get_mut(idx.as_ref()).unwrap()
    }
}

impl serde::Serialize for Config {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        Ok(Config {
            inner: BTreeMap::<String, Value>::deserialize(deserializer)?,
            files: Vec::default(),
        })
    }
}

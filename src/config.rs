use std::collections::BTreeMap;
use value::Value;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub(crate) inner: BTreeMap<String, Value>,
    // files: Vec<PathBuf>,
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
        })
    }
}

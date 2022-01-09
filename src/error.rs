use thiserror::Error as ThisError;
use toback::Error as TobackError;
#[derive(ThisError, Debug)]
pub enum Error {
    #[error("unknown format")]
    UnknownFormat(String),
    #[error("io")]
    Io(#[from] std::io::Error),
    #[error("config not found")]
    NotFound,
    #[error("serialize")]
    Serialize(#[from] TobackError),
    #[error("unknonw error")]
    Unknown(Box<dyn std::error::Error + Send + Sync>),
}

// #[derive(ThisError, Debug)]
// #[non_exhaustive]
// pub enum SerializeError {
//     #[error("json")]
//     Json(#[from] serde_json::Error),
//     #[cfg(feature = "yaml")]
//     #[error("yaml")]
//     Yaml(#[from] serde_yaml::Error),
//     #[cfg(feature = "toml")]
//     #[error("toml")]
//     Toml(#[from] TomlError),
//     #[cfg(feature = "ron")]
//     #[error("ron")]
//     Ron(#[from] ron::Error),
// }

// #[cfg(feature = "toml")]
// #[derive(ThisError, Debug)]
// pub enum TomlError {
//     #[error("serialize")]
//     Serialize(toml::ser::Error),
//     #[error("deserialize")]
//     Deserialize(toml::de::Error),
// }

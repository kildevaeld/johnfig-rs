#[cfg(feature = "builder")]
mod builder;
#[cfg(feature = "builder")]
mod error;
#[cfg(feature = "builder")]
mod locator;

mod config;

pub use self::config::Config;

pub use value::{value, Value};

#[cfg(feature = "builder")]
pub use self::{
    builder::{ConfigBuilder, ConfigFinder},
    error::Error,
    locator::{DirLocator, DirWalkLocator, Locator},
};

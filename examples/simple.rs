use futures::TryStreamExt;
use johnfig::{find, value, ConfigBuilder, WalkDirLocator};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig {
    port: u16,
    template_path: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Context<'a> {
    ext: &'a str,
    name: &'a str,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let mut cfg = smol::block_on(async move {
        ConfigBuilder::new()
            .with_locator(WalkDirLocator::new(".")?.depth(1))
            .with_current_path()?
            .with_name_pattern("simple.config.{ext}")
            .with_name_pattern("0-dev.{ext}")
            .with_sorting(|a, b| a.cmp(b))
            .build()
            .await

        // cfg.files().try_collect::<Vec<_>>().await
    })?;

    // cfg.sort();

    cfg["database"] = value!({
        "address": "http://github.com",
        "user": "rasmus"
    });

    println!("Debug {:#?}", cfg);

    Ok(())
}

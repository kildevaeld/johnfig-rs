use johnfig::{value, ConfigBuilder, DirLocator, WalkDirLocator};
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
        let finder = ConfigBuilder::new()
            .with_search_path("./examples")?
            // .with_locator(WalkDirLocator::new(".")?.depth(1))
            .with_current_path()?
            .with_name_pattern("{name}.config.{ext}")
            .with_name_pattern("0-{env}.{ext}")
            .with_sorting(|a, b| a.cmp(b))
            .build_with(|ext| {
                value!({
                    "ext": ext,
                    "env": "dev",
                    "name": "simple"
                })
            })?;

        finder.config().await
    })?;

    // cfg.sort();

    // cfg["database"] = value!({
    //     "address": "http://github.com",
    //     "user": "rasmus"
    // });

    println!("Debug {:#?}", cfg);

    Ok(())
}

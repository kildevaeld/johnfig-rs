use brunson::{Backend, Tokio};
use futures::{pin_mut, StreamExt, TryStreamExt};
use johnfig::{value, ConfigBuilder, DirLocator, Error};
use notify::Watcher;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let mut cfg = {
        let finder = ConfigBuilder::<Tokio>::new()
            .with_search_path("./examples")?
            // .with_locator(WalkDirLocator::new(".")?.depth(1))
            .with_current_path()?
            .with_name_pattern("{name}.config.{ext}")
            .with_name_pattern("*-{env}.{ext}")
            .with_sorting(|a, b| a.cmp(b))
            .build_with(|ext| {
                value!({
                    "ext": ext,
                    "env": "dev",
                    "name": "simple"
                })
            })?;

        let mut watcher = finder.watchable_config(Tokio::runtime()).await;

        // watcher.snapshot().await;

        // while let Some(_) = watcher.listen().next().await {
        //     println!("config changed: {:?}", watcher.snapshot().await);
        // }

        Result::<_, Error>::Ok(watcher.snapshot().await)
    }?;

    cfg["database"] = value!({
        "address": "http://github.com",
        "user": "rasmus"
    });

    println!("Debug {:#?}", cfg);

    Ok(())
}

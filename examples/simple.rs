use johnfig::{value, ConfigBuilder, DirWalkLocator, Error};
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

    let mut cfg = {
        let finder = ConfigBuilder::new()
            .with_locator(DirWalkLocator::new("./examples".into(), 2)?)
            .with_current_path()?
            .with_name_pattern("{name}.config.{ext}")
            .with_name_pattern("*-{env}.{ext}")
            .with_sorting(|a, b| b.cmp(a))
            .with_default(|cfg| {
                cfg["database"] = value!({
                    "address": "http://github.com",
                    "user": "rasmus"
                });
            })
            .build_with(|ext| {
                value!({
                    "ext": ext,
                    "env": "dev",
                    "name": "simple"
                })
            })?;

        finder.config()
    }?;

    println!("Debug {:#?}", cfg);

    Ok(())
}

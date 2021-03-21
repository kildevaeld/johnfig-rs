use johnfig::{ConfigBuilder, WalkDirLocator};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ServerConfig {
    port: u16,
    template_path: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let cfg = smol::block_on(async move {
        let cfg = ConfigBuilder::<ServerConfig>::new("simple")
            .with_locator(WalkDirLocator::new(".")?.depth(1))
            .with_current_path()?
            .build()?;

        cfg.load_all(true).await
    })?;

    println!("Debug {:?}", cfg);

    Ok(())
}

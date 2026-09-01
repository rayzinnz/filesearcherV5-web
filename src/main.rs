use std::path::PathBuf;

use filesearcherv5_web::{router, AppState, Config};
use log::*;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    helper_lib::setup_logger(LevelFilter::Debug, None, "", "html5ever");
    
    info!("starting");

    let config_path = "config.toml";
    let config_str = std::fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_str)?;

    let state = AppState {
        base_dir: PathBuf::from(config.base_dir),
    };

    let app = router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Server running on http://127.0.0.1:{}", config.port);
    axum::serve(listener, app).await?;

    Ok(())
}

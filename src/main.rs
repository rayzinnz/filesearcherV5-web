use filesearcherv5_web::{router, AppState, Config};
use std::path::PathBuf;
use tokio::net::TcpListener;

use axum::body::{Body, Bytes};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::io::AsyncReadExt;
use tokio::process::{Command, Stdio};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("starting");

    let config_path = "config.toml";
    let config_str = std::fs::read_to_string(config_path)?;
    let config: Config = toml::from_str(&config_str)?;

    let state = AppState {
        base_dir: PathBuf::from(config.base_dir),
    };

    let app = router(state).route("/refresh_file_db", get(refresh_file_db_handler));

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = TcpListener::bind(&addr).await?;
    println!("Server running on http://127.0.0.1:{}", config.port);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn refresh_file_db_handler() -> Response {
    const FILE_SEARCHER_DAEMON: &str =
        "/home/ray/MEGA/Rays/Programming/rust/filesearcher-deamon-v5/target/release/file_searcher_deamon_v5";

    let mut child = match Command::new(FILE_SEARCHER_DAEMON)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to spawn file_searcher_deamon_v5: {e}"),
            )
            .into_response();
        }
    };

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to capture stdout from file_searcher_deamon_v5",
            )
            .into_response();
        }
    };

    let (tx, rx) = mpsc::channel::<Vec<u8>>(32);

    tokio::spawn(async move {
        let mut stdout = stdout;
        let mut child = child;
        let mut tx = tx;
        let mut buf = vec![0u8; 8192];

        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("error reading file_searcher_deamon_v5 stdout: {e}");
                    break;
                }
            }
        }

        let _ = child.wait().await;
    });

    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|chunk| {
            (Ok::<Bytes, std::io::Error>(Bytes::from(chunk)), rx)
        })
    });

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; charset=utf-8")
        .header("connection", "close")
        .body(Body::from_stream(stream))
        .unwrap()
}

use std::{path::{Component, Path, PathBuf}, process::Stdio};

use axum::{
    body::{Body, Bytes},
    extract::{FromRequestParts, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use futures_util::{StreamExt, TryStreamExt};
use log::*;
use serde::Deserialize;
use tokio::{fs::{self, File}, io::AsyncReadExt, process::Command, sync::mpsc};
use tokio_util::io::{ReaderStream, StreamReader};

#[derive(Deserialize)]
pub struct Config {
    pub base_dir: String,
    pub port: u16,
    pub file_db_path: String,
}

#[derive(Clone)]
pub struct AppState {
    pub base_dir: PathBuf,
}

pub struct FileMetadata {
    pub filename: String,
    pub filetime: Option<i64>,
    pub sub_dir: Option<String>,
}

impl<S> FromRequestParts<S> for FileMetadata
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        let filename = headers
            .get("x-file-name")
            .and_then(|val| val.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Missing or invalid X-File-Name header".to_string(),
                )
            })?
            .to_string();

        let filetime: Option<i64> = headers
            .get("x-file-time")
            .and_then(|val| val.to_str().ok())
            .map(|s| s.parse().expect("Could not parse to i64"));

        let sub_dir = headers
            .get("x-sub-dir")
            .and_then(|val| val.to_str().ok())
            .map(|s| s.to_string());

        Ok(FileMetadata {
            filename,
            filetime,
            sub_dir,
        })
    }
}

pub struct FilePath {
    pub path: String,
}

impl<S> FromRequestParts<S> for FilePath
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let path = parts
            .headers
            .get("file-path")
            .and_then(|val| val.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "Missing or invalid file-path header".to_string(),
                )
            })?
            .to_string();

        Ok(FilePath { path })
    }
}

fn io_error_to_response(e: std::io::Error, not_found_message: &str) -> (StatusCode, String) {
    match e.kind() {
        std::io::ErrorKind::NotFound => (StatusCode::NOT_FOUND, not_found_message.to_string()),
        std::io::ErrorKind::PermissionDenied => (StatusCode::FORBIDDEN, "Access denied".to_string()),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("IO error: {e}"),
        ),
    }
}

pub async fn root_handler(State(_state): State<AppState>) -> &'static str {
    "filesearcherV5-web active"
}

pub async fn upload_file_handler(
    State(state): State<AppState>,
    metadata: FileMetadata,
    request: Request<Body>,
) -> Result<StatusCode, (StatusCode, String)> {
    info!("Uploading file {}", metadata.filename);

    let upload_dir = state.base_dir.clone();
    fs::create_dir_all(&upload_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create storage directory: {e}"),
        )
    })?;

    let safe_filename = PathBuf::from(&metadata.filename)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid file name".to_string()))?;

    let mut destination_path = upload_dir;
    if let Some(sub_dir) = &metadata.sub_dir {
        destination_path.push(sub_dir);
    }
    destination_path.push(&safe_filename);

    let body_stream = request
        .into_body()
        .into_data_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));

    let mut reader = StreamReader::new(body_stream);

    let mut file = File::create(&destination_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create file: {e}"),
        )
    })?;

    tokio::io::copy(&mut reader, &mut file).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed writing stream to disk: {e}"),
        )
    })?;

    //write the filetime
    if let Some(filetime) = metadata.filetime {
        let mtime = helper_lib::datetime::unixtimestamp_to_systemtime(filetime as u64);
        helper_lib::paths::set_mtime(&destination_path, mtime).map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to set mtime: {e}"),
            )
        })?;
    }

    info!(
        "Successfully uploaded '{}' (Source: {:?}, Timestamp: {:?})",
        safe_filename, metadata.sub_dir, metadata.filetime
    );

    Ok(StatusCode::CREATED)
}

pub async fn download_file_handler(
    State(state): State<AppState>,
    file_path: FilePath,
) -> Result<Response, (StatusCode, String)> {
    if file_path.path.trim().is_empty()
        || file_path.path.contains('\0')
        || file_path.path.chars().any(|c| c.is_control())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid file-path header".to_string(),
        ));
    }

    info!("Downloading file {}", file_path.path);

    let base = state.base_dir.clone();
    let base = fs::canonicalize(&base)
        .await
        .map_err(|e| io_error_to_response(e, "Base directory not found"))?;

    let rel = Path::new(&file_path.path);

    if rel.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "file-path must be relative".to_string(),
        ));
    }

    if rel
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "file-path must not contain '..'".to_string(),
        ));
    }

    let candidate = base.join(rel);
    let candidate = fs::canonicalize(&candidate)
        .await
        .map_err(|e| io_error_to_response(e, "File not found"))?;

    if !candidate.starts_with(&base) {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let metadata = fs::metadata(&candidate)
        .await
        .map_err(|e| io_error_to_response(e, "File not found"))?;

    if !metadata.is_file() {
        return Err((StatusCode::NOT_FOUND, "Not a file".to_string()));
    }

    let file = File::open(&candidate)
        .await
        .map_err(|e| io_error_to_response(e, "File not found"))?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let filename = candidate
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());

    let safe_filename: String = filename
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, metadata.len().to_string())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", safe_filename),
        )
        .body(body)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build response: {e}"),
            )
        })
}

pub async fn delete_file_handler(
    State(state): State<AppState>,
    file_path: FilePath,
) -> Result<StatusCode, (StatusCode, String)> {
    if file_path.path.trim().is_empty()
        || file_path.path.contains('\0')
        || file_path.path.chars().any(|c| c.is_control())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid file-path header".to_string(),
        ));
    }

    info!("Deleting file {}", file_path.path);

    let base = state.base_dir.clone();
    let base = fs::canonicalize(&base)
        .await
        .map_err(|e| io_error_to_response(e, "Base directory not found"))?;

    let rel = Path::new(&file_path.path);

    if rel.is_absolute() {
        return Err((
            StatusCode::BAD_REQUEST,
            "file-path must be relative".to_string(),
        ));
    }

    if rel
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "file-path must not contain '..'".to_string(),
        ));
    }

    let candidate = base.join(rel);
    let candidate = match fs::canonicalize(&candidate).await {
        Ok(candidate) => candidate,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            info!("File not found, nothing to delete: {}", file_path.path);
            return Ok(StatusCode::OK);
        }
        Err(e) => return Err(io_error_to_response(e, "File not found")),
    };

    if !candidate.starts_with(&base) {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    let metadata = fs::metadata(&candidate)
        .await
        .map_err(|e| io_error_to_response(e, "File not found"))?;

    if metadata.is_file() {
        match fs::remove_file(&candidate).await {
            Ok(()) => {
                info!("Deleted file {}", file_path.path);
                Ok(StatusCode::OK)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StatusCode::OK),
            Err(e) => Err(io_error_to_response(e, "Failed to delete file")),
        }
    } else if metadata.is_dir() {
        let mut entries = fs::read_dir(&candidate)
            .await
            .map_err(|e| io_error_to_response(e, "Failed to read directory"))?;

        if entries
            .next()
            .await
            .transpose()
            .map_err(|e| io_error_to_response(e, "Failed to read directory"))?
            .is_some()
        {
            return Err((
                StatusCode::CONFLICT,
                "Directory is not empty".to_string(),
            ));
        }

        match fs::remove_dir(&candidate).await {
            Ok(()) => {
                info!("Deleted empty directory {}", file_path.path);
                Ok(StatusCode::OK)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(StatusCode::OK),
            Err(e) => Err(io_error_to_response(e, "Failed to delete directory")),
        }
    } else {
        Err((StatusCode::NOT_FOUND, "Not a file or directory".to_string()))
    }
}

pub async fn get_file_db_handler() -> Result<Response, (StatusCode, String)> {
    info!("start get_file_db_handler");

    let config_path = "config.toml";

    let config_str = fs::read_to_string(config_path)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read config file: {e}"),
            )
        })?;

    let config: Config = toml::from_str(&config_str).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse config file: {e}"),
        )
    })?;

    let path = PathBuf::from(config.file_db_path);

    let metadata = fs::metadata(&path)
        .await
        .map_err(|e| io_error_to_response(e, "File DB not found"))?;

    if !metadata.is_file() {
        return Err((StatusCode::NOT_FOUND, "File DB not found".to_string()));
    }

    let file = File::open(&path)
        .await
        .map_err(|e| io_error_to_response(e, "File DB not found"))?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file.db".to_string());

    let safe_filename: String = filename
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();

    info!("end get_file_db_handler");

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, metadata.len().to_string())
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", safe_filename),
        )
        .body(body)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build response: {e}"),
            )
        })
}

async fn refresh_file_db_handler() -> Response {
    info!("start refresh_file_db_handler");

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
        let tx = tx;
        let mut buf = vec![0u8; 32];

        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("error reading file_searcher_deamon_v5 stdout: {e}");
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

    info!("end refresh_file_db_handler");

    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; charset=utf-8")
        .header("connection", "close")
        .body(Body::from_stream(stream))
        .unwrap()
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler).post(root_handler))
        .route("/upload", post(upload_file_handler))
        .route("/download", get(download_file_handler).post(download_file_handler))
        .route("/delete_file", delete(delete_file_handler).post(delete_file_handler))
        .route("/get_file_db", get(get_file_db_handler).post(get_file_db_handler))
        .route("/refresh_file_db", get(refresh_file_db_handler).post(refresh_file_db_handler))
        .with_state(state)
}

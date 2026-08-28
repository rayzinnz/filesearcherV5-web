use axum::{
    body::Body,
    extract::{FromRequestParts, Request},
    http::{header, StatusCode},
    response::Response,
    routing::{get, post},
    Router,
};
use futures_util::TryStreamExt;
use std::path::{Component, Path, PathBuf};
use tokio::fs::{self, File};
use tokio_util::io::{ReaderStream, StreamReader};

const BASE_DIR: &str = "/home/ray/MEGA/Rays";

/// Extractor for custom file metadata passed via HTTP headers
struct FileMetadata {
    filename: String,
    filetime: Option<String>,
    sub_dir: Option<String>,
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

        // Custom header X-File-Name is required
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

        let filetime = headers
            .get("x-file-time")
            .and_then(|val| val.to_str().ok())
            .map(|s| s.to_string());

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

/// Extractor for download path passed via HTTP header
struct FilePath {
    path: String,
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

/// Handler for the root GET route
async fn root_handler() -> &'static str {
    "filesearcherV5-web active"
}

/// Handler that receives metadata from headers and streams the raw body to disk
async fn upload_file_handler(
    metadata: FileMetadata,
    request: Request<Body>,
) -> Result<StatusCode, (StatusCode, String)> {
    println!("Uploading file {}", metadata.filename);
    // 1. Ensure target uploads directory exists
    let upload_dir = PathBuf::from(BASE_DIR);
    fs::create_dir_all(&upload_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create storage directory: {e}"),
        )
    })?;

    // 2. Sanitize filename to prevent directory traversal attacks (e.g., "../")
    let safe_filename = PathBuf::from(&metadata.filename)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "Invalid file name".to_string()))?;

    let destination_path = upload_dir.join(&safe_filename);

    // 3. Convert Axum Body stream into an AsyncRead stream
    let body_stream = request
        .into_body()
        .into_data_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));

    let mut reader = StreamReader::new(body_stream);

    // 4. Create local target file and stream bytes directly from memory/network to disk
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

    println!(
        "Successfully uploaded '{}' (Source: {:?}, Timestamp: {:?})",
        safe_filename, metadata.sub_dir, metadata.filetime
    );

    Ok(StatusCode::CREATED)
}

/// Handler that streams a file from disk based on the `file-path` header
async fn download_file_handler(file_path: FilePath) -> Result<Response, (StatusCode, String)> {
    if file_path.path.trim().is_empty()
        || file_path.path.contains('\0')
        || file_path.path.chars().any(|c| c.is_control())
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid file-path header".to_string(),
        ));
    }
    println!("Downloading file {}", file_path.path);

    let base = PathBuf::from(BASE_DIR);
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

#[tokio::main]
async fn main() {
    println!("starting");
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/upload", post(upload_file_handler))
        .route("/download", get(download_file_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:34890")
        .await
        .unwrap();
    println!("Server running on http://127.0.0.1:34890");
    axum::serve(listener, app).await.unwrap();
}

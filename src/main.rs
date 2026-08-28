use axum::{
    body::Body,
    extract::{FromRequestParts, Request},
    http::{header, HeaderMap, StatusCode},
    routing::post,
    Router,
};
use futures_util::TryStreamExt;
use std::path::PathBuf;
use tokio::fs::{self, File};
use tokio_util::io::StreamReader;

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

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let headers = &parts.headers;

        // Custom header X-File-Name is required
        let filename = headers
            .get("x-file-name")
            .and_then(|val| val.to_str().ok())
            .ok_or_else(|| (StatusCode::BAD_REQUEST, "Missing or invalid X-File-Name header".to_string()))?
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

/// Handler that receives metadata from headers and streams the raw body to disk
async fn upload_file_handler(
    metadata: FileMetadata,
    request: Request<Body>,
) -> Result<StatusCode, (StatusCode, String)> {
    // 1. Ensure target uploads directory exists
    let upload_dir = PathBuf::from("/home/ray/temp/webtransfer");
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

#[tokio::main]
async fn main() {
    println!("starting");
    let app = Router::new().route("/upload", post(upload_file_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:34890").await.unwrap();
    println!("Server running on http://127.0.0.1:34890");
    axum::serve(listener, app).await.unwrap();
}

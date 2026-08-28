use axum::{body::Body, http::{Request, StatusCode}, Router};
use filesearcherv5_web::{router, AppState};
use http_body_util::BodyExt;
use std::path::PathBuf;
use tower::ServiceExt;

fn app_with_base(base_dir: PathBuf) -> Router {
    router(AppState { base_dir })
}

async fn body_to_bytes(body: Body) -> Vec<u8> {
    body.collect().await.unwrap().to_bytes().to_vec()
}

#[tokio::test]
async fn test_root() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        body_to_bytes(response.into_body()).await,
        b"filesearcherV5-web active".to_vec()
    );
}

#[tokio::test]
async fn test_upload_happy_path() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("x-file-name", "hello.txt")
        .body(Body::from("hello world"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let saved = std::fs::read(tmp.path().join("hello.txt")).unwrap();
    assert_eq!(saved, b"hello world".to_vec());
}

#[tokio::test]
async fn test_upload_missing_filename_header() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .body(Body::from("data"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_empty_filename() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("x-file-name", "")
        .body(Body::from("data"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_traversal_sanitised() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("x-file-name", "../../etc/passwd")
        .body(Body::from("pwned"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let saved = std::fs::read(tmp.path().join("passwd")).unwrap();
    assert_eq!(saved, b"pwned".to_vec());
}

#[tokio::test]
async fn test_upload_optional_headers() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("x-file-name", "a.txt")
        .header("x-file-time", "2025-01-01")
        .header("x-sub-dir", "docs")
        .body(Body::from("optional"))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let saved = std::fs::read(tmp.path().join("a.txt")).unwrap();
    assert_eq!(saved, b"optional".to_vec());
}

#[tokio::test]
async fn test_download_happy_path() {
    let app = app_with_base(PathBuf::from("test/files"));
    let expected = std::fs::read("test/files/test.txt").unwrap();

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "test.txt")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/octet-stream"
    );
    assert_eq!(
        response.headers().get("content-length").unwrap().to_str().unwrap(),
        expected.len().to_string()
    );
    assert_eq!(
        response.headers().get("content-disposition").unwrap().to_str().unwrap(),
        "attachment; filename=\"test.txt\""
    );
    assert_eq!(body_to_bytes(response.into_body()).await, expected);
}

#[tokio::test]
async fn test_download_nested_file() {
    let app = app_with_base(PathBuf::from("test/files"));
    let expected = std::fs::read("test/files/subdir/nested.txt").unwrap();

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "subdir/nested.txt")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_to_bytes(response.into_body()).await, expected);
}

#[tokio::test]
async fn test_download_file_not_found() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "nonexistent.txt")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_download_missing_header() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_empty_path() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_absolute_path_rejected() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "/etc/passwd")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_parent_dir_rejected() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "../secret.txt")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_parent_dir_inside_rejected() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "subdir/../test.txt")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_download_directory_returns_not_found() {
    let app = app_with_base(PathBuf::from("test/files"));

    let request = Request::builder()
        .uri("/download")
        .header("file-path", "subdir")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_upload_then_download_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let app = app_with_base(tmp.path().to_path_buf());

    let upload_request = Request::builder()
        .method("POST")
        .uri("/upload")
        .header("x-file-name", "roundtrip.txt")
        .body(Body::from("roundtrip content"))
        .unwrap();

    let upload_response = app.clone().oneshot(upload_request).await.unwrap();
    assert_eq!(upload_response.status(), StatusCode::CREATED);

    let download_request = Request::builder()
        .uri("/download")
        .header("file-path", "roundtrip.txt")
        .body(Body::empty())
        .unwrap();

    let download_response = app.oneshot(download_request).await.unwrap();
    assert_eq!(download_response.status(), StatusCode::OK);
    assert_eq!(
        body_to_bytes(download_response.into_body()).await,
        b"roundtrip content".to_vec()
    );
}

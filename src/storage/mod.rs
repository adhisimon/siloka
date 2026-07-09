use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path as AxumPath, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, put},
    Router,
};
use tokio::net::TcpListener;

/// Dumb Storage - Physical storage component for Siloka objects.
/// Only responsible for disk I/O and nested directory management.
#[derive(Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance with a pre-resolved absolute data directory.
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Deterministically determines the full path of the physical file on disk.
    /// Siloka Hybrid Nesting Pattern:
    /// - Isolates all objects under the "blobs" subdirectory of the resolved data directory.
    /// - Tier 1 & 2 folders use right-padding with '_' (minimum length of 4 characters).
    /// - The physical filename at the end uses the original `blob_id`.
    pub fn resolve_path(&self, blob_id: &str) -> PathBuf {
        let padded = format!("{:_<4}", blob_id);
        let dir_1 = &padded[0..2];
        let dir_2 = &padded[2..4];

        self.data_dir
            .join("blobs")
            .join(dir_1)
            .join(dir_2)
            .join(blob_id)
    }

    /// Writes raw binary data directly to disk (UPSERT contract).
    pub fn write_raw(&self, blob_id: &str, raw_data: &[u8]) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);

        // Ensure parent directories exist before writing
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(final_path, raw_data)?;
        Ok(())
    }

    /// Reads raw binary data directly from disk based on the BLOB_ID.
    pub fn read_raw(&self, blob_id: &str) -> io::Result<Vec<u8>> {
        let final_path = self.resolve_path(blob_id);
        fs::read(final_path)
    }

    /// Deletes raw binary data from disk based on the BLOB_ID.
    /// Cleans up empty parent directories up to the "blobs" root directory.
    pub fn delete_raw(&self, blob_id: &str) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);

        if final_path.exists() {
            fs::remove_file(&final_path)?;

            // Walk up and clean up empty directories recursively
            let mut current_dir = final_path.parent();
            while let Some(dir) = current_dir {
                // Stop directory traversal if we reach the root data_dir or its "blobs" subdirectory
                if dir == self.data_dir || dir.ends_with("blobs") {
                    break;
                }

                if let Ok(mut entries) = fs::read_dir(dir) {
                    if entries.next().is_none() {
                        let _ = fs::remove_dir(dir);
                    } else {
                        break; // Stop climbing if directory is not empty
                    }
                } else {
                    break;
                }

                current_dir = dir.parent();
            }
        }

        Ok(())
    }
}

/// Shared application state for the Axum HTTP Router.
struct AppState {
    storage: Storage,
    apikey: String,
}

/// Helper function to validate authorization.
/// Expects the 'Authorization' header with 'ApiKey <APIKEY>' format.
fn is_authorized(headers: &HeaderMap, expected_key: &str) -> bool {
    if let Some(auth_str) = headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
    {
        const PREFIX: &str = "ApiKey ";
        if let Some(provided_key) = auth_str.strip_prefix(PREFIX) {
            return provided_key == expected_key;
        }
    }
    false
}

// --- HTTP HANDLERS ---

/// Handler for GET /blobs/{id} (GET operation)
async fn get_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    match state.storage.read_raw(&blob_id) {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "Blob not found").into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read blob: {}", e),
        )
            .into_response(),
    }
}

/// Handler for PUT /blobs/{id} (PUT/UPSERT operation)
async fn put_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    match state.storage.write_raw(&blob_id, &body) {
        Ok(_) => (StatusCode::OK, "Blob written successfully").into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write blob: {}", e),
        )
            .into_response(),
    }
}

/// Handler for DELETE /blobs/{id} (Idempotent DELETE operation)
async fn delete_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Always returns 204 No Content (Idempotent success) unless system error occurs
    match state.storage.delete_raw(&blob_id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete blob: {}", e),
        )
            .into_response(),
    }
}

/// Starts the HTTP storage server using Tokio and Axum.
pub async fn start_server(
    data_dir: PathBuf,
    addr: std::net::SocketAddr,
    apikey: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = Storage::new(data_dir);
    let shared_state = Arc::new(AppState { storage, apikey });

    // Build routes matching the PUT/GET/DELETE contract using Axum's modern {capture} syntax
    let app = Router::new()
        .route("/blobs/{id}", get(get_blob))
        .route("/blobs/{id}", put(put_blob))
        .route("/blobs/{id}", delete(delete_blob))
        .with_state(shared_state);

    println!("Starting Storage Node HTTP server on {}", addr);
    
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
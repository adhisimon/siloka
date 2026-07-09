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
use tracing::{info, error, instrument};

// --- SILOKA STORAGE CONFIGURATION ---
// These constants define the directory partitioning strategy for the Hybrid Nesting Pattern.
const NESTING_DEPTH: usize = 2;
const CHARS_PER_LEVEL: usize = 2;
const CURRENT_VERSION: &str = "1";
const VERSION_FILE_NAME: &str = "VERSION";

/// Dumb Storage - Physical storage component for Siloka objects.
/// Responsible for disk I/O, nested directory management, and version metadata initialization.
#[derive(Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance with an absolute resolved data directory.
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Performs initial setup of the physical directory structure.
    /// 
    /// If the `blobs` directory does not exist, this will:
    /// 1. Create the `blobs` directory.
    /// 2. Write a `VERSION` file recording the current layout version ("1").
    ///
    /// If the `blobs` directory exists but the version file is missing,
    /// it defaults to Version 1 to maintain backward compatibility.
    pub fn init(&self) -> io::Result<()> {
        let blobs_dir = self.data_dir.join("blobs");
        let version_file_path = blobs_dir.join(VERSION_FILE_NAME);

        if !blobs_dir.exists() {
            info!(path = %blobs_dir.display(), "Initializing new blobs storage directory");
            fs::create_dir_all(&blobs_dir)?;
            fs::write(&version_file_path, format!("{}\n", CURRENT_VERSION))?;
        } else if !version_file_path.exists() {
            info!("Blobs directory found without a version file. Labeling as Version 1.");
            fs::write(&version_file_path, format!("{}\n", CURRENT_VERSION))?;
        } else {
            let version_on_disk = fs::read_to_string(&version_file_path)?
                .trim()
                .to_string();
            
            if version_on_disk != CURRENT_VERSION {
                error!(disk = %version_on_disk, code = CURRENT_VERSION, "Storage version mismatch!");
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Storage version mismatch! Disk: {}, Code: {}", version_on_disk, CURRENT_VERSION),
                ));
            }
        }

        Ok(())
    }

    /// Determines the exact physical file path based on the BLOB_ID.
    pub fn resolve_path(&self, blob_id: &str) -> PathBuf {
        let required_len = NESTING_DEPTH * CHARS_PER_LEVEL;
        let mut padded = blob_id.to_string();
        
        // Add padding '_' to the right if blob_id is too short
        while padded.len() < required_len {
            padded.push('_');
        }

        let mut final_path = self.data_dir.join("blobs");
        for i in 0..NESTING_DEPTH {
            let start = i * CHARS_PER_LEVEL;
            let end = start + CHARS_PER_LEVEL;
            final_path = final_path.join(&padded[start..end]);
        }
        final_path.join(blob_id)
    }

    /// Writes raw binary data directly to disk (UPSERT contract).
    pub fn write_raw(&self, blob_id: &str, raw_data: &[u8]) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(final_path, raw_data)
    }

    /// Reads raw binary data from disk based on BLOB_ID.
    pub fn read_raw(&self, blob_id: &str) -> io::Result<Vec<u8>> {
        fs::read(self.resolve_path(blob_id))
    }

    /// Deletes a binary file from disk and cleans up empty parent directories.
    pub fn delete_raw(&self, blob_id: &str) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);
        if final_path.exists() {
            fs::remove_file(&final_path)?;
            let mut current_dir = final_path.parent();
            while let Some(dir) = current_dir {
                if dir == self.data_dir || dir.ends_with("blobs") {
                    break;
                }
                if let Ok(mut entries) = fs::read_dir(dir) {
                    if entries.next().is_none() {
                        let _ = fs::remove_dir(dir);
                    } else {
                        break;
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

/// Shared application state for Axum HTTP Router.
struct AppState {
    storage: Storage,
    apikey: String,
}

/// Helper function to validate authorization headers.
fn is_authorized(headers: &HeaderMap, expected_key: &str) -> bool {
    headers
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|auth_str| auth_str.strip_prefix("ApiKey "))
        .map(|key| key == expected_key)
        .unwrap_or(false)
}

#[instrument(skip(state, body))]
async fn put_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.storage.write_raw(&blob_id, &body) {
        Ok(_) => {
            info!(%blob_id, "Blob written successfully");
            StatusCode::OK.into_response()
        }
        Err(e) => {
            error!(%blob_id, error = %e, "Failed to write blob");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[instrument(skip(state))]
async fn get_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.storage.read_raw(&blob_id) {
        Ok(data) => (StatusCode::OK, data).into_response(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!(%blob_id, error = %e, "Failed to read blob");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

#[instrument(skip(state))]
async fn delete_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match state.storage.delete_raw(&blob_id) {
        Ok(_) => {
            info!(%blob_id, "Blob deleted successfully");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            error!(%blob_id, error = %e, "Failed to delete blob");
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// Starts the HTTP storage server using Tokio and Axum.
pub async fn start_server(
    data_dir: PathBuf,
    addr: std::net::SocketAddr,
    apikey: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = Storage::new(data_dir);
    storage.init()?;

    let shared_state = Arc::new(AppState { storage, apikey });

    let app = Router::new()
        .route("/blobs/:id", get(get_blob))
        .route("/blobs/:id", put(put_blob))
        .route("/blobs/:id", delete(delete_blob))
        .with_state(shared_state);

    info!(address = %addr, "Starting Storage Node HTTP server");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
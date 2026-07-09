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

// --- KONFIGURASI PENYIMPANAN SILOKA ---
// Konstanta ini mendefinisikan strategi partisi direktori untuk Pola Hybrid Nesting.
// Dapat disesuaikan secara bebas untuk menskalakan indeks sistem berkas.
const NESTING_DEPTH: usize = 2;     // Jumlah tingkatan direktori bersarang (2 tingkat: dir1/dir2)
const CHARS_PER_LEVEL: usize = 2;   // Jumlah karakter yang diekstrak per tingkat direktori
const CURRENT_VERSION: &str = "1";  // Versi tata letak penyimpanan fisik saat ini (2x2)
const VERSION_FILE_NAME: &str = "VERSION"; // Nama file pelacakan versi di bawah direktori root blobs

/// Dumb Storage - Komponen penyimpanan fisik untuk objek Siloka.
/// Hanya bertanggung jawab untuk I/O disk, manajemen direktori bersarang, dan inisialisasi metadata versi.
#[derive(Clone)]
pub struct Storage {
    data_dir: PathBuf,
}

impl Storage {
    /// Membuat instansi Storage baru dengan direktori data absolut yang sudah di-resolve.
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Melakukan inisialisasi awal terhadap struktur direktori fisik.
    ///
    /// Jika direktori `blobs` belum ada, metode ini akan:
    ///
    /// 1. Membuat direktori `blobs`.
    /// 2. Menulis file `VERSION` yang mencatat versi tata letak saat ini ("1").
    ///
    /// Jika direktori `blobs` sudah ada namun file `VERSION` tidak ditemukan,
    /// kita asumsikan sebagai Versi 1 demi menjamin kompatibilitas ke belakang (backward compatibility).
    pub fn init(&self) -> io::Result<()> {
        let blobs_dir = self.data_dir.join("blobs");
        let version_file_path = blobs_dir.join(VERSION_FILE_NAME);

        if !blobs_dir.exists() {
            println!("Initializing new blobs storage directory at: {:?}", blobs_dir);
            fs::create_dir_all(&blobs_dir)?;
            fs::write(&version_file_path, format!("{}\n", CURRENT_VERSION))?;
        } else if !version_file_path.exists() {
            // Skenario transisi: folder blobs sudah ada dari fase sebelum versi diterapkan.
            // Kita secara otomatis melabelinya sebagai Versi 1 demi menjaga kompatibilitas ke belakang.
            println!("Blobs directory found without a version file. Labeling as Version 1.");
            fs::write(&version_file_path, format!("{}\n", CURRENT_VERSION))?;
        } else {
            // Memvalidasi apakah versi yang ada di disk sesuai dengan versi kode saat ini.
            // Di masa depan, logika migrasi otomatis akan dipicu di blok ini.
            let version_on_disk = fs::read_to_string(&version_file_path)?
                .trim()
                .to_string();
            
            if version_on_disk != CURRENT_VERSION {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Storage version mismatch! Disk: Version {}, Code: Version {}. \
                        Migration is required before starting the node.", 
                        version_on_disk, CURRENT_VERSION
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Mendeterminasi jalur lengkap file fisik di disk secara pasti berdasarkan BLOB_ID.
    ///
    /// Pola Hybrid Nesting Siloka:
    /// - Mengisolasi semua objek di bawah sub-direktori "blobs" dari direktori data yang ditentukan.
    /// - Menghasilkan N tingkatan direktori bersarang secara dinamis berdasarkan NESTING_DEPTH and CHARS_PER_LEVEL.
    /// - Secara otomatis menambahkan padding '_' di sebelah kanan BLOB_ID pendek untuk segmentasi direktori yang aman.
    /// - Nama file fisik di ujung jalur tetap menggunakan 'blob_id' asli tanpa padding.
    pub fn resolve_path(&self, blob_id: &str) -> PathBuf {
        let required_len = NESTING_DEPTH * CHARS_PER_LEVEL;
        
        // Tambahkan padding '_' di sebelah kanan blob_id jika karakternya kurang dari batas minimum
        let mut padded = blob_id.to_string();
        while padded.len() < required_len {
            padded.push('_');
        }

        let mut final_path = self.data_dir.join("blobs");

        // Tempelkan direktori bersarang secara dinamis berdasarkan konfigurasi
        for i in 0..NESTING_DEPTH {
            let start = i * CHARS_PER_LEVEL;
            let end = start + CHARS_PER_LEVEL;
            let dir_segment = &padded[start..end];
            final_path = final_path.join(dir_segment);
        }

        // File fisik di ujung jalur tetap mempertahankan nama BLOB_ID asli yang belum dipad
        final_path.join(blob_id)
    }

    /// Menulis data biner mentah langsung ke disk (Kontrak UPSERT).
    pub fn write_raw(&self, blob_id: &str, raw_data: &[u8]) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);

        // Pastikan folder induk telah terbentuk sebelum menulis file
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(final_path, raw_data)?;
        Ok(())
    }

    /// Membaca data biner mentah langsung dari disk berdasarkan BLOB_ID.
    pub fn read_raw(&self, blob_id: &str) -> io::Result<Vec<u8>> {
        let final_path = self.resolve_path(blob_id);
        fs::read(final_path)
    }

    /// Menghapus file biner dari disk berdasarkan BLOB_ID.
    ///
    /// Melakukan pembersihan folder induk yang kosong secara rekursif hingga batas root "blobs".
    pub fn delete_raw(&self, blob_id: &str) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);

        if final_path.exists() {
            fs::remove_file(&final_path)?;

            // Telusuri ke atas dan bersihkan direktori kosong secara rekursif
            let mut current_dir = final_path.parent();
            while let Some(dir) = current_dir {
                // Hentikan penelusuran jika mencapai direktori data utama atau sub-direktori "blobs"
                if dir == self.data_dir || dir.ends_with("blobs") {
                    break;
                }

                if let Ok(mut entries) = fs::read_dir(dir) {
                    if entries.next().is_none() {
                        let _ = fs::remove_dir(dir);
                    } else {
                        break; // Hentikan pendakian jika direktori tidak kosong
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

/// State aplikasi bersama untuk Router HTTP Axum.
struct AppState {
    storage: Storage,
    apikey: String,
}

/// Fungsi pembantu untuk memvalidasi otorisasi.
///
/// Mengekspektasikan header 'Authorization' dengan format 'ApiKey <APIKEY>'.
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

// --- HANDLER HTTP ---

/// Handler untuk GET /blobs/{id} (Operasi GET)
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

/// Handler untuk PUT /blobs/{id} (Operasi PUT/UPSERT)
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

/// Handler untuk DELETE /blobs/{id} (Operasi DELETE Idempoten)
async fn delete_blob(
    AxumPath(blob_id): AxumPath<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !is_authorized(&headers, &state.apikey) {
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }

    // Selalu mengembalikan 204 No Content jika sukses (idempoten) kecuali terjadi error sistem internal
    match state.storage.delete_raw(&blob_id) {
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete blob: {}", e),
        )
            .into_response(),
    }
}

/// Memulai server HTTP storage menggunakan Tokio dan Axum.
pub async fn start_server(
    data_dir: PathBuf,
    addr: std::net::SocketAddr,
    apikey: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = Storage::new(data_dir);
    
    // Pemeriksaan keamanan: Inisialisasi direktori penyimpanan dan penanda VERSION sebelum mengikat socket
    storage.init()?;

    let shared_state = Arc::new(AppState { storage, apikey });

    // Membangun rute yang cocok dengan kontrak PUT/GET/DELETE menggunakan sintaks tangkapan dinamis {id} Axum
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
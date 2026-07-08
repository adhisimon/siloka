use std::fs;
use std::io::{self};
use std::path::{Path, PathBuf};

/// Dumb Storage - Physical storage component for Siloka objects.
/// Only responsible for disk I/O and nested directory management.
pub struct Storage {
    base_path: PathBuf,
}

impl Storage {
    /// Creates a new Storage instance with a specified base path.
    pub fn new<P: AsRef<Path>>(base_path: P) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Deterministically determines the full path of the physical file on disk.
    /// Siloka Hybrid Nesting Pattern:
    /// - Isolates all objects under the "blobs" subdirectory of the base path.
    /// - Tier 1 & 2 folders use right-padding with '_' (minimum length of 4 characters).
    /// - The physical filename at the end uses the original `blob_id` to facilitate easy scanning.
    pub fn resolve_path(&self, blob_id: &str) -> PathBuf {
        // Right padding with '_' so it will have at least 4 chars
        let padded = format!("{:_<4}", blob_id);
        
        // Take slices for level 1 and 2
        let dir_1 = &padded[0..2];
        let dir_2 = &padded[2..4];

        // Isolate blob storage under "blobs/" subdirectory
        self.base_path
            .join("blobs")
            .join(dir_1)
            .join(dir_2)
            .join(blob_id)
    }

    /// Writes raw binary data directly to disk.
    pub fn write_raw(&self, blob_id: &str, raw_data: &[u8]) -> io::Result<()> {
        let final_path = self.resolve_path(blob_id);

        // Recursively create parent directories if they don't exist yet
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
}

/// Local test function to verify Dumb Storage functionality with a dynamic base path.
pub fn run<P: AsRef<Path>>(base_path: P) {
    println!("Bootstraping storage...");

    let storage = Storage::new(base_path);
    
    // Test with a very short ID (only 1 character)
    let short_id = "a";
    let data = b"Siloka Hybrid Nesting Test";

    println!("Writing data with a very short ID: '{}'", short_id);
    if let Err(e) = storage.write_raw(short_id, data) {
        eprintln!("Error writing data: {}", e);
        return;
    }

    let resolved = storage.resolve_path(short_id);
    println!("Blob written on: {:?}", resolved);

    // Verify data readback
    match storage.read_raw(short_id) {
        Ok(read_data) => {
            let text = String::from_utf8_lossy(&read_data);
            println!("Read successful: '{}'", text);
        }
        Err(e) => eprintln!("Error reading blob: {}", e),
    }
}

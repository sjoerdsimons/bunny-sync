use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    pub size: u64,
    /// SHA256 hex digest, uppercase
    pub checksum: String,
}

/// Walk `local_dir` recursively and compute SHA256 checksums for all files.
/// Returns a map from forward-slash relative paths to `LocalFile`.
pub async fn scan(local_dir: &Path) -> Result<HashMap<String, LocalFile>> {
    let mut files = HashMap::new();

    for entry in WalkDir::new(local_dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let abs_path = entry.path().to_owned();
        let rel = abs_path
            .strip_prefix(local_dir)
            .context("failed to strip local dir prefix")?;

        // Normalise to forward-slash relative path
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");

        let local_file = hash_file(&abs_path).await?;
        files.insert(rel_str, local_file);
    }

    Ok(files)
}

/// Read a file in chunks, streaming it through SHA256, without loading it all into memory.
async fn hash_file(path: &Path) -> Result<LocalFile> {
    let mut file = fs::File::open(path)
        .await
        .with_context(|| format!("failed to open {}", path.display()))?;

    let size = file
        .metadata()
        .await
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len();

    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024]; // 256 KiB chunks

    loop {
        let n = file
            .read(&mut buf)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    let checksum = hex::encode(hasher.finalize()).to_uppercase();
    Ok(LocalFile { size, checksum })
}

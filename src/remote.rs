use anyhow::Result;
use bunny_api_tokio::EdgeStorageClient;
use futures::future::join_all;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub size: u64,
    /// SHA256 hex digest, uppercase (as returned by Bunny)
    pub checksum: Option<String>,
}

/// Recursively list all files in the storage zone.
/// Returns a map from forward-slash relative paths (no leading slash, no zone prefix)
/// to `RemoteFile`.
pub async fn scan(client: &EdgeStorageClient, zone: &str) -> Result<HashMap<String, RemoteFile>> {
    let mut files = HashMap::new();
    scan_dir(client, zone, "", &mut files).await?;
    Ok(files)
}

async fn scan_dir(
    client: &EdgeStorageClient,
    zone: &str,
    path: &str,
    files: &mut HashMap<String, RemoteFile>,
) -> Result<()> {
    println!("Scanning path: {path}");
    let entries = client.list(path).await?;

    let mut subdir_futures = Vec::new();

    for entry in &entries {
        if entry.is_directory {
            // Build the subdir path for the next list() call: /<name>/
            let subpath = format!("{}{}/", path, entry.object_name);
            subdir_futures.push(subpath);
        } else {
            // Strip the leading "/<zone>/" prefix that Bunny includes in the path field
            let full = format!("{}{}", entry.path, entry.object_name);
            let prefix = format!("/{}/", zone);
            let rel = full
                .strip_prefix(&prefix)
                .unwrap_or(full.trim_start_matches('/'));

            files.insert(
                rel.to_string(),
                RemoteFile {
                    size: entry.length as u64,
                    checksum: entry.checksum.as_ref().map(|s| s.to_uppercase()),
                },
            );
        }
    }

    // Recurse into subdirectories concurrently
    if !subdir_futures.is_empty() {
        // We need to collect results sequentially into the shared map;
        // scan each subdir independently and merge afterwards.
        let futures: Vec<_> = subdir_futures
            .iter()
            .map(|p| {
                let p = p.clone();
                async move {
                    let mut sub = HashMap::new();
                    // Box to avoid infinite recursion in async fn
                    Box::pin(scan_dir(client, zone, &p, &mut sub)).await?;
                    Ok::<_, anyhow::Error>(sub)
                }
            })
            .collect();

        let results = join_all(futures).await;
        for result in results {
            files.extend(result?);
        }
    }

    Ok(())
}

use crate::local::LocalFile;
use crate::remote::RemoteFile;
use anyhow::{Context, Result};
use bunny_api_tokio::EdgeStorageClient;
use futures::future::join_all;
use std::collections::HashMap;
use tokio::fs;

pub struct SyncPlan {
    /// Relative paths of files to upload (new or changed)
    pub uploads: Vec<String>,
    /// Relative paths of remote files to delete (not in local)
    pub deletes: Vec<String>,
}

impl SyncPlan {
    pub fn build(local: &HashMap<String, LocalFile>, remote: &HashMap<String, RemoteFile>) -> Self {
        let mut uploads = Vec::new();
        let mut deletes = Vec::new();

        for (path, lf) in local {
            let upload = match remote.get(path) {
                // If sizes don't match always upload
                Some(rf) if rf.size != lf.size => true,
                // If there is no remote checksum, always upload otherwise only if checksums don't
                // match
                Some(rf) => match rf.checksum {
                    Some(ref rf_checksum) => rf_checksum != &lf.checksum,
                    None => true,
                },
                // File doesn't exist yet on the remote upload
                _ => true,
            };

            if upload {
                uploads.push(path.clone())
            }
        }

        for path in remote.keys() {
            if !local.contains_key(path) {
                deletes.push(path.clone());
            }
        }

        uploads.sort();
        deletes.sort();

        SyncPlan { uploads, deletes }
    }

    pub fn print(&self) {
        if self.uploads.is_empty() && self.deletes.is_empty() {
            println!("Everything is up to date. Nothing to do.");
            return;
        }
        for path in &self.uploads {
            println!("  UPLOAD  {path}");
        }
        for path in &self.deletes {
            println!("  DELETE  {path}");
        }
        println!(
            "\n{} upload(s), {} delete(s)",
            self.uploads.len(),
            self.deletes.len()
        );
    }

    pub async fn execute(
        &self,
        client: &EdgeStorageClient,
        local_dir: &std::path::Path,
    ) -> Result<()> {
        // Uploads (concurrent)
        let upload_futures: Vec<_> = self
            .uploads
            .iter()
            .map(|path| {
                let abs = local_dir.join(path.split('/').collect::<std::path::PathBuf>());
                async move {
                    let bytes = fs::read(&abs)
                        .await
                        .with_context(|| format!("reading {}", abs.display()))?;
                    println!("R: {path}");
                    client
                        .upload(path, bytes.into())
                        .await
                        .with_context(|| format!("uploading {path}"))?;
                    println!("  uploaded  {path}");
                    Ok::<_, anyhow::Error>(())
                }
            })
            .collect();

        let upload_results = join_all(upload_futures).await;
        for r in upload_results {
            r?;
        }

        // Deletes
        for path in &self.deletes {
            client
                .delete(path)
                .await
                .with_context(|| format!("deleting {path}"))?;
            println!("  deleted   {path}");
        }

        Ok(())
    }
}

use std::convert::TryInto;

pub async fn write_file_async(
    path: impl AsRef<std::path::Path>,
    content: &[u8],
) -> Result<(), anyhow::Error> {
    use tokio::io::AsyncWriteExt;

    tracing::trace!("Writing file (async) {}", path.as_ref().display());
    let mut file = tokio::fs::File::create(&path).await?;
    file.write_all(content).await?;
    file.sync_all().await?;

    Ok(())
}

pub async fn read_file_async(path: impl AsRef<std::path::Path>) -> Result<Vec<u8>, anyhow::Error> {
    use tokio::io::AsyncReadExt;

    tracing::trace!("Opening file (async) {}", path.as_ref().display());
    let mut file = tokio::fs::File::open(&path).await?;
    let mut content = vec![];
    file.read_to_end(&mut content).await?;
    tracing::trace!(
        "Read file {} with {} bytes",
        path.as_ref().display(),
        content.len()
    );

    Ok(content)
}

pub async fn write_file_async_encrypted(
    path: impl AsRef<std::path::Path>,
    content: impl Into<Vec<u8>>,
    store_enc: Option<&super::encryption::StorageEncryption>,
) -> Result<(), anyhow::Error> {
    let mut content = content.into();

    if let Some(store_enc) = store_enc {
        store_enc.encrypt(&mut content);
    }

    write_file_async(path, &content).await?;

    Ok(())
}

pub async fn read_file_async_encrypted(
    path: impl AsRef<std::path::Path>,
    store_enc: Option<&super::encryption::StorageEncryption>,
) -> Result<Vec<u8>, anyhow::Error> {
    let mut content = read_file_async(path).await?;

    if let Some(store_enc) = store_enc {
        store_enc.decrypt(&mut content)?;
    }

    Ok(content)
}

pub async fn read_salt_file(path: impl AsRef<std::path::Path>) -> Result<[u8; 8], anyhow::Error> {
    let salt = read_file_async(path).await?;
    anyhow::ensure!(salt.len() == 8, "salt file not 8 bytes");

    Ok(salt.try_into().unwrap())
}

pub fn clear_old_logs(path: &std::path::PathBuf, keep_count: usize, filename_regex: &str) -> bool {
    if keep_count < 2 {
        tracing::error!("Can't rotate logs with count {}", keep_count);
        return false;
    }

    match std::fs::read_dir(path) {
        Ok(file_list) => {
            // Get list of logfiles
            let log_regex = regex::Regex::new(filename_regex).unwrap();
            let mut file_list: Vec<String> = file_list
                .filter_map(|a| if let Ok(a) = a { Some(a) } else { None })
                .filter(|a| {
                    a.metadata().unwrap().is_file()
                        && log_regex.is_match(&a.file_name().to_string_lossy())
                })
                .map(|f| f.file_name().to_str().unwrap().to_owned())
                .collect();

            // If enough files, remove the oldest ones
            if file_list.len() > keep_count {
                file_list.sort_by(|b, a| a.cmp(b));

                for file in file_list[keep_count..].iter() {
                    match std::fs::remove_file(path.join(file)) {
                        Ok(()) => tracing::trace!("Deleted old log file: {}", file),
                        Err(e) => {
                            tracing::error!("Could not delete old log file {}: {:?}", file, e);
                            return false;
                        }
                    }
                }
            }
        }
        Err(e) => {
            tracing::error!("Could not read log file folder contents: {:?}", e);
            return false;
        }
    };
    true
}

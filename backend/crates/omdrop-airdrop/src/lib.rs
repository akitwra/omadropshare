use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AirDropCapabilities {
    pub chunked_upload: bool,
    pub transfer_id: bool,
    pub dvzip: bool,
    pub multiple_files: bool,
}

impl AirDropCapabilities {
    pub const MODERN_IOS: Self = Self {
        chunked_upload: true,
        transfer_id: true,
        dvzip: true,
        multiple_files: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetyLimits {
    pub max_incoming_bytes: u64,
    pub max_decompressed_bytes: u64,
    pub max_file_count: usize,
    pub max_metadata_bytes: usize,
    pub max_filename_bytes: usize,
}

impl Default for SafetyLimits {
    fn default() -> Self {
        Self {
            max_incoming_bytes: 5 * 1_024 * 1_024 * 1_024,
            max_decompressed_bytes: 10 * 1_024 * 1_024 * 1_024,
            max_file_count: 1_000,
            max_metadata_bytes: 4 * 1_024 * 1_024,
            max_filename_bytes: 255,
        }
    }
}

impl SafetyLimits {
    pub fn validate_offer(
        &self,
        declared_bytes: u64,
        file_count: usize,
        metadata_bytes: usize,
    ) -> Result<(), SafetyError> {
        if declared_bytes > self.max_incoming_bytes {
            return Err(SafetyError::TransferTooLarge);
        }
        if file_count > self.max_file_count {
            return Err(SafetyError::TooManyFiles);
        }
        if metadata_bytes > self.max_metadata_bytes {
            return Err(SafetyError::MetadataTooLarge);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SafetyError {
    #[error("incoming transfer exceeds the configured byte limit")]
    TransferTooLarge,
    #[error("incoming transfer contains too many files")]
    TooManyFiles,
    #[error("incoming metadata exceeds the configured limit")]
    MetadataTooLarge,
    #[error("filename is unsafe")]
    UnsafeFilename,
    #[error("filename is too long")]
    FilenameTooLong,
    #[error("URL scheme is not allowed")]
    UrlSchemeNotAllowed,
    #[error("URL is malformed")]
    MalformedUrl,
}

pub fn safe_filename(path: &Path, max_bytes: usize) -> Result<&OsStr, SafetyError> {
    let mut components = path.components();
    let Some(Component::Normal(name)) = components.next() else {
        return Err(SafetyError::UnsafeFilename);
    };
    if components.next().is_some() || name.is_empty() || name == OsStr::new(".") {
        return Err(SafetyError::UnsafeFilename);
    }
    if name.as_encoded_bytes().len() > max_bytes {
        return Err(SafetyError::FilenameTooLong);
    }
    Ok(name)
}

pub fn reserve_destination(directory: &Path, offered: &Path) -> io::Result<(PathBuf, fs::File)> {
    let name = safe_filename(offered, 255)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let name_path = Path::new(name);
    let stem = name_path.file_stem().unwrap_or(name);
    let extension = name_path.extension();

    for suffix in 0_u32..10_000 {
        let candidate_name = if suffix == 0 {
            name.to_owned()
        } else if let Some(extension) = extension {
            format!(
                "{}-{suffix}.{}",
                stem.to_string_lossy(),
                extension.to_string_lossy()
            )
            .into()
        } else {
            format!("{}-{suffix}", stem.to_string_lossy()).into()
        };
        let candidate = directory.join(candidate_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not find a collision-free filename",
    ))
}

pub fn validate_received_url(value: &str) -> Result<Url, SafetyError> {
    let parsed = Url::parse(value).map_err(|_| SafetyError::MalformedUrl)?;
    if matches!(parsed.scheme(), "http" | "https") {
        Ok(parsed)
    } else {
        Err(SafetyError::UrlSchemeNotAllowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_and_absolute_names_are_rejected() {
        for name in ["../secret", "/etc/passwd", "a/b", "."] {
            assert_eq!(
                safe_filename(Path::new(name), 255),
                Err(SafetyError::UnsafeFilename)
            );
        }
    }

    #[test]
    fn collision_never_overwrites() {
        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("photo.jpg"), b"old").unwrap();
        let (path, mut file) = reserve_destination(temp.path(), Path::new("photo.jpg")).unwrap();
        use std::io::Write;
        file.write_all(b"new").unwrap();
        assert_eq!(path.file_name().unwrap(), "photo-1.jpg");
        assert_eq!(fs::read(temp.path().join("photo.jpg")).unwrap(), b"old");
    }

    #[test]
    fn only_web_urls_are_actionable() {
        assert!(validate_received_url("https://example.com/path").is_ok());
        for value in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/plain,x",
        ] {
            assert_eq!(
                validate_received_url(value),
                Err(SafetyError::UrlSchemeNotAllowed)
            );
        }
    }

    #[test]
    fn unreasonable_offer_is_rejected_before_upload() {
        let limits = SafetyLimits::default();
        assert_eq!(
            limits.validate_offer(limits.max_incoming_bytes + 1, 1, 1),
            Err(SafetyError::TransferTooLarge)
        );
    }
}

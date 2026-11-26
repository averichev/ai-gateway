use std::{
    fs::File,
    io::{self, ErrorKind, Read, Write},
    path::Path,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use rand::{RngCore, rngs::OsRng};
use thiserror::Error;

use super::SecretCrypto;

const FILE_NAME: &str = "ai-gateway.key";

#[derive(Debug, Error)]
pub(crate) enum KeyFileError {
    #[error("cannot use {FILE_NAME} in the working directory: {0}")]
    Io(#[from] io::Error),
    #[error(
        "encrypted provider secrets already exist. Restore {FILE_NAME} in the working directory or set the original GATEWAY_MASTER_KEY"
    )]
    MissingForStoredSecrets,
}

pub(crate) fn load_or_create(
    directory: &Path,
    has_provider_secrets: bool,
) -> Result<SecretCrypto, KeyFileError> {
    let path = directory.join(FILE_NAME);
    match read_key(&path) {
        Ok(key) => return Ok(key),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    if has_provider_secrets {
        return Err(KeyFileError::MissingForStoredSecrets);
    }

    let mut key = [0u8; 32];
    OsRng.try_fill_bytes(&mut key).map_err(io::Error::other)?;
    // NamedTempFile is created privately (0600 on Unix). Publish only complete keys.
    let mut temp = tempfile::Builder::new()
        .prefix(".ai-gateway.key-")
        .tempfile_in(directory)?;
    writeln!(temp, "{}", STANDARD.encode(key))?;
    temp.as_file().sync_all()?;
    match temp.persist_noclobber(&path) {
        Ok(_) => {}
        Err(error) if error.error.kind() == ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.error.into()),
    }
    #[cfg(unix)]
    File::open(directory)?.sync_all()?;
    // Concurrent first starts must use the same bytes, including the losing writer.
    Ok(read_key(&path)?)
}

fn read_key(path: &Path) -> io::Result<SecretCrypto> {
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "expected a regular key file",
        ));
    }
    let mut encoded = String::new();
    // A base64-encoded 32-byte key occupies 44 bytes plus an optional line ending.
    file.take(47).read_to_string(&mut encoded)?;
    if encoded.len() > 46 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            "invalid master key file length",
        ));
    }
    SecretCrypto::from_base64(&encoded)
        .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Barrier, thread};

    use super::*;

    #[test]
    fn concurrent_first_starts_share_one_private_key() {
        let directory = tempfile::tempdir().unwrap();
        let barrier = Barrier::new(8);
        let keys = thread::scope(|scope| {
            let attempts: Vec<_> = (0..8)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        load_or_create(directory.path(), false).unwrap()
                    })
                })
                .collect();
            attempts
                .into_iter()
                .map(|attempt| attempt.join().unwrap())
                .collect::<Vec<_>>()
        });
        let encrypted = keys[0].encrypt("test credential").unwrap();
        for key in keys {
            assert_eq!(
                key.decrypt(&encrypted.ciphertext, &encrypted.nonce)
                    .unwrap(),
                "test credential"
            );
        }
        let path = directory.path().join(FILE_NAME);
        let encoded = fs::read_to_string(&path).unwrap();
        assert_eq!(STANDARD.decode(encoded.trim()).unwrap().len(), 32);
        assert_eq!(directory.path().read_dir().unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn malformed_key_file_is_preserved() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_NAME);
        for contents in ["", "invalid existing file", "YWJj"] {
            fs::write(&path, contents).unwrap();
            assert!(load_or_create(directory.path(), false).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), contents);
        }
    }
}

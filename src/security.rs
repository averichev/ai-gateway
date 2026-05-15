use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng as PasswordOsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use sha2::{Digest, Sha256};
use thiserror::Error;

const AES_GCM_NONCE_LEN: usize = 12;
const TOKEN_RANDOM_BYTES: usize = 32;

#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("invalid password hash: {0}")]
    PasswordHash(String),
    #[error("invalid master key: expected base64 encoded 32 byte key")]
    InvalidMasterKey,
    #[error("secret encryption failed")]
    Encrypt,
    #[error("secret decryption failed")]
    Decrypt,
}

#[derive(Debug, Clone)]
pub struct SecretCrypto {
    master_key: [u8; 32],
    key_version: i32,
}

#[derive(Debug, Clone)]
pub struct EncryptedSecret {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub algorithm: String,
    pub key_version: i32,
}

impl SecretCrypto {
    pub fn from_base64(master_key: &str) -> Result<Self, SecurityError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(master_key.trim())
            .map_err(|_| SecurityError::InvalidMasterKey)?;
        let master_key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| SecurityError::InvalidMasterKey)?;

        Ok(Self {
            master_key,
            key_version: 1,
        })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<EncryptedSecret, SecurityError> {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.master_key));
        let mut nonce = [0u8; AES_GCM_NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), plaintext.as_bytes())
            .map_err(|_| SecurityError::Encrypt)?;

        Ok(EncryptedSecret {
            ciphertext,
            nonce: nonce.to_vec(),
            algorithm: "AES-256-GCM".to_owned(),
            key_version: self.key_version,
        })
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, SecurityError> {
        if nonce.len() != AES_GCM_NONCE_LEN {
            return Err(SecurityError::Decrypt);
        }

        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.master_key));
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| SecurityError::Decrypt)?;

        String::from_utf8(plaintext).map_err(|_| SecurityError::Decrypt)
    }
}

pub fn hash_password(password: &str) -> Result<String, SecurityError> {
    let salt = SaltString::generate(&mut PasswordOsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| SecurityError::PasswordHash(error.to_string()))
}

pub fn verify_password(password: &str, password_hash: &str) -> Result<bool, SecurityError> {
    let parsed_hash = PasswordHash::new(password_hash)
        .map_err(|error| SecurityError::PasswordHash(error.to_string()))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn generate_admin_session_token() -> String {
    generate_token("ags_")
}

pub fn generate_gateway_client_token() -> String {
    generate_token("agc_")
}

pub fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn token_prefix(token: &str) -> String {
    token.chars().take(12).collect()
}

fn generate_token(prefix: &str) -> String {
    let mut bytes = [0u8; TOKEN_RANDOM_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(bytes))
}

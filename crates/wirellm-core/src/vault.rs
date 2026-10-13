use std::fs;
use std::sync::OnceLock;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use sha2::{Digest, Sha256};
use tracing::info;

use crate::db::get_wirellm_home;
use crate::error::OxllmError;

const VAULT_SALT_FILE: &str = ".vault_salt";
const VAULT_PREFIX: &str = "enc:v1:";
static VAULT_SALT_CACHE: OnceLock<[u8; 32]> = OnceLock::new();

/// Retrieves or creates a 32-byte cryptographic salt stored securely in ~/.wirellm/.vault_salt.
/// On Unix systems, permissions are restricted to 0600 (owner read/write only).
pub fn get_or_create_vault_salt() -> Result<[u8; 32], OxllmError> {
    if let Some(salt) = VAULT_SALT_CACHE.get() {
        return Ok(*salt);
    }

    let wirellm_dir = get_wirellm_home();
    let _ = fs::create_dir_all(&wirellm_dir);
    let salt_path = wirellm_dir.join(VAULT_SALT_FILE);

    let salt = if salt_path.exists() {
        let content = fs::read(&salt_path)
            .map_err(|e| OxllmError::Internal(format!("Failed to read vault salt: {}", e)))?;
        if content.len() == 32 {
            let mut s = [0u8; 32];
            s.copy_from_slice(&content);
            s
        } else if content.len() == 64 {
            let bytes = hex::decode(&content)
                .map_err(|e| OxllmError::Internal(format!("Invalid hex salt: {}", e)))?;
            if bytes.len() == 32 {
                let mut s = [0u8; 32];
                s.copy_from_slice(&bytes);
                s
            } else {
                return Err(OxllmError::Internal("Corrupted vault salt length".to_string()));
            }
        } else {
            return Err(OxllmError::Internal("Corrupted vault salt length".to_string()));
        }
    } else {
        // Generate new 32-byte cryptographically secure random salt
        let new_salt: [u8; 32] = rand::random();
        fs::write(&salt_path, &new_salt)
            .map_err(|e| OxllmError::Internal(format!("Failed to write vault salt: {}", e)))?;

        // Restrict permissions to owner read/write (0600) on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&salt_path, perms);
        }

        info!("Generated new secure vault salt at {:?}", salt_path);
        new_salt
    };

    let _ = VAULT_SALT_CACHE.set(salt);
    Ok(salt)
}

/// Derives a 256-bit AES-GCM encryption key from the vault salt and host context.
fn derive_vault_key() -> Result<[u8; 32], OxllmError> {
    let salt = get_or_create_vault_salt()?;
    let user = std::env::var("USER").unwrap_or_else(|_| "oxllm".to_string());

    let mut hasher = Sha256::new();
    hasher.update(&salt);
    hasher.update(b"::oxllm-vault-aes-256-gcm::");
    hasher.update(user.as_bytes());
    let result = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    Ok(key)
}

/// Encrypts an API key string using AES-256-GCM with a random 96-bit nonce and salt.
/// Returns string in format: `enc:v1:<hex_nonce>:<hex_ciphertext>`.
pub fn encrypt_key(plaintext: &str) -> Result<String, OxllmError> {
    if plaintext.is_empty() {
        return Ok(String::new());
    }

    let key = derive_vault_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| OxllmError::Internal(format!("Failed to initialize AES-GCM: {}", e)))?;

    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| OxllmError::Internal(format!("Encryption failed: {}", e)))?;

    Ok(format!(
        "{}{}:{}",
        VAULT_PREFIX,
        hex::encode(nonce_bytes),
        hex::encode(ciphertext)
    ))
}

/// Decrypts a vault ciphertext formatted as `enc:v1:<hex_nonce>:<hex_ciphertext>`.
pub fn decrypt_key(encrypted_str: &str) -> Result<String, OxllmError> {
    if encrypted_str.is_empty() {
        return Ok(String::new());
    }

    if !encrypted_str.starts_with(VAULT_PREFIX) {
        return Err(OxllmError::Internal(
            "Invalid vault ciphertext prefix".to_string(),
        ));
    }

    let rest = &encrypted_str[VAULT_PREFIX.len()..];
    let (nonce_hex, cipher_hex) = rest
        .split_once(':')
        .ok_or_else(|| OxllmError::Internal("Malformed encrypted key format".to_string()))?;

    let nonce_bytes = hex::decode(nonce_hex)
        .map_err(|e| OxllmError::Internal(format!("Invalid nonce hex: {}", e)))?;
    let ciphertext = hex::decode(cipher_hex)
        .map_err(|e| OxllmError::Internal(format!("Invalid ciphertext hex: {}", e)))?;

    if nonce_bytes.len() != 12 {
        return Err(OxllmError::Internal(
            "Invalid AES-GCM nonce length".to_string(),
        ));
    }

    let key = derive_vault_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| OxllmError::Internal(format!("Failed to initialize AES-GCM: {}", e)))?;

    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext_bytes = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| OxllmError::Internal(format!("Decryption failed (MAC mismatch): {}", e)))?;

    String::from_utf8(plaintext_bytes)
        .map_err(|e| OxllmError::Internal(format!("Decrypted bytes not UTF-8: {}", e)))
}

/// Returns a fully masked placeholder for a secret key (never exposing raw characters) or the env var template.
pub fn mask_key_preview(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('$') {
        return trimmed.to_string();
    }
    "••••••••••••".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_encryption_roundtrip() {
        let secret = "sk-ant-api03-test-token-secret-key-123456789";
        let encrypted = encrypt_key(secret).expect("encryption should succeed");
        assert!(encrypted.starts_with(VAULT_PREFIX));
        assert_ne!(encrypted, secret);

        let decrypted = decrypt_key(&encrypted).expect("decryption should succeed");
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn test_vault_different_nonces() {
        let secret = "my-secret-key";
        let enc1 = encrypt_key(secret).unwrap();
        let enc2 = encrypt_key(secret).unwrap();
        // Each encryption must produce a different ciphertext due to random nonce
        assert_ne!(enc1, enc2);

        assert_eq!(decrypt_key(&enc1).unwrap(), secret);
        assert_eq!(decrypt_key(&enc2).unwrap(), secret);
    }

    #[test]
    fn test_mask_key_preview() {
        assert_eq!(mask_key_preview("${MY_KEY}"), "${MY_KEY}");
        assert_eq!(mask_key_preview("$MY_KEY"), "$MY_KEY");
        assert_eq!(mask_key_preview("1234567890abcdef"), "••••••••••••");
        assert_eq!(mask_key_preview("short"), "••••••••••••");
    }
}

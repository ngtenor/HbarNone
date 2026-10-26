use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64_STD};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const VAULT_VERSION: u32 = 1;
const KDF_ID: &str = "argon2id-v1";
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const KEY_LENGTH: usize = 32;
const TAG_LENGTH: usize = 16;

fn argon2id() -> Result<Argon2<'static>, String> {
    let params = ParamsBuilder::new()
        .m_cost(64 * 1024)
        .t_cost(4)
        .p_cost(1)
        .output_len(KEY_LENGTH)
        .build()
        .map_err(|e| format!("argon2 params error: {e}"))?;

    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VaultFile {
    pub version: u32,
    pub kdf: String,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

impl VaultFile {
    pub fn encrypt(mnemonic: &str, password: &str) -> Result<Self, String> {
        let salt = rand::random::<[u8; 16]>();

        let argon2 = argon2id()?;
        let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key[..])
            .map_err(|e| format!("key derivation failed: {e}"))?;

        let cipher =
            Aes256Gcm::new_from_slice(&key[..]).map_err(|e| format!("cipher init failed: {e}"))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = mnemonic.as_bytes();
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("encryption failed: {e}"))?;

        Ok(Self {
            version: VAULT_VERSION,
            kdf: KDF_ID.to_string(),
            salt: BASE64_STD.encode(salt),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(ciphertext),
        })
    }

    pub fn decrypt(&self, password: &str) -> Result<Zeroizing<String>, String> {
        if self.version != VAULT_VERSION {
            return Err("unsupported vault version".to_string());
        }
        if self.kdf != KDF_ID {
            return Err("unsupported vault key derivation function".to_string());
        }

        let salt = BASE64_STD
            .decode(&self.salt)
            .map_err(|e| format!("invalid salt: {e}"))?;
        if salt.len() != SALT_LENGTH {
            return Err("invalid salt length".to_string());
        }

        let argon2 = argon2id()?;
        let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key[..])
            .map_err(|_| "wrong password or corrupted vault".to_string())?;

        let cipher =
            Aes256Gcm::new_from_slice(&key[..]).map_err(|e| format!("cipher init failed: {e}"))?;

        let nonce_bytes = hex::decode(&self.nonce).map_err(|e| format!("invalid nonce: {e}"))?;
        if nonce_bytes.len() != NONCE_LENGTH {
            return Err("invalid nonce length".to_string());
        }
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext =
            hex::decode(&self.ciphertext).map_err(|e| format!("invalid ciphertext: {e}"))?;
        if ciphertext.len() < TAG_LENGTH {
            return Err("invalid ciphertext length".to_string());
        }

        let plaintext = Zeroizing::new(
            cipher
                .decrypt(nonce, ciphertext.as_ref())
                .map_err(|_| "wrong password or corrupted vault".to_string())?,
        );

        let mnemonic = std::str::from_utf8(&plaintext).map_err(|e| format!("invalid utf8: {e}"))?;

        Ok(Zeroizing::new(mnemonic.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::VaultFile;

    #[test]
    fn round_trip_encrypt_decrypt_works() {
        let password = "correct horse battery staple";
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let vault = VaultFile::encrypt(mnemonic, password).unwrap();
        let decrypted = vault.decrypt(password).unwrap();

        assert_eq!(mnemonic, decrypted.as_str());
    }

    #[test]
    fn wrong_password_fails() {
        let password = "correct horse battery staple";
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let vault = VaultFile::encrypt(mnemonic, password).unwrap();
        let result = vault.decrypt("wrong password");

        assert!(result.is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let password = "correct horse battery staple";
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let mut vault = VaultFile::encrypt(mnemonic, password).unwrap();

        let mut chars: Vec<char> = vault.ciphertext.chars().collect();
        let last_index = chars.len() - 1;
        let last = chars[last_index];

        chars[last_index] = if last == '0' { '1' } else { '0' };
        vault.ciphertext = chars.into_iter().collect();

        let result = vault.decrypt(password);

        assert!(result.is_err());
    }

    #[test]
    fn malformed_nonce_is_rejected() {
        let mut vault = VaultFile::encrypt(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "correct horse battery staple",
        )
        .unwrap();
        vault.nonce = "00".to_string();

        assert_eq!(
            vault.decrypt("correct horse battery staple").unwrap_err(),
            "invalid nonce length"
        );
    }

    #[test]
    fn unsupported_kdf_is_rejected() {
        let mut vault = VaultFile::encrypt(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "correct horse battery staple",
        )
        .unwrap();
        vault.kdf = "unknown".to_string();

        assert_eq!(
            vault.decrypt("correct horse battery staple").unwrap_err(),
            "unsupported vault key derivation function"
        );
    }
}

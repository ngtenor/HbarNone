use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
    Nonce,
};
use argon2::{
    Algorithm,
    Argon2,
    ParamsBuilder,
    Version,
};
use base64::{
    engine::general_purpose::STANDARD as BASE64_STD,
    Engine,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

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

        let params = ParamsBuilder::new()
            .m_cost(64 * 1024)
            .t_cost(4)
            .p_cost(1)
            .output_len(32)
            .build()
            .map_err(|e| format!("argon2 params error: {e}"))?;

        let argon2 = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            params,
        );

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|e| format!("key derivation failed: {e}"))?;

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("cipher init failed: {e}"))?;

        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext = mnemonic.as_bytes();
        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| format!("encryption failed: {e}"))?;

        key.zeroize();

        Ok(Self {
            version: 1,
            kdf: "argon2id-v1".to_string(),
            salt: BASE64_STD.encode(salt),
            nonce: hex::encode(nonce_bytes),
            ciphertext: hex::encode(ciphertext),
        })
    }

    pub fn decrypt(&self, password: &str) -> Result<String, String> {
        if self.version != 1 {
            return Err("unsupported vault version".to_string());
        }

        let salt = BASE64_STD
            .decode(&self.salt)
            .map_err(|e| format!("invalid salt: {e}"))?;

        let params = ParamsBuilder::new()
            .m_cost(64 * 1024)
            .t_cost(4)
            .p_cost(1)
            .output_len(32)
            .build()
            .map_err(|e| format!("argon2 params error: {e}"))?;

        let argon2 = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            params,
        );

        let mut key = [0u8; 32];
        argon2
            .hash_password_into(password.as_bytes(), &salt, &mut key)
            .map_err(|_| "wrong password or corrupted vault".to_string())?;

        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| format!("cipher init failed: {e}"))?;

        let nonce_bytes = hex::decode(&self.nonce)
            .map_err(|e| format!("invalid nonce: {e}"))?;
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = hex::decode(&self.ciphertext)
            .map_err(|e| format!("invalid ciphertext: {e}"))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| "wrong password or corrupted vault".to_string())?;

        let mnemonic = String::from_utf8(plaintext)
            .map_err(|e| format!("invalid utf8: {e}"))?;

        key.zeroize();

        Ok(mnemonic)
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

        assert_eq!(mnemonic, decrypted);
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
}
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm,
    Nonce,
};

use argon2::{
    password_hash::{
        rand_core::OsRng,
        SaltString,
    },
    Argon2,
};

use serde::{Deserialize, Serialize};

#[derive(
    Serialize,
    Deserialize,
    Debug,
)]
pub struct VaultFile {
    pub version: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

impl VaultFile {
    pub fn encrypt(
        mnemonic: &str,
        password: &str,
    ) -> Self {
        let salt =
            SaltString::generate(&mut OsRng);

        let mut key = [0u8; 32];

        Argon2::default()
            .hash_password_into(
                password.as_bytes(),
                salt.as_str().as_bytes(),
                &mut key,
            )
            .expect(
                "Failed to derive key",
            );

        let cipher =
            Aes256Gcm::new_from_slice(&key)
                .expect(
                    "Invalid key length",
                );

        let nonce_bytes: [u8; 12] =
            rand::random();

        let nonce =
            Nonce::from_slice(
                &nonce_bytes,
            );

        let ciphertext = cipher
            .encrypt(
                nonce,
                mnemonic.as_bytes(),
            )
            .expect(
                "Encryption failed",
            );

        Self {
            version: 1,
            salt: salt.to_string(),
            nonce: hex::encode(
                nonce_bytes,
            ),
            ciphertext: hex::encode(
                ciphertext,
            ),
        }
    }

    pub fn decrypt(
        &self,
        password: &str,
    ) -> String {
        let mut key = [0u8; 32];

        Argon2::default()
            .hash_password_into(
                password.as_bytes(),
                self.salt.as_bytes(),
                &mut key,
            )
            .expect(
                "Failed to derive key",
            );

        let cipher =
            Aes256Gcm::new_from_slice(&key)
                .expect(
                    "Invalid key length",
                );

        let nonce_bytes =
            hex::decode(
                &self.nonce,
            )
            .expect(
                "Invalid nonce",
            );

        let nonce =
            Nonce::from_slice(
                &nonce_bytes,
            );

        let ciphertext =
            hex::decode(
                &self.ciphertext,
            )
            .expect(
                "Invalid ciphertext",
            );

        let plaintext = cipher
            .decrypt(
                nonce,
                ciphertext.as_ref(),
            )
            .expect(
                "Decryption failed",
            );

        String::from_utf8(
            plaintext,
        )
        .expect(
            "Invalid UTF-8 data",
        )
    }
}
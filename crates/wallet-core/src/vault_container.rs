use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
use hex::{decode, encode};
use rand::random;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;
use zeroize::Zeroizing;

const VAULT_VERSION: u32 = 2;
const KDF_ID: &str = "argon2id-v1";
const SALT_LENGTH: usize = 16;
const KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const TAG_LENGTH: usize = 16;

pub const DEFAULT_AUTO_LOCK_SECONDS: u32 = 300;
pub const MIN_AUTO_LOCK_SECONDS: u32 = 30;
pub const MAX_AUTO_LOCK_SECONDS: u32 = 1800;

/// A Hedera network an account lives on. Only Testnet is reachable for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Testnet,
    Mainnet,
}

/// An account that the wallet's key controls on one network. Two entries are
/// the same account when network and ID match, whatever their names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletAccount {
    pub network: Network,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl PartialEq for WalletAccount {
    fn eq(&self, other: &Self) -> bool {
        self.network == other.network && self.id == other.id
    }
}

impl Eq for WalletAccount {}

/// A named recipient outside this vault's wallets (a third party).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddressBookEntry {
    pub network: Network,
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultWallet {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub accounts: Vec<WalletAccount>,
    /// Single linked account of vaults saved before wallets held several.
    /// Moved into `accounts` when the vault is opened and never written back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default)]
    pub secret_kind: crate::key_import::SecretKind,
    #[serde(default)]
    pub key_type: crate::key_import::KeyType,
    pub(crate) nonce: String,
    pub(crate) ciphertext: String,
}

impl VaultWallet {
    fn migrate_legacy_account(&mut self) {
        if let Some(id) = self.account_id.take() {
            let account = WalletAccount {
                network: Network::Testnet,
                id,
                name: None,
            };
            if !self.accounts.contains(&account) {
                self.accounts.push(account);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPayload {
    pub wallets: Vec<VaultWallet>,
    pub auto_lock_seconds: u32,
    #[serde(default)]
    pub address_book: Vec<AddressBookEntry>,
    /// Whether HBAR may be sent to accounts that are not in the address book.
    #[serde(default)]
    pub allow_external_sends: bool,
    /// Whether a deleted account's funds may go to an account that is not in
    /// the address book.
    #[serde(default)]
    pub allow_external_deletes: bool,
}

impl Default for VaultPayload {
    fn default() -> Self {
        Self {
            wallets: Vec::new(),
            auto_lock_seconds: DEFAULT_AUTO_LOCK_SECONDS,
            address_book: Vec::new(),
            allow_external_sends: false,
            allow_external_deletes: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultContainer {
    pub version: u32,
    pub kdf: String,
    pub salt: String,
    pub key_nonce: String,
    pub wrapped_key: String,
    pub payload_nonce: String,
    pub payload_ciphertext: String,
}

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

fn derive_key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LENGTH]>, String> {
    let argon2 = argon2id()?;
    let mut key = Zeroizing::new([0u8; KEY_LENGTH]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key[..])
        .map_err(|_| "wrong password or corrupted vault".to_string())?;
    Ok(key)
}

fn encrypt(key: &[u8; KEY_LENGTH], plaintext: &[u8]) -> Result<(String, String), String> {
    let nonce_bytes: [u8; NONCE_LENGTH] = random();
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("cipher init failed: {e}"))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|e| format!("encryption failed: {e}"))?;
    Ok((encode(nonce_bytes), encode(ciphertext)))
}

fn decrypt(
    key: &[u8; KEY_LENGTH],
    nonce: &str,
    ciphertext: &str,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let nonce = decode(nonce).map_err(|e| format!("invalid nonce: {e}"))?;
    if nonce.len() != NONCE_LENGTH {
        return Err("invalid nonce length".to_string());
    }

    let ciphertext = decode(ciphertext).map_err(|e| format!("invalid ciphertext: {e}"))?;
    if ciphertext.len() < TAG_LENGTH {
        return Err("invalid ciphertext length".to_string());
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("cipher init failed: {e}"))?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
        .map_err(|_| "wrong password or corrupted vault".to_string())?;
    Ok(Zeroizing::new(plaintext))
}

impl VaultContainer {
    pub fn create(
        password: &str,
        auto_lock_seconds: u32,
    ) -> Result<(Self, Zeroizing<[u8; KEY_LENGTH]>), String> {
        validate_auto_lock_seconds(auto_lock_seconds)?;
        let salt: [u8; SALT_LENGTH] = random();
        let password_key = derive_key(password, &salt)?;
        let master_key = Zeroizing::new(random::<[u8; KEY_LENGTH]>());
        let (key_nonce, wrapped_key) = encrypt(&password_key, &master_key[..])?;
        let mut container = Self {
            version: VAULT_VERSION,
            kdf: KDF_ID.to_string(),
            salt: encode(salt),
            key_nonce,
            wrapped_key,
            payload_nonce: String::new(),
            payload_ciphertext: String::new(),
        };
        let payload = VaultPayload {
            auto_lock_seconds,
            ..VaultPayload::default()
        };
        container.encrypt_payload(&master_key, &payload)?;
        Ok((container, master_key))
    }

    pub fn unlock(
        &self,
        password: &str,
    ) -> Result<(Zeroizing<[u8; KEY_LENGTH]>, VaultPayload), String> {
        if self.version != VAULT_VERSION {
            return Err("unsupported vault version".to_string());
        }
        if self.kdf != KDF_ID {
            return Err("unsupported vault key derivation function".to_string());
        }
        let salt = decode(&self.salt).map_err(|e| format!("invalid salt: {e}"))?;
        if salt.len() != SALT_LENGTH {
            return Err("invalid salt length".to_string());
        }
        let password_key = derive_key(password, &salt)?;
        let master_key_bytes = decrypt(&password_key, &self.key_nonce, &self.wrapped_key)?;
        if master_key_bytes.len() != KEY_LENGTH {
            return Err("invalid wrapped vault key length".to_string());
        }
        let mut master_key = Zeroizing::new([0u8; KEY_LENGTH]);
        master_key.copy_from_slice(&master_key_bytes);

        let plaintext = decrypt(&master_key, &self.payload_nonce, &self.payload_ciphertext)?;
        let mut payload: VaultPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| format!("deserialize vault contents failed: {e}"))?;
        for wallet in &mut payload.wallets {
            wallet.migrate_legacy_account();
        }
        validate_auto_lock_seconds(payload.auto_lock_seconds)?;
        Ok((master_key, payload))
    }

    /// Re-wraps the existing master key under a new password. Wallet data is
    /// not re-encrypted, and the current password must be correct.
    pub fn with_changed_password(
        &self,
        current_password: &str,
        new_password: &str,
    ) -> Result<Self, String> {
        let (master_key, _) = self
            .unlock(current_password)
            .map_err(|_| "incorrect current vault password".to_string())?;
        let salt: [u8; SALT_LENGTH] = random();
        let password_key = derive_key(new_password, &salt)?;
        let (key_nonce, wrapped_key) = encrypt(&password_key, &master_key[..])?;
        Ok(Self {
            salt: encode(salt),
            key_nonce,
            wrapped_key,
            ..self.clone()
        })
    }

    pub fn encrypt_payload(
        &mut self,
        master_key: &[u8; KEY_LENGTH],
        payload: &VaultPayload,
    ) -> Result<(), String> {
        let plaintext = Zeroizing::new(
            serde_json::to_vec(payload)
                .map_err(|e| format!("serialize vault contents failed: {e}"))?,
        );
        (self.payload_nonce, self.payload_ciphertext) = encrypt(master_key, &plaintext)?;
        Ok(())
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize vault failed: {e}"))?;
        let path = Path::new(path);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temp_file = NamedTempFile::new_in(parent)
            .map_err(|e| format!("create temporary vault failed: {e}"))?;
        temp_file
            .write_all(json.as_bytes())
            .map_err(|e| format!("write temporary vault failed: {e}"))?;
        temp_file
            .as_file()
            .sync_all()
            .map_err(|e| format!("sync temporary vault failed: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            temp_file
                .as_file()
                .set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("restrict vault permissions failed: {e}"))?;
        }
        temp_file
            .persist(path)
            .map_err(|e| format!("replace vault failed: {}", e.error))?;
        Ok(())
    }
}

pub fn encrypt_wallet_secret(
    master_key: &[u8; KEY_LENGTH],
    secret: &str,
) -> Result<(String, String), String> {
    encrypt(master_key, secret.as_bytes())
}

pub fn decrypt_wallet_secret(
    master_key: &[u8; KEY_LENGTH],
    nonce: &str,
    ciphertext: &str,
) -> Result<Zeroizing<String>, String> {
    let plaintext = decrypt(master_key, nonce, ciphertext)?;
    let secret = std::str::from_utf8(&plaintext)
        .map_err(|e| format!("invalid wallet secret encoding: {e}"))?;
    Ok(Zeroizing::new(secret.to_string()))
}

pub fn validate_auto_lock_seconds(seconds: u32) -> Result<(), String> {
    if !(MIN_AUTO_LOCK_SECONDS..=MAX_AUTO_LOCK_SECONDS).contains(&seconds) {
        return Err(format!(
            "auto-lock timeout must be between {MIN_AUTO_LOCK_SECONDS} and {MAX_AUTO_LOCK_SECONDS} seconds"
        ));
    }
    Ok(())
}

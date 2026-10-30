use crate::{
    vault_container::{
        DEFAULT_AUTO_LOCK_SECONDS, VaultContainer, VaultPayload, VaultWallet,
        decrypt_wallet_secret, encrypt_wallet_secret, validate_auto_lock_seconds,
    },
    wallet::Wallet,
    wallet_store::WalletStore,
};
use rand::random;
use serde::Serialize;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub const MAX_BACKUP_BYTES: usize = 10 * 1024 * 1024;
const FREE_UNLOCK_ATTEMPTS: u32 = 3;
const BASE_UNLOCK_DELAY_SECONDS: u64 = 5;
const MAX_UNLOCK_DELAY_SECONDS: u64 = 300;

// Delay doubles with each failure beyond the free attempts, up to a cap.
fn unlock_delay(failed_attempts: u32) -> Option<Duration> {
    let excess = failed_attempts.checked_sub(FREE_UNLOCK_ATTEMPTS)?;
    let seconds = BASE_UNLOCK_DELAY_SECONDS
        .checked_shl(excess.min(16))
        .unwrap_or(MAX_UNLOCK_DELAY_SECONDS)
        .min(MAX_UNLOCK_DELAY_SECONDS);
    Some(Duration::from_secs(seconds))
}

#[derive(Debug, Clone, Serialize)]
pub struct WalletSummary {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

pub struct LegacyWalletPassword {
    pub id: String,
    pub password: Zeroizing<String>,
}

pub struct WalletService {
    container: Option<VaultContainer>,
    legacy_store: Option<WalletStore>,
    master_key: Option<Zeroizing<[u8; 32]>>,
    payload: Option<VaultPayload>,
    last_activity: Option<Instant>,
    auto_lock_seconds: u32,
    failed_unlock_attempts: u32,
    unlock_blocked_until: Option<Instant>,
}

impl WalletService {
    pub fn new() -> Self {
        Self {
            container: None,
            legacy_store: None,
            master_key: None,
            payload: None,
            last_activity: None,
            auto_lock_seconds: DEFAULT_AUTO_LOCK_SECONDS,
            failed_unlock_attempts: 0,
            unlock_blocked_until: None,
        }
    }

    pub fn load_store(path: &str) -> Result<Self, String> {
        if !std::path::Path::new(path).exists() {
            return Ok(Self::new());
        }

        let contents =
            std::fs::read_to_string(path).map_err(|e| format!("read wallet store failed: {e}"))?;
        let json: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| format!("deserialize wallet store failed: {e}"))?;

        if json.get("wrapped_key").is_some() {
            let container: VaultContainer = serde_json::from_value(json)
                .map_err(|e| format!("deserialize vault failed: {e}"))?;
            Ok(Self {
                container: Some(container),
                ..Self::new()
            })
        } else {
            let legacy_store: WalletStore = serde_json::from_value(json)
                .map_err(|e| format!("deserialize legacy wallet store failed: {e}"))?;
            Self::validate_legacy_store(&legacy_store)?;
            Ok(Self {
                legacy_store: Some(legacy_store),
                ..Self::new()
            })
        }
    }

    fn validate_legacy_store(store: &WalletStore) -> Result<(), String> {
        let mut ids = HashSet::with_capacity(store.wallets.len());
        for wallet in &store.wallets {
            if wallet.id.trim().is_empty() || !ids.insert(&wallet.id) {
                return Err("wallet store contains an empty or duplicate wallet ID".to_string());
            }
        }
        Ok(())
    }

    pub fn vault_status(&self) -> &'static str {
        if self.master_key.is_some() {
            "unlocked"
        } else if self.legacy_store.is_some() {
            "legacy"
        } else if self.container.is_some() {
            "locked"
        } else {
            "uninitialized"
        }
    }

    pub fn auto_lock_seconds(&self) -> u32 {
        self.auto_lock_seconds
    }

    pub fn legacy_wallets(&self) -> Vec<WalletSummary> {
        self.legacy_store
            .as_ref()
            .map(|store| {
                store
                    .list_wallets()
                    .iter()
                    .map(|wallet| WalletSummary {
                        id: wallet.id.clone(),
                        name: wallet.name.clone(),
                        created_at: wallet.created_at.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn change_vault_password(
        &mut self,
        current_password: &str,
        new_password: &str,
        path: &str,
    ) -> Result<(), String> {
        self.ensure_unlocked()?;
        if current_password.is_empty() || current_password.len() > 1024 {
            return Err("vault password must contain between 1 and 1024 bytes".to_string());
        }
        if new_password.is_empty() || new_password.len() > 1024 {
            return Err("new vault password must contain between 1 and 1024 bytes".to_string());
        }
        if new_password == current_password {
            return Err("the new password must differ from the current password".to_string());
        }
        let updated = self
            .container
            .as_ref()
            .ok_or_else(|| "vault has not been created".to_string())?
            .with_changed_password(current_password, new_password)?;
        updated.save_to_file(path)?;
        self.container = Some(updated);
        Ok(())
    }

    pub fn set_auto_lock_seconds(&mut self, seconds: u32, path: &str) -> Result<(), String> {
        validate_auto_lock_seconds(seconds)?;
        if self.container.is_some() {
            self.ensure_unlocked()?;
            let mut updated_payload = self.payload.as_ref().expect("checked unlocked").clone();
            updated_payload.auto_lock_seconds = seconds;
            self.persist_payload(updated_payload, path)?;
        }
        self.auto_lock_seconds = seconds;
        Ok(())
    }

    pub fn create_vault(&mut self, password: &str, path: &str) -> Result<(), String> {
        if self.container.is_some() || self.legacy_store.is_some() {
            return Err("a vault already exists".to_string());
        }
        if password.is_empty() || password.len() > 1024 {
            return Err("vault password must contain between 1 and 1024 bytes".to_string());
        }

        let (container, master_key) = VaultContainer::create(password, self.auto_lock_seconds)?;
        container.save_to_file(path)?;
        self.container = Some(container);
        self.master_key = Some(master_key);
        self.payload = Some(VaultPayload::default());
        self.last_activity = Some(Instant::now());
        Ok(())
    }

    pub fn unlock_vault(&mut self, password: &str) -> Result<Vec<WalletSummary>, String> {
        if password.is_empty() || password.len() > 1024 {
            return Err("vault password must contain between 1 and 1024 bytes".to_string());
        }
        let container = self
            .container
            .as_ref()
            .ok_or_else(|| "vault has not been created".to_string())?;
        if let Some(blocked_until) = self.unlock_blocked_until {
            let remaining = blocked_until.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                return Err(format!(
                    "too many failed unlock attempts; try again in {} seconds",
                    remaining.as_secs() + 1
                ));
            }
        }
        let (master_key, payload) = match container.unlock(password) {
            Ok(unlocked) => unlocked,
            Err(error) => {
                self.failed_unlock_attempts = self.failed_unlock_attempts.saturating_add(1);
                self.unlock_blocked_until =
                    unlock_delay(self.failed_unlock_attempts).map(|delay| Instant::now() + delay);
                return Err(error);
            }
        };
        self.failed_unlock_attempts = 0;
        self.unlock_blocked_until = None;
        let summaries = summarize(&payload);
        self.auto_lock_seconds = payload.auto_lock_seconds;
        self.master_key = Some(master_key);
        self.payload = Some(payload);
        self.last_activity = Some(Instant::now());
        Ok(summaries)
    }

    pub fn migrate_legacy_vault(
        &mut self,
        vault_password: &str,
        legacy_passwords: &[LegacyWalletPassword],
        auto_lock_seconds: u32,
        path: &str,
    ) -> Result<Vec<WalletSummary>, String> {
        if vault_password.is_empty() || vault_password.len() > 1024 {
            return Err("vault password must contain between 1 and 1024 bytes".to_string());
        }
        validate_auto_lock_seconds(auto_lock_seconds)?;
        let legacy = self
            .legacy_store
            .as_ref()
            .ok_or_else(|| "there are no legacy wallets to migrate".to_string())?;
        if legacy_passwords.len() != legacy.wallets.len() {
            return Err("enter the current password for every existing wallet".to_string());
        }
        let mut credential_ids = HashSet::with_capacity(legacy_passwords.len());
        if legacy_passwords
            .iter()
            .any(|credential| !credential_ids.insert(credential.id.as_str()))
        {
            return Err("duplicate legacy wallet credentials were provided".to_string());
        }

        let (mut container, master_key) =
            VaultContainer::create(vault_password, auto_lock_seconds)?;
        let mut payload = VaultPayload::default();
        for legacy_wallet in legacy.list_wallets() {
            let credential = legacy_passwords
                .iter()
                .find(|credential| credential.id == legacy_wallet.id)
                .ok_or_else(|| format!("missing password for wallet '{}'", legacy_wallet.name))?;
            let mnemonic = legacy_wallet
                .vault
                .decrypt(credential.password.as_str())
                .map_err(|_| format!("incorrect password for wallet '{}'", legacy_wallet.name))?;
            Wallet::from_mnemonic(mnemonic.as_str()).map_err(|_| {
                format!(
                    "wallet '{}' contains an invalid recovery phrase",
                    legacy_wallet.name
                )
            })?;
            let (nonce, ciphertext) = encrypt_wallet_secret(&master_key, mnemonic.as_str())?;
            payload.wallets.push(VaultWallet {
                id: legacy_wallet.id.clone(),
                name: legacy_wallet.name.clone(),
                created_at: legacy_wallet.created_at.clone(),
                updated_at: legacy_wallet.updated_at.clone(),
                nonce,
                ciphertext,
            });
        }
        container.encrypt_payload(&master_key, &payload)?;
        container.save_to_file(path)?;

        let summaries = summarize(&payload);
        self.container = Some(container);
        self.legacy_store = None;
        self.master_key = Some(master_key);
        self.payload = Some(payload);
        self.last_activity = Some(Instant::now());
        self.auto_lock_seconds = auto_lock_seconds;
        Ok(summaries)
    }

    /// Returns the encrypted vault file contents. It is protected by the vault
    /// password and contains no plaintext wallet data.
    pub fn export_backup(&mut self) -> Result<String, String> {
        self.ensure_unlocked()?;
        let container = self
            .container
            .as_ref()
            .ok_or_else(|| "vault has not been created".to_string())?;
        serde_json::to_string_pretty(container).map_err(|e| format!("serialize backup failed: {e}"))
    }

    /// Replaces the current vault with a verified backup. The previous vault
    /// file is kept beside it as `<path>.pre-restore`. A restore is allowed
    /// while the vault is unlocked, or when no vault exists yet.
    pub fn restore_backup(
        &mut self,
        backup_json: &str,
        backup_password: &str,
        path: &str,
    ) -> Result<Vec<WalletSummary>, String> {
        if self.vault_status() != "uninitialized" {
            self.ensure_unlocked()?;
        }
        if backup_password.is_empty() || backup_password.len() > 1024 {
            return Err("backup password must contain between 1 and 1024 bytes".to_string());
        }
        if backup_json.len() > MAX_BACKUP_BYTES {
            return Err("backup file is too large".to_string());
        }
        let json: serde_json::Value = serde_json::from_str(backup_json)
            .map_err(|_| "the selected file is not a HederaVault backup".to_string())?;
        if json.get("wrapped_key").is_none() {
            return Err("the selected file is not a HederaVault backup".to_string());
        }
        let container: VaultContainer = serde_json::from_value(json)
            .map_err(|_| "the selected file is not a HederaVault backup".to_string())?;
        let (master_key, payload) = container
            .unlock(backup_password)
            .map_err(|_| "incorrect backup password or damaged backup".to_string())?;

        let mut ids = HashSet::with_capacity(payload.wallets.len());
        for wallet in &payload.wallets {
            if wallet.id.trim().is_empty() || !ids.insert(wallet.id.as_str()) {
                return Err("backup contains an empty or duplicate wallet ID".to_string());
            }
            let mnemonic = decrypt_wallet_secret(&master_key, &wallet.nonce, &wallet.ciphertext)
                .map_err(|_| "backup contains a damaged wallet".to_string())?;
            Wallet::from_mnemonic(mnemonic.as_str())
                .map_err(|_| "backup contains an invalid recovery phrase".to_string())?;
        }

        let previous = std::path::PathBuf::from(format!("{path}.pre-restore"));
        if std::path::Path::new(path).exists() {
            std::fs::copy(path, &previous)
                .map_err(|e| format!("could not keep a copy of the current vault: {e}"))?;
        }
        container.save_to_file(path)?;

        let summaries = summarize(&payload);
        self.auto_lock_seconds = payload.auto_lock_seconds;
        self.container = Some(container);
        self.legacy_store = None;
        self.master_key = Some(master_key);
        self.payload = Some(payload);
        self.last_activity = Some(Instant::now());
        Ok(summaries)
    }

    pub fn record_activity(&mut self) -> Result<(), String> {
        self.ensure_unlocked()?;
        self.last_activity = Some(Instant::now());
        Ok(())
    }

    pub fn lock(&mut self) {
        self.master_key = None;
        self.payload = None;
        self.last_activity = None;
    }

    fn ensure_unlocked(&mut self) -> Result<(), String> {
        if self.master_key.is_none() || self.payload.is_none() {
            return Err("vault is locked".to_string());
        }
        if self.last_activity.is_some_and(|last| {
            last.elapsed() >= Duration::from_secs(self.auto_lock_seconds.into())
        }) {
            self.lock();
            return Err("vault locked after inactivity".to_string());
        }
        self.last_activity = Some(Instant::now());
        Ok(())
    }

    pub fn list_wallets(&mut self) -> Result<Vec<WalletSummary>, String> {
        self.ensure_unlocked()?;
        Ok(summarize(self.payload.as_ref().expect("checked unlocked")))
    }

    pub fn create_and_save_wallet(
        &mut self,
        name: &str,
        path: &str,
    ) -> Result<WalletSummary, String> {
        let wallet = Wallet::generate()?;
        self.store_wallet(name, wallet.mnemonic(), path)
    }

    pub fn import_and_save_wallet(
        &mut self,
        name: &str,
        mnemonic: &str,
        path: &str,
    ) -> Result<WalletSummary, String> {
        if mnemonic.is_empty() {
            return Err("recovery phrase cannot be empty".to_string());
        }
        if mnemonic.len() > 1024 {
            return Err("recovery phrase cannot exceed 1024 bytes".to_string());
        }
        let wallet = Wallet::from_mnemonic(mnemonic)?;
        self.store_wallet(name, wallet.mnemonic(), path)
    }

    fn store_wallet(
        &mut self,
        name: &str,
        mnemonic: &str,
        path: &str,
    ) -> Result<WalletSummary, String> {
        self.ensure_unlocked()?;
        let master_key = self.master_key.as_ref().expect("checked unlocked");
        let current_payload = self.payload.as_ref().expect("checked unlocked");
        let mut next_payload = current_payload.clone();
        let id = loop {
            let candidate = format!("wallet-{}", hex::encode(random::<[u8; 16]>()));
            if !next_payload
                .wallets
                .iter()
                .any(|wallet| wallet.id == candidate)
            {
                break candidate;
            }
        };
        let now = chrono::Utc::now().to_rfc3339();
        let (nonce, ciphertext) = encrypt_wallet_secret(master_key, mnemonic)?;
        let wallet = VaultWallet {
            id,
            name: name.to_string(),
            created_at: now.clone(),
            updated_at: now,
            nonce,
            ciphertext,
        };
        let summary = WalletSummary {
            id: wallet.id.clone(),
            name: wallet.name.clone(),
            created_at: wallet.created_at.clone(),
        };
        next_payload.wallets.push(wallet);
        self.persist_payload(next_payload, path)?;
        Ok(summary)
    }

    pub fn delete_wallet_and_save(&mut self, wallet_id: &str, path: &str) -> Result<(), String> {
        self.ensure_unlocked()?;
        let mut next_payload = self.payload.as_ref().expect("checked unlocked").clone();
        let index = next_payload
            .wallets
            .iter()
            .position(|wallet| wallet.id == wallet_id)
            .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;
        next_payload.wallets.remove(index);
        self.persist_payload(next_payload, path)
    }

    pub fn reveal_mnemonic(&mut self, wallet_id: &str) -> Result<Zeroizing<String>, String> {
        self.ensure_unlocked()?;
        let wallet = self
            .payload
            .as_ref()
            .expect("checked unlocked")
            .wallets
            .iter()
            .find(|wallet| wallet.id == wallet_id)
            .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;
        decrypt_wallet_secret(
            self.master_key.as_ref().expect("checked unlocked"),
            &wallet.nonce,
            &wallet.ciphertext,
        )
    }

    fn persist_payload(&mut self, payload: VaultPayload, path: &str) -> Result<(), String> {
        let mut container = self
            .container
            .as_ref()
            .ok_or_else(|| "vault has not been created".to_string())?
            .clone();
        container.encrypt_payload(
            self.master_key
                .as_ref()
                .ok_or_else(|| "vault is locked".to_string())?,
            &payload,
        )?;
        container.save_to_file(path)?;
        self.container = Some(container);
        self.payload = Some(payload);
        self.last_activity = Some(Instant::now());
        Ok(())
    }

    #[cfg(test)]
    fn expire_for_test(&mut self) {
        self.last_activity =
            Some(Instant::now() - Duration::from_secs(self.auto_lock_seconds.into()));
    }
}

impl Default for WalletService {
    fn default() -> Self {
        Self::new()
    }
}

fn summarize(payload: &VaultPayload) -> Vec<WalletSummary> {
    payload
        .wallets
        .iter()
        .map(|wallet| WalletSummary {
            id: wallet.id.clone(),
            name: wallet.name.clone(),
            created_at: wallet.created_at.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{LegacyWalletPassword, WalletService};
    use crate::{vault::VaultFile, wallet::Wallet, wallet_store::WalletStore};
    use std::time::{Duration, Instant};
    use zeroize::Zeroizing;

    fn temp_path() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vault.json");
        (directory, path)
    }

    fn legacy_store(path: &str, old_password: &str) -> String {
        let wallet = Wallet::from_mnemonic(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        )
        .unwrap();
        let mut store = WalletStore::new();
        store
            .add_wallet(crate::wallet_store::WalletRecord {
                id: "legacy-wallet".to_string(),
                name: "Legacy".to_string(),
                vault: VaultFile::encrypt(wallet.mnemonic(), old_password).unwrap(),
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-01T00:00:00Z".to_string(),
            })
            .unwrap();
        store.save_to_file(path).unwrap();
        wallet.public_key
    }

    fn credential(id: &str, password: &str) -> LegacyWalletPassword {
        LegacyWalletPassword {
            id: id.to_string(),
            password: Zeroizing::new(password.to_string()),
        }
    }

    #[test]
    fn repeated_failed_unlocks_are_delayed_and_success_resets_the_counter() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        service.lock();

        for _ in 0..3 {
            assert!(service.unlock_vault("wrong").is_err());
        }
        assert!(service.unlock_blocked_until.is_some());
        let error = service.unlock_vault("vault password").unwrap_err();
        assert!(error.contains("too many failed unlock attempts"));
        assert_eq!(service.vault_status(), "locked");

        service.unlock_blocked_until = Some(Instant::now() - Duration::from_secs(1));
        service.unlock_vault("vault password").unwrap();
        assert_eq!(service.failed_unlock_attempts, 0);
        assert!(service.unlock_blocked_until.is_none());
    }

    #[test]
    fn changing_the_password_rewraps_the_key_and_keeps_wallets() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("old password", path).unwrap();
        let created = service.create_and_save_wallet("Keep", path).unwrap();
        let phrase = service.reveal_mnemonic(&created.id).unwrap().to_string();
        let before = std::fs::read_to_string(path).unwrap();

        assert!(
            service
                .change_vault_password("wrong password", "new password", path)
                .is_err()
        );
        assert!(
            service
                .change_vault_password("old password", "old password", path)
                .is_err()
        );
        assert!(
            service
                .change_vault_password("old password", "", path)
                .is_err()
        );
        let invalid_path = format!("{path}.missing/vault.json");
        assert!(
            service
                .change_vault_password("old password", "new password", &invalid_path)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), before);

        service
            .change_vault_password("old password", "new password", path)
            .unwrap();
        assert_ne!(std::fs::read_to_string(path).unwrap(), before);
        service.lock();

        let mut reloaded = WalletService::load_store(path).unwrap();
        assert!(reloaded.unlock_vault("old password").is_err());
        reloaded.unlock_vault("new password").unwrap();
        assert_eq!(reloaded.list_wallets().unwrap()[0].id, created.id);
        assert_eq!(
            reloaded.reveal_mnemonic(&created.id).unwrap().as_str(),
            phrase
        );
    }

    #[test]
    fn changing_the_password_requires_an_unlocked_vault() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("old password", path).unwrap();
        service.lock();
        assert!(
            service
                .change_vault_password("old password", "new password", path)
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn saved_vault_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        let mode = std::fs::metadata(path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn backup_round_trips_to_a_new_machine_without_plaintext() {
        let (_source_directory, source_path) = temp_path();
        let source_path = source_path.to_str().unwrap();
        let mut source = WalletService::new();
        source.create_vault("backup password", source_path).unwrap();
        let first = source.create_and_save_wallet("First", source_path).unwrap();
        let second = source
            .create_and_save_wallet("Second", source_path)
            .unwrap();
        let first_phrase = source.reveal_mnemonic(&first.id).unwrap().to_string();
        let backup = source.export_backup().unwrap();
        assert!(!backup.contains(&first_phrase));
        assert!(!backup.contains("First"));

        let (_target_directory, target_path) = temp_path();
        let target_path = target_path.to_str().unwrap();
        let mut target = WalletService::new();
        assert_eq!(target.vault_status(), "uninitialized");
        let restored = target
            .restore_backup(&backup, "backup password", target_path)
            .unwrap();
        assert_eq!(restored.len(), 2);
        assert_eq!(target.vault_status(), "unlocked");
        assert_eq!(
            target.reveal_mnemonic(&first.id).unwrap().as_str(),
            first_phrase
        );
        assert!(
            target
                .list_wallets()
                .unwrap()
                .iter()
                .any(|w| w.id == second.id)
        );
        target.lock();

        let mut reloaded = WalletService::load_store(target_path).unwrap();
        reloaded.unlock_vault("backup password").unwrap();
        assert_eq!(reloaded.list_wallets().unwrap().len(), 2);
    }

    #[test]
    fn restore_rejects_bad_backups_and_leaves_the_vault_unchanged() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        service.create_and_save_wallet("Keep", path).unwrap();
        let backup = service.export_backup().unwrap();
        let before = std::fs::read_to_string(path).unwrap();

        assert!(
            service
                .restore_backup(&backup, "wrong password", path)
                .is_err()
        );
        assert!(
            service
                .restore_backup("not json", "vault password", path)
                .is_err()
        );
        assert!(
            service
                .restore_backup("{\"hello\":1}", "vault password", path)
                .is_err()
        );
        assert!(service.restore_backup(&backup, "", path).is_err());
        let oversized = " ".repeat(super::MAX_BACKUP_BYTES + 1);
        assert!(
            service
                .restore_backup(&oversized, "vault password", path)
                .is_err()
        );
        let invalid_path = format!("{path}.missing/vault.json");
        assert!(
            service
                .restore_backup(&backup, "vault password", &invalid_path)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), before);
        assert_eq!(service.vault_status(), "unlocked");
        assert_eq!(service.list_wallets().unwrap().len(), 1);
    }

    #[test]
    fn restore_over_an_existing_vault_keeps_a_pre_restore_copy() {
        let (_source_directory, source_path) = temp_path();
        let source_path = source_path.to_str().unwrap();
        let mut source = WalletService::new();
        source.create_vault("source password", source_path).unwrap();
        let source_wallet = source
            .create_and_save_wallet("Source", source_path)
            .unwrap();
        let backup = source.export_backup().unwrap();

        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("local password", path).unwrap();
        service.create_and_save_wallet("Local", path).unwrap();
        let before = std::fs::read_to_string(path).unwrap();

        let restored = service
            .restore_backup(&backup, "source password", path)
            .unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, source_wallet.id);
        assert_eq!(
            std::fs::read_to_string(format!("{path}.pre-restore")).unwrap(),
            before
        );
        service.lock();
        let mut reloaded = WalletService::load_store(path).unwrap();
        assert!(reloaded.unlock_vault("local password").is_err());
        reloaded.unlock_vault("source password").unwrap();
    }

    #[test]
    fn backup_operations_require_an_unlocked_vault() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        let backup = service.export_backup().unwrap();
        service.lock();
        assert!(service.export_backup().is_err());
        assert!(
            service
                .restore_backup(&backup, "vault password", path)
                .is_err()
        );
    }

    #[test]
    fn unlock_delay_grows_and_is_capped() {
        use super::unlock_delay;
        assert_eq!(unlock_delay(2), None);
        assert_eq!(unlock_delay(3), Some(Duration::from_secs(5)));
        assert_eq!(unlock_delay(4), Some(Duration::from_secs(10)));
        assert_eq!(unlock_delay(100), Some(Duration::from_secs(300)));
        assert_eq!(unlock_delay(u32::MAX), Some(Duration::from_secs(300)));
    }

    #[test]
    fn shared_vault_password_protects_all_wallets_and_reloads_locked() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        let created = service.create_and_save_wallet("Primary", path).unwrap();
        let second = service.create_and_save_wallet("Savings", path).unwrap();
        let phrase = service.reveal_mnemonic(&created.id).unwrap();
        let serialized = std::fs::read_to_string(path).unwrap();
        assert!(!serialized.contains(phrase.as_str()));
        assert!(!serialized.contains("Primary"));
        assert!(!serialized.contains("Savings"));
        service.lock();
        assert!(service.list_wallets().is_err());
        assert_eq!(service.vault_status(), "locked");

        let mut reloaded = WalletService::load_store(path).unwrap();
        assert!(reloaded.unlock_vault("wrong password").is_err());
        assert!(reloaded.list_wallets().is_err());
        reloaded.unlock_vault("vault password").unwrap();
        let reloaded_wallets = reloaded.list_wallets().unwrap();
        assert_eq!(reloaded_wallets.len(), 2);
        assert_eq!(reloaded_wallets[0].id, created.id);
        assert_eq!(reloaded_wallets[1].id, second.id);
        assert_eq!(
            reloaded.reveal_mnemonic(&created.id).unwrap().as_str(),
            phrase.as_str()
        );
    }

    #[test]
    fn legacy_migration_requires_every_old_password_and_preserves_original_on_failure() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let expected_key = legacy_store(path, "old wallet password");
        let original_file = std::fs::read_to_string(path).unwrap();
        let mut service = WalletService::load_store(path).unwrap();
        assert_eq!(service.vault_status(), "legacy");

        assert!(
            service
                .migrate_legacy_vault(
                    "new vault password",
                    &[credential("legacy-wallet", "wrong password")],
                    300,
                    path,
                )
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original_file);
        assert_eq!(service.vault_status(), "legacy");

        let invalid_path = format!("{path}.missing/vault.json");
        assert!(
            service
                .migrate_legacy_vault(
                    "new vault password",
                    &[credential("legacy-wallet", "old wallet password")],
                    300,
                    &invalid_path,
                )
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original_file);
        assert_eq!(service.vault_status(), "legacy");

        let migrated = service
            .migrate_legacy_vault(
                "new vault password",
                &[credential("legacy-wallet", "old wallet password")],
                300,
                path,
            )
            .unwrap();
        assert_eq!(migrated.len(), 1);
        assert_eq!(service.vault_status(), "unlocked");
        let wallet =
            Wallet::from_mnemonic(service.reveal_mnemonic("legacy-wallet").unwrap().as_str())
                .unwrap();
        assert_eq!(wallet.public_key, expected_key);
        assert!(
            !std::fs::read_to_string(path)
                .unwrap()
                .contains("abandon abandon")
        );

        let mut loaded = WalletService::load_store(path).unwrap();
        assert_eq!(loaded.vault_status(), "locked");
        loaded.unlock_vault("new vault password").unwrap();
        assert_eq!(loaded.list_wallets().unwrap()[0].name, "Legacy");
    }

    #[test]
    fn wallet_mutations_are_rollback_safe_on_save_failure() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.create_vault("vault password", path).unwrap();
        let created = service.create_and_save_wallet("Keep", path).unwrap();
        let invalid_path = format!("{path}.missing/vault.json");

        assert!(
            service
                .delete_wallet_and_save(&created.id, &invalid_path)
                .is_err()
        );
        assert_eq!(service.list_wallets().unwrap()[0].id, created.id);
        let persisted = WalletService::load_store(path).unwrap();
        assert_eq!(persisted.vault_status(), "locked");
    }

    #[test]
    fn auto_lock_range_is_validated_and_idle_vault_locks() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        assert!(service.set_auto_lock_seconds(29, path).is_err());
        assert!(service.set_auto_lock_seconds(1801, path).is_err());
        service.set_auto_lock_seconds(30, path).unwrap();
        service.create_vault("vault password", path).unwrap();
        service.lock();
        assert!(service.set_auto_lock_seconds(60, path).is_err());
        service.unlock_vault("vault password").unwrap();
        assert_eq!(service.auto_lock_seconds(), 30);
        service.expire_for_test();

        assert_eq!(
            service.list_wallets().unwrap_err(),
            "vault locked after inactivity"
        );
        assert_eq!(service.vault_status(), "locked");
    }

    #[test]
    fn recorded_activity_resets_the_inactivity_deadline() {
        let (_directory, path) = temp_path();
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();
        service.set_auto_lock_seconds(30, path).unwrap();
        service.create_vault("vault password", path).unwrap();
        service.last_activity = Some(Instant::now() - Duration::from_secs(10));

        service.record_activity().unwrap();

        assert_eq!(service.vault_status(), "unlocked");
        assert!(service.last_activity.unwrap().elapsed() < Duration::from_secs(1));
    }
}

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Write, path::Path};
use tempfile::NamedTempFile;

use crate::vault::VaultFile;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WalletRecord {
    pub id: String,
    pub name: String,
    pub vault: VaultFile,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct WalletStore {
    pub wallets: Vec<WalletRecord>,
}

impl WalletStore {
    pub fn new() -> Self {
        Self {
            wallets: Vec::new(),
        }
    }

    pub fn add_wallet(&mut self, wallet: WalletRecord) -> Result<(), String> {
        if self.find_by_id(&wallet.id).is_some() {
            return Err(format!("wallet ID already exists: {}", wallet.id));
        }
        self.wallets.push(wallet);
        Ok(())
    }

    pub fn remove_wallet(&mut self, id: &str) -> Option<WalletRecord> {
        self.wallets
            .iter()
            .position(|wallet| wallet.id == id)
            .map(|index| self.wallets.remove(index))
    }

    pub fn list_wallets(&self) -> &[WalletRecord] {
        &self.wallets
    }

    pub fn find_by_id(&self, id: &str) -> Option<&WalletRecord> {
        self.wallets.iter().find(|w| w.id == id)
    }

    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("serialize wallet store failed: {e}"))?;

        let path = Path::new(path);
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temp_file = NamedTempFile::new_in(parent)
            .map_err(|e| format!("create temporary wallet store failed: {e}"))?;
        temp_file
            .write_all(json.as_bytes())
            .map_err(|e| format!("write temporary wallet store failed: {e}"))?;
        temp_file
            .as_file()
            .sync_all()
            .map_err(|e| format!("sync temporary wallet store failed: {e}"))?;
        temp_file
            .persist(path)
            .map_err(|e| format!("replace wallet store failed: {}", e.error))?;

        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let data =
            std::fs::read_to_string(path).map_err(|e| format!("read wallet store failed: {e}"))?;

        let store: Self = serde_json::from_str(&data)
            .map_err(|e| format!("deserialize wallet store failed: {e}"))?;

        let mut wallet_ids = HashSet::with_capacity(store.wallets.len());
        for wallet in &store.wallets {
            if wallet.id.trim().is_empty() || !wallet_ids.insert(&wallet.id) {
                return Err("wallet store contains an empty or duplicate wallet ID".to_string());
            }
        }

        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::{WalletRecord, WalletStore};
    use crate::vault::VaultFile;

    fn test_record(id: &str) -> WalletRecord {
        WalletRecord {
            id: id.to_string(),
            name: "Test wallet".to_string(),
            vault: VaultFile::encrypt(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                "test password",
            )
            .unwrap(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn store_round_trip_preserves_wallet_records() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallets.json");
        let mut store = WalletStore::new();
        store.add_wallet(test_record("wallet-test")).unwrap();

        store.save_to_file(path.to_str().unwrap()).unwrap();
        store.add_wallet(test_record("wallet-test-2")).unwrap();
        store.save_to_file(path.to_str().unwrap()).unwrap();
        let loaded = WalletStore::load_from_file(path.to_str().unwrap()).unwrap();

        assert_eq!(loaded.wallets.len(), 2);
        assert_eq!(loaded.wallets[0].id, "wallet-test");
        assert_eq!(loaded.wallets[0].name, "Test wallet");
    }

    #[test]
    fn store_rejects_duplicate_ids() {
        let mut store = WalletStore::new();
        store.add_wallet(test_record("wallet-test")).unwrap();

        assert!(store.add_wallet(test_record("wallet-test")).is_err());
        assert_eq!(store.wallets.len(), 1);
    }
}

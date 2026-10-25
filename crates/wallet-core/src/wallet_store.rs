use serde::{Deserialize, Serialize};

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
        Self { wallets: Vec::new() }
    }

    pub fn add_wallet(&mut self, wallet: WalletRecord) {
        self.wallets.push(wallet);
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

        std::fs::write(path, json)
            .map_err(|e| format!("write wallet store failed: {e}"))?;

        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let data = std::fs::read_to_string(path)
            .map_err(|e| format!("read wallet store failed: {e}"))?;

        let store: Self = serde_json::from_str(&data)
            .map_err(|e| format!("deserialize wallet store failed: {e}"))?;

        Ok(store)
    }
}
use crate::vault::VaultFile;
use crate::wallet::Wallet;
use crate::wallet_store::{WalletRecord, WalletStore};
use rand::random;

pub struct WalletService {
    pub store: WalletStore,
}

impl WalletService {
    pub fn new() -> Self {
        Self {
            store: WalletStore::new(),
        }
    }

    pub fn create_wallet(&mut self, name: &str, password: &str) -> Result<WalletRecord, String> {
        let wallet = Wallet::generate()?;

        let vault = VaultFile::encrypt(wallet.mnemonic(), password)
            .map_err(|e| format!("encrypt wallet failed: {e}"))?;

        let id = loop {
            let candidate = format!("wallet-{}", hex::encode(random::<[u8; 16]>()));
            if self.store.find_by_id(&candidate).is_none() {
                break candidate;
            }
        };

        let record = WalletRecord {
            id,
            name: name.to_string(),
            vault,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        self.store.add_wallet(record.clone())?;

        Ok(record)
    }

    pub fn load_store(path: &str) -> Result<Self, String> {
        let store = WalletStore::load_from_file(path)?;
        Ok(Self { store })
    }

    pub fn save_store(&self, path: &str) -> Result<(), String> {
        self.store.save_to_file(path)
    }

    pub fn list_wallets(&self) -> &[WalletRecord] {
        self.store.list_wallets()
    }

    pub fn unlock_wallet(&self, wallet_id: &str, password: &str) -> Result<Wallet, String> {
        let record = self
            .store
            .find_by_id(wallet_id)
            .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;

        let mnemonic = record
            .vault
            .decrypt(password)
            .map_err(|e| format!("decrypt wallet failed: {e}"))?;

        Wallet::from_mnemonic(mnemonic.as_str())
    }
}

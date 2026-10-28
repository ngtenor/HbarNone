use crate::vault::VaultFile;
use crate::wallet::Wallet;
use crate::wallet_store::{WalletRecord, WalletStore};
use rand::random;
use zeroize::Zeroizing;

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
        self.store_wallet(name, password, &wallet)
    }

    pub fn import_wallet(
        &mut self,
        name: &str,
        mnemonic: &str,
        password: &str,
    ) -> Result<WalletRecord, String> {
        if mnemonic.is_empty() {
            return Err("recovery phrase cannot be empty".to_string());
        }
        if mnemonic.len() > 1024 {
            return Err("recovery phrase cannot exceed 1024 bytes".to_string());
        }
        let wallet = Wallet::from_mnemonic(mnemonic)?;
        self.store_wallet(name, password, &wallet)
    }

    fn store_wallet(
        &mut self,
        name: &str,
        password: &str,
        wallet: &Wallet,
    ) -> Result<WalletRecord, String> {
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

    pub fn create_and_save_wallet(
        &mut self,
        name: &str,
        password: &str,
        path: &str,
    ) -> Result<WalletRecord, String> {
        let record = self.create_wallet(name, password)?;
        self.save_new_wallet(record, path)
    }

    pub fn import_and_save_wallet(
        &mut self,
        name: &str,
        mnemonic: &str,
        password: &str,
        path: &str,
    ) -> Result<WalletRecord, String> {
        let record = self.import_wallet(name, mnemonic, password)?;
        self.save_new_wallet(record, path)
    }

    fn save_new_wallet(
        &mut self,
        record: WalletRecord,
        path: &str,
    ) -> Result<WalletRecord, String> {
        if let Err(error) = self.save_store(path) {
            self.store.remove_wallet(&record.id);
            return Err(error);
        }
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

    pub fn reveal_mnemonic(
        &self,
        wallet_id: &str,
        password: &str,
    ) -> Result<Zeroizing<String>, String> {
        let record = self
            .store
            .find_by_id(wallet_id)
            .ok_or_else(|| format!("wallet not found: {wallet_id}"))?;

        record
            .vault
            .decrypt(password)
            .map_err(|e| format!("decrypt wallet failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use crate::wallet::Wallet;

    use super::WalletService;

    #[test]
    fn create_save_reload_and_unlock_preserves_wallet() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallets.json");
        let path = path.to_str().unwrap();
        let mut service = WalletService::new();

        let created = service
            .create_and_save_wallet("Primary", "a sufficiently long test password", path)
            .unwrap();
        let loaded = WalletService::load_store(path).unwrap();
        let unlocked = loaded
            .unlock_wallet(&created.id, "a sufficiently long test password")
            .unwrap();
        let original = service
            .unlock_wallet(&created.id, "a sufficiently long test password")
            .unwrap();

        assert_eq!(loaded.list_wallets().len(), 1);
        assert_eq!(unlocked.public_key, original.public_key);
    }

    #[test]
    fn failed_save_rolls_back_created_wallet() {
        let directory = tempfile::tempdir().unwrap();
        let invalid_path = directory.path().join("missing").join("wallets.json");
        let mut service = WalletService::new();

        let result = service.create_and_save_wallet(
            "Primary",
            "a sufficiently long test password",
            invalid_path.to_str().unwrap(),
        );

        assert!(result.is_err());
        assert!(service.list_wallets().is_empty());
    }

    #[test]
    fn unlock_rejects_wrong_password() {
        let mut service = WalletService::new();
        let created = service
            .create_wallet("Primary", "a sufficiently long test password")
            .unwrap();

        assert!(
            service
                .unlock_wallet(&created.id, "incorrect password")
                .is_err()
        );
    }

    #[test]
    fn reveal_mnemonic_requires_correct_password_and_matches_wallet() {
        let mut service = WalletService::new();
        let password = "a sufficiently long test password";
        let created = service.create_wallet("Primary", password).unwrap();

        let mnemonic = service.reveal_mnemonic(&created.id, password).unwrap();
        let revealed_wallet = Wallet::from_mnemonic(mnemonic.as_str()).unwrap();
        let unlocked_wallet = service.unlock_wallet(&created.id, password).unwrap();

        assert_eq!(revealed_wallet.public_key, unlocked_wallet.public_key);
        assert!(
            service
                .reveal_mnemonic(&created.id, "incorrect password")
                .is_err()
        );
    }

    #[test]
    fn import_wallet_validates_phrase_and_saves_encrypted_record() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallets.json");
        let path = path.to_str().unwrap();
        let phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let password = "a sufficiently long test password";
        let mut service = WalletService::new();

        assert!(
            service
                .import_and_save_wallet("Imported", "not a valid phrase", password, path)
                .is_err()
        );
        assert!(
            service
                .import_and_save_wallet("Imported", &"x".repeat(1025), password, path)
                .is_err()
        );
        assert!(service.list_wallets().is_empty());

        let imported = service
            .import_and_save_wallet("Imported", phrase, password, path)
            .unwrap();
        let loaded = WalletService::load_store(path).unwrap();
        let revealed = loaded.reveal_mnemonic(&imported.id, password).unwrap();

        assert_eq!(loaded.list_wallets().len(), 1);
        assert_eq!(loaded.list_wallets()[0].name, "Imported");
        assert_eq!(revealed.as_str(), phrase);
    }
}

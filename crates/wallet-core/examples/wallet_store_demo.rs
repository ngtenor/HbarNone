use wallet_core::vault::VaultFile;
use wallet_core::wallet::Wallet;
use wallet_core::wallet_store::{WalletRecord, WalletStore};

fn main() {
    let password = "correct horse battery staple";

    let wallet = Wallet::generate().expect("failed to generate wallet");

    let vault =
        VaultFile::encrypt(wallet.mnemonic(), password).expect("failed to encrypt mnemonic");

    let record = WalletRecord {
        id: "wallet-1".to_string(),
        name: "Primary Wallet".to_string(),
        vault,
        created_at: "2026-10-25T00:00:00Z".to_string(),
        updated_at: "2026-10-25T00:00:00Z".to_string(),
    };

    let mut store = WalletStore::new();
    store.add_wallet(record).expect("failed to add wallet");

    let path = "wallet_store.json";
    store
        .save_to_file(path)
        .expect("failed to save wallet store");

    let loaded_store = WalletStore::load_from_file(path).expect("failed to load wallet store");
    let wallet_record = loaded_store
        .find_by_id("wallet-1")
        .expect("wallet not found");

    let mnemonic = wallet_record
        .vault
        .decrypt(password)
        .expect("failed to decrypt mnemonic");
    let restored = Wallet::from_mnemonic(&mnemonic).expect("invalid recovered mnemonic");

    println!(
        "public key match: {}",
        wallet.public_key == restored.public_key
    );
}

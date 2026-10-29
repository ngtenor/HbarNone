use wallet_core::{wallet::Wallet, wallet_service::WalletService};

#[test]
fn created_wallet_is_listed_and_unlocks_after_service_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wallets.json");
    let path = path.to_str().unwrap();
    let password = "a sufficiently long vault password";

    let mut service = WalletService::new();
    service.create_vault(password, path).unwrap();
    let created = service.create_and_save_wallet("Disposable", path).unwrap();
    assert_eq!(service.list_wallets().unwrap().len(), 1);
    assert_eq!(service.list_wallets().unwrap()[0].id, created.id);
    let expected_public_key =
        Wallet::from_mnemonic(service.reveal_mnemonic(&created.id).unwrap().as_str())
            .unwrap()
            .public_key;

    let serialized_store = std::fs::read_to_string(path).unwrap();
    let mnemonic = service.reveal_mnemonic(&created.id).unwrap();
    assert!(!serialized_store.contains(mnemonic.as_str()));
    drop(service);

    let mut restarted_service = WalletService::load_store(path).unwrap();
    assert_eq!(restarted_service.vault_status(), "locked");
    assert!(restarted_service.list_wallets().is_err());
    restarted_service.unlock_vault(password).unwrap();
    assert_eq!(restarted_service.list_wallets().unwrap().len(), 1);
    assert_eq!(restarted_service.list_wallets().unwrap()[0].id, created.id);
    assert_eq!(
        restarted_service.list_wallets().unwrap()[0].name,
        "Disposable"
    );

    let unlocked = Wallet::from_mnemonic(
        restarted_service
            .reveal_mnemonic(&created.id)
            .unwrap()
            .as_str(),
    )
    .unwrap();
    assert_eq!(unlocked.public_key, expected_public_key);
}

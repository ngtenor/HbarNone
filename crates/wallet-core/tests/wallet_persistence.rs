use wallet_core::wallet_service::WalletService;

#[test]
fn created_wallet_is_listed_and_unlocks_after_service_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wallets.json");
    let path = path.to_str().unwrap();
    let password = "a sufficiently long test password";

    let mut service = WalletService::new();
    let created = service
        .create_and_save_wallet("Disposable", password, path)
        .unwrap();
    assert_eq!(service.list_wallets().len(), 1);
    assert_eq!(service.list_wallets()[0].id, created.id);
    let expected_public_key = service
        .unlock_wallet(&created.id, password)
        .unwrap()
        .public_key;

    let serialized_store = std::fs::read_to_string(path).unwrap();
    let mnemonic = service.reveal_mnemonic(&created.id, password).unwrap();
    assert!(!serialized_store.contains(mnemonic.as_str()));
    drop(service);

    let restarted_service = WalletService::load_store(path).unwrap();
    assert_eq!(restarted_service.list_wallets().len(), 1);
    assert_eq!(restarted_service.list_wallets()[0].id, created.id);
    assert_eq!(restarted_service.list_wallets()[0].name, "Disposable");

    let unlocked = restarted_service
        .unlock_wallet(&created.id, password)
        .unwrap();
    assert_eq!(unlocked.public_key, expected_public_key);
}

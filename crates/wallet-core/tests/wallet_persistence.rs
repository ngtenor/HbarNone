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

fn restart(path: &str) -> WalletService {
    WalletService::load_store(path).unwrap()
}

#[test]
fn every_change_survives_successive_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wallets.json");
    let path = path.to_str().unwrap();
    let password = "a sufficiently long vault password";
    let new_password = "a different long vault password";
    let imported_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    // Session 1: create the vault, a generated wallet and an imported wallet.
    let mut service = WalletService::new();
    service.create_vault(password, path).unwrap();
    service.set_auto_lock_seconds(90, path).unwrap();
    let generated = service.create_and_save_wallet("Generated", path).unwrap();
    let imported = service
        .import_and_save_wallet("Imported", imported_phrase, path)
        .unwrap();
    let generated_key =
        Wallet::from_mnemonic(service.reveal_mnemonic(&generated.id).unwrap().as_str())
            .unwrap()
            .public_key;
    drop(service);

    // Session 2: auto-lock persisted; rename and delete persist.
    let mut service = restart(path);
    assert_eq!(service.vault_status(), "locked");
    assert!(service.unlock_vault("wrong password").is_err());
    let wallets = service.unlock_vault(password).unwrap();
    assert_eq!(wallets.len(), 2);
    assert_eq!(service.auto_lock_seconds(), 90);
    service
        .rename_wallet_and_save(&generated.id, "Renamed", path)
        .unwrap();
    service.delete_wallet_and_save(&imported.id, path).unwrap();
    drop(service);

    // Session 3: the rename and deletion persisted; the deleted phrase can be imported again.
    let mut service = restart(path);
    let wallets = service.unlock_vault(password).unwrap();
    assert_eq!(wallets.len(), 1);
    assert_eq!(wallets[0].id, generated.id);
    assert_eq!(wallets[0].name, "Renamed");
    service
        .import_and_save_wallet("Imported again", imported_phrase, path)
        .unwrap();
    service
        .change_vault_password(password, new_password, path)
        .unwrap();
    drop(service);

    // Session 4: only the new password works, and the keys are unchanged.
    let mut service = restart(path);
    assert!(service.unlock_vault(password).is_err());
    let wallets = service.unlock_vault(new_password).unwrap();
    assert_eq!(wallets.len(), 2);
    assert_eq!(service.auto_lock_seconds(), 90);
    let key_after = Wallet::from_mnemonic(service.reveal_mnemonic(&generated.id).unwrap().as_str())
        .unwrap()
        .public_key;
    assert_eq!(key_after, generated_key);
    let backup = service.export_backup().unwrap();
    drop(service);

    // Session 5: restore the backup into a fresh store, then restart again.
    let other_directory = tempfile::tempdir().unwrap();
    let other_path = other_directory.path().join("wallets.json");
    let other_path = other_path.to_str().unwrap();
    let mut fresh = WalletService::load_store(other_path).unwrap();
    assert_eq!(fresh.vault_status(), "uninitialized");
    fresh
        .restore_backup(&backup, new_password, other_path)
        .unwrap();
    drop(fresh);

    let mut restored = restart(other_path);
    assert_eq!(restored.vault_status(), "locked");
    let wallets = restored.unlock_vault(new_password).unwrap();
    assert_eq!(wallets.len(), 2);
    assert!(wallets.iter().any(|wallet| wallet.name == "Renamed"));
    assert!(wallets.iter().any(|wallet| wallet.name == "Imported again"));
    assert_eq!(restored.auto_lock_seconds(), 90);
}

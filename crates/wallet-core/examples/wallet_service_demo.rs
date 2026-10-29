use wallet_core::wallet_service::WalletService;

fn main() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.json");
    let path = path.to_str().unwrap();
    let mut service = WalletService::new();
    service
        .create_vault("correct horse battery staple", path)
        .unwrap();

    let record = service
        .create_and_save_wallet("Primary Wallet", path)
        .unwrap();

    let mnemonic = service.reveal_mnemonic(&record.id).unwrap();
    let wallet = wallet_core::wallet::Wallet::from_mnemonic(mnemonic.as_str()).unwrap();

    println!("{}", wallet.public_key);
}

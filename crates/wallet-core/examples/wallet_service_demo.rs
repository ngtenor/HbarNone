use wallet_core::wallet_service::WalletService;

fn main() {
    let mut service = WalletService::new();

    let record = service
        .create_wallet("Primary Wallet", "correct horse battery staple")
        .unwrap();

    let wallet = service
        .unlock_wallet(&record.id, "correct horse battery staple")
        .unwrap();

    println!("{}", wallet.public_key);
}
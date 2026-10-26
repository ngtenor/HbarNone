use wallet_core::wallet::Wallet;

fn main() {
    let wallet = Wallet::generate().expect("failed to generate wallet");

    wallet.display();
}

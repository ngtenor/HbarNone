use wallet_core::wallet::Wallet;

fn main() {
    let wallet = Wallet::generate();

    wallet.display();
}
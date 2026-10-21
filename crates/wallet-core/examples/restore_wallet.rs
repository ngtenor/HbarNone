use wallet_core::wallet::Wallet;

fn main() {
    let phrase =
        "pupil bamboo mammal rug club bid maple december essence capable admit human rule total myself bargain receive what advice erode hundred require theme truth";

    let wallet =
        Wallet::from_mnemonic(phrase);

    wallet.display();
}

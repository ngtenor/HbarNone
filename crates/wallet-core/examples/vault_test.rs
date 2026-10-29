use wallet_core::vault::VaultFile;
use wallet_core::wallet::Wallet;

fn main() {
    let password = "correct horse battery staple";

    println!("Creating wallet...\n");

    let original_wallet = Wallet::generate().expect("failed to generate wallet");

    println!("Original Public Key:");
    println!("{}", original_wallet.public_key);

    let vault =
        VaultFile::encrypt(original_wallet.mnemonic(), password).expect("Failed to encrypt wallet");

    let json = serde_json::to_string_pretty(&vault).expect("Failed to serialize vault");

    println!("\nVault JSON:");
    println!("{}", json);

    let loaded_vault: VaultFile = serde_json::from_str(&json).expect("Failed to deserialize vault");

    let mnemonic = loaded_vault
        .decrypt(password)
        .expect("Failed to decrypt wallet");

    let restored_wallet =
        Wallet::from_mnemonic(mnemonic.as_str()).expect("invalid recovered mnemonic");

    println!("\nRestored Public Key:");
    println!("{}", restored_wallet.public_key);

    println!("\nVerification:");

    if original_wallet.public_key == restored_wallet.public_key {
        println!("SUCCESS: Public keys match");
    } else {
        println!("ERROR: Public keys differ");
    }
}

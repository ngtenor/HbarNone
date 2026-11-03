//! Command-line offline signer (test build).
//!
//! Shows every field of a HederaVault signing request, asks for the key, and
//! signs only after the user types `yes`. It never connects to a network.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use signer_core::{
    KeyKind, SigningKey, TransferSummary, check_time, format_hbar, parse_request, sign,
};
use wallet_core::key_import::parse_private_key;
use wallet_core::wallet::Wallet;
use zeroize::Zeroizing;

fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn ask(label: &str) -> Result<String, String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line.trim().to_string())
}

/// Hidden input on a terminal; plain input when stdin is piped.
fn ask_secret(label: &str) -> Result<Zeroizing<String>, String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    match rpassword::read_password() {
        Ok(text) => Ok(Zeroizing::new(text.trim().to_string())),
        Err(_) => {
            let mut line = Zeroizing::new(String::new());
            io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            Ok(Zeroizing::new(line.trim().to_string()))
        }
    }
}

fn show(summary: &TransferSummary) {
    println!();
    println!("  ====== PLEASE CHECK EVERY LINE ======");
    println!("  Action        Send HBAR");
    println!("  Network       Hedera Testnet (no real value)");
    println!("  From          {}", summary.payer);
    println!("  To            {}", summary.recipient);
    println!(
        "  Amount        {} HBAR ({} tinybars)",
        format_hbar(summary.tinybars),
        summary.tinybars
    );
    println!("  Memo          {:?}", summary.memo);
    println!(
        "  Fee limit     {} HBAR (the real fee is far lower)",
        format_hbar(summary.fee_limit_tinybars as i64)
    );
    println!("  Node          {}", summary.node);
    println!(
        "  Time left     {} seconds",
        summary.expires_at_seconds - now_seconds()
    );
    println!("  =====================================");
    println!();
}

fn read_key() -> Result<SigningKey, String> {
    println!("Key source:  1) recovery phrase   2) private key (hex)");
    let choice = ask("Choose 1 or 2: ")?;
    match choice.as_str() {
        "1" => {
            let phrase = ask_secret("Recovery phrase (hidden): ")?;
            let wallet = Wallet::from_mnemonic(&phrase)?;
            SigningKey::new(KeyKind::Ed25519, *wallet.private_key_bytes())
        }
        "2" => {
            let kind = match ask("Key type (ed25519 or ecdsa; Enter for ed25519): ")?
                .to_lowercase()
                .as_str()
            {
                "" | "ed25519" => KeyKind::Ed25519,
                "ecdsa" => KeyKind::Ecdsa,
                _ => return Err("Key type must be ed25519 or ecdsa.".to_string()),
            };
            let text = ask_secret("Private key hex (hidden): ")?;
            let parsed = parse_private_key(&text)?;
            SigningKey::new(kind, *parsed.secret)
        }
        _ => Err("Choose 1 or 2.".to_string()),
    }
}

fn run() -> Result<(), String> {
    println!("HederaVault offline signer (test build). Testnet only. Use disposable keys.");
    println!("Nothing here connects to a network.\n");
    let request = ask("Paste the signing request (HVTX1-REQ:...): ")?;
    let (_, summary) = parse_request(&request).map_err(|e| e.to_string())?;
    check_time(&summary, now_seconds()).map_err(|e| e.to_string())?;
    show(&summary);

    let key = read_key()?;
    println!("\nSigning key (public): {}", key.public_key_hex()?);
    println!(
        "Check this is the key of the paying account, {}.",
        summary.payer
    );
    if ask("Type yes to sign, anything else to cancel: ")? != "yes" {
        return Err("Cancelled. Nothing was signed.".to_string());
    }
    let reply = sign(&request, &key, now_seconds())?;
    println!("\nPaste this reply into the app:\n\n{reply}\n");
    Ok(())
}

fn main() -> ExitCode {
    let result = run();
    if let Err(error) = &result {
        println!("\n{error}");
    }
    // Keep the window open when started by double-click.
    let _ = ask("Press Enter to close ");
    if result.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

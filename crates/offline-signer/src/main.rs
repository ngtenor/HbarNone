//! Command-line offline signer (test build).
//!
//! Shows every field of a HederaVault signing request, asks for the key, and
//! signs only after the user types `yes`. It never connects to a network.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

mod scan;

use signer_core::{
    Action, KeyKind, SigningKey, Summary, check_time, format_hbar, parse_request, sign,
    with_checksum,
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

fn show(summary: &Summary) {
    println!();
    println!("  ====== PLEASE CHECK EVERY LINE ======");
    match &summary.action {
        Action::Transfer {
            recipient,
            tinybars,
        } => {
            println!("  Action        Send HBAR");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  From          {}", with_checksum(&summary.payer));
            println!("  To            {}", with_checksum(recipient));
            println!(
                "  Amount        {} HBAR ({tinybars} tinybars)",
                format_hbar(*tinybars)
            );
            println!("  Memo          {:?}", summary.memo);
        }
        Action::DeleteAccount { account, recipient } => {
            println!("  Action        DELETE AN ACCOUNT (permanent)");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  Delete        {}", with_checksum(account));
            println!("  Remaining HBAR go to  {}", with_checksum(recipient));
            println!("  Fee paid by   {}", with_checksum(&summary.payer));
        }
        Action::CreateAccount {
            key_kind,
            key_hex,
            initial_tinybars,
        } => {
            println!("  Action        CREATE AN ACCOUNT");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  Funded and paid by  {}", with_checksum(&summary.payer));
            println!(
                "  Opening balance  {} HBAR ({initial_tinybars} tinybars)",
                format_hbar(*initial_tinybars)
            );
            println!(
                "  New account key  {} {key_hex}",
                match key_kind {
                    KeyKind::Ed25519 => "(Ed25519)",
                    KeyKind::Ecdsa => "(ECDSA)",
                }
            );
            println!("  Account memo  {:?}", summary.memo);
            println!("  Whoever holds that key controls the new account and its HBAR.");
        }
        Action::TokenTransfer {
            token,
            recipient,
            amount,
            decimals,
        } => {
            println!("  Action        SEND A TOKEN");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  From          {}", with_checksum(&summary.payer));
            println!("  To            {}", with_checksum(recipient));
            println!("  Token ID      {}", with_checksum(token));
            println!("  Raw amount    {amount} (smallest unit of the token)");
            println!(
                "  Decimals      {decimals} (the app's claim; the network rejects a mismatch)"
            );
            println!(
                "  The token's name cannot be checked here. Compare the token ID with the app."
            );
            println!("  Memo          {:?}", summary.memo);
        }
        Action::TokenAssociate { token } => {
            println!("  Action        ASSOCIATE A TOKEN (lets the account hold it)");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  Account       {}", with_checksum(&summary.payer));
            println!("  Token ID      {}", with_checksum(token));
        }
        Action::TokenDissociate { token } => {
            println!("  Action        DISASSOCIATE A TOKEN (the account stops holding it)");
            println!("  Network       Hedera Testnet (no real value)");
            println!("  Account       {}", with_checksum(&summary.payer));
            println!("  Token ID      {}", with_checksum(token));
        }
    }
    println!(
        "  Fee limit     {} HBAR (the real fee is far lower)",
        format_hbar(summary.fee_limit_tinybars as i64)
    );
    println!("  Node          {}", summary.node);
    println!(
        "  Time left     {} seconds",
        summary.expires_at_seconds - now_seconds()
    );
    println!("  (Account checksums are the letters after the dash. Compare them with the app.)");
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

/// The request as text, or read from a screenshot or photo of its QR code.
fn read_request() -> Result<String, String> {
    let input = ask("Paste the signing request (HVTX1-REQ:...),\n\
         or the path of a screenshot or photo of its QR code (PNG or JPEG): ")?;
    if input.starts_with(signer_core::REQUEST_PREFIX) || input.is_empty() {
        return Ok(input);
    }
    let path = scan::clean_path(&input);
    let text = scan::read_qr_text(Path::new(path))?;
    println!("Read the request from the picture.");
    Ok(text.trim().to_string())
}

fn run() -> Result<(), String> {
    println!("HederaVault offline signer (test build). Testnet only. Use disposable keys.");
    println!("Nothing here connects to a network.\n");
    let request = read_request()?;
    let (_, summary) = parse_request(&request).map_err(|e| e.to_string())?;
    check_time(&summary, now_seconds()).map_err(|e| e.to_string())?;
    show(&summary);

    let key = read_key()?;
    let key_hex = key.public_key_hex()?;
    println!("\nSigning key (public): {key_hex}");
    println!(
        "Check this is the key of the paying account, {}.",
        with_checksum(&summary.payer)
    );
    match &summary.action {
        Action::CreateAccount { key_hex: new, .. } if *new == key_hex => {
            println!("The new account will use this same key.");
        }
        Action::CreateAccount { .. } => {
            println!(
                "WARNING: the new account's key is DIFFERENT from the key you are signing with."
            );
        }
        Action::DeleteAccount { .. } => {
            println!("A deletion needs this key to control both accounts shown above.");
        }
        Action::Transfer { .. }
        | Action::TokenTransfer { .. }
        | Action::TokenAssociate { .. }
        | Action::TokenDissociate { .. } => {}
    }
    if ask("Type yes to sign, anything else to cancel: ")? != "yes" {
        return Err("Cancelled. Nothing was signed.".to_string());
    }
    let reply = sign(&request, &key, now_seconds())?;
    println!("\nPaste this reply into the app, or scan the QR code below:\n\n{reply}\n");
    print_qr(&reply);
    Ok(())
}

/// Draws the reply as a QR code with half-block characters, light on dark so it scans in a dark terminal.
fn print_qr(text: &str) {
    use qrcode::render::unicode::Dense1x2;
    use qrcode::{EcLevel, QrCode};
    match QrCode::with_error_correction_level(text.as_bytes(), EcLevel::L) {
        Ok(code) => {
            let image = code
                .render::<Dense1x2>()
                .dark_color(Dense1x2::Light)
                .light_color(Dense1x2::Dark)
                .quiet_zone(true)
                .build();
            println!("{image}\n");
        }
        Err(_) => println!("(The reply is too long to show as a QR code. Use the text above.)\n"),
    }
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

//! Test signer standing in for an offline device. Use a DISPOSABLE Testnet key only.
//!
//! Command line: sign_request <HVTX1-REQ:...> <ed25519|ecdsa> <private-key-hex>
//! Double-clicked (no arguments): asks for each value, then waits before closing.

use std::io::{self, Write};

use hedera_tx::offline::{decode_request, sign_request};
use wallet_core::key_import::{KeyType, parse_private_key};

fn ask(label: &str) -> Result<String, String> {
    print!("{label}: ");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line.trim().to_string())
}

fn sign(request: &str, key_type: &str, key: &str) -> Result<String, String> {
    let key_type = match key_type.to_lowercase().as_str() {
        "" | "ed25519" => KeyType::Ed25519,
        "ecdsa" => KeyType::Ecdsa,
        _ => return Err("key type must be ed25519 or ecdsa".into()),
    };
    let (_, d) = decode_request(request)?;
    println!(
        "Signing: send {} tinybars from {} to {} (memo: {:?})",
        d.tinybars, d.sender, d.receiver, d.memo
    );
    let parsed = parse_private_key(key)?;
    sign_request(request, key_type, &parsed.secret)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let interactive = args.is_empty();
    let result = if let [request, key_type, key] = args.as_slice() {
        sign(request, key_type, key)
    } else if interactive {
        (|| {
            let request = ask("Paste the request (HVTX1-REQ:...)")?;
            let key_type = ask("Key type (ed25519 or ecdsa; Enter for ed25519)")?;
            let key = ask("Private key (hex)")?;
            sign(&request, &key_type, &key)
        })()
    } else {
        Err("usage: sign_request <request> <ed25519|ecdsa> <private-key-hex>".into())
    };
    match result {
        Ok(signature) => println!("\nPaste this into the app:\n\n{signature}"),
        Err(error) => println!("\nError: {error}"),
    }
    if interactive {
        let _ = ask("\nPress Enter to close");
    }
}

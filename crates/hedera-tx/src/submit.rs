//! Submits signed transactions to a Hedera Testnet node over gRPC.
//!
//! The node address is a compile-time constant and the transaction is built
//! for the Testnet node account, so the wallet cannot be pointed at Mainnet or
//! at an arbitrary host.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tonic::codegen::http::uri::PathAndQuery;
use tonic::transport::Endpoint;
use wallet_core::key_import::KeyType;
use wallet_core::wallet_service::validate_account_id;

use crate::{
    ChangeKeyParams, CreateAccountParams, CreateTokenParams, DeleteAccountParams,
    DeleteTokenParams, MAX_TOKEN_DECIMALS, TokenAssociationParams, TokenSupplyParams,
    TokenTransferParams, Transaction, TransactionResponse, TransferParams, change_key_body_bytes,
    create_account_body_bytes, create_token_body_bytes, delete_account_body_bytes,
    delete_token_body_bytes, sign_body, sign_body_multi, token_association_body_bytes,
    token_supply_body_bytes, token_transfer_body_bytes, transfer_body_bytes,
};

/// Testnet node account 0.0.3 and its plain-gRPC endpoint.
pub const TESTNET_NODE_ACCOUNT: (i64, i64, i64) = (0, 0, 3);
pub const TESTNET_NODE_URL: &str = "http://34.94.106.61:50211";
/// Upper bound on the fee the payer agrees to (1 HBAR). The real fee is far lower.
pub const MAX_FEE_TINYBARS: u64 = 100_000_000;
/// Creating a token costs about one US dollar, so its limit is higher (20 HBAR).
pub const MAX_TOKEN_CREATE_FEE_TINYBARS: u64 = 2_000_000_000;
pub const MAX_MEMO_BYTES: usize = 100;
const TINYBARS_PER_HBAR: u64 = 100_000_000;
pub(crate) const TRANSFER_PATH: &str = "/proto.CryptoService/cryptoTransfer";
pub(crate) const CREATE_PATH: &str = "/proto.CryptoService/createAccount";
const UPDATE_PATH: &str = "/proto.CryptoService/updateAccount";
pub(crate) const DELETE_PATH: &str = "/proto.CryptoService/cryptoDelete";
const TOKEN_CREATE_PATH: &str = "/proto.TokenService/createToken";
const TOKEN_DELETE_PATH: &str = "/proto.TokenService/deleteToken";
const TOKEN_MINT_PATH: &str = "/proto.TokenService/mintToken";
const TOKEN_BURN_PATH: &str = "/proto.TokenService/burnToken";
const TOKEN_ASSOCIATE_PATH: &str = "/proto.TokenService/associateTokens";
const TOKEN_DISSOCIATE_PATH: &str = "/proto.TokenService/dissociateTokens";
/// Largest opening balance the app will fund a new account with.
pub(crate) const MAX_INITIAL_TINYBARS: i64 = 10_000 * TINYBARS_PER_HBAR as i64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    /// Transaction ID in the mirror node's `0.0.x-seconds-nanos` form.
    pub transaction_id: String,
}

/// Parses an HBAR amount such as `1`, `0.5` or `12.34567890` into tinybars.
pub fn parse_hbar(input: &str) -> Result<i64, String> {
    let text = input.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits_ok = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty())
        || !digits_ok(whole)
        || !digits_ok(fraction)
        || fraction.len() > 8
    {
        return Err("Enter an amount like 1.5 (at most 8 decimal places).".to_string());
    }
    let whole: u64 = if whole.is_empty() {
        0
    } else {
        whole
            .parse()
            .map_err(|_| "Amount is too large.".to_string())?
    };
    let fraction: u64 = format!("{fraction:0<8}").parse().expect("digits only");
    let tinybars = whole
        .checked_mul(TINYBARS_PER_HBAR)
        .and_then(|v| v.checked_add(fraction))
        .and_then(|v| i64::try_from(v).ok())
        .ok_or_else(|| "Amount is too large.".to_string())?;
    if tinybars == 0 {
        return Err("Amount must be greater than zero.".to_string());
    }
    Ok(tinybars)
}

pub(crate) fn parse_account(id: &str) -> Result<(i64, i64, i64), String> {
    let normal = validate_account_id(id)?;
    let num = normal
        .rsplit('.')
        .next()
        .and_then(|n| n.parse::<i64>().ok())
        .ok_or_else(|| "Invalid account ID.".to_string())?;
    Ok((0, 0, num))
}

/// Human-readable meaning of a node precheck code.
pub fn precheck_message(code: i32) -> String {
    match code {
        0 => "OK".to_string(),
        1 => "INVALID_TRANSACTION".to_string(),
        2 => "PAYER_ACCOUNT_NOT_FOUND".to_string(),
        3 => "INVALID_NODE_ACCOUNT".to_string(),
        4 => "TRANSACTION_EXPIRED".to_string(),
        5 => "INVALID_TRANSACTION_START".to_string(),
        6 => "INVALID_TRANSACTION_DURATION".to_string(),
        7 => "INVALID_SIGNATURE".to_string(),
        8 => "MEMO_TOO_LONG".to_string(),
        9 => "INSUFFICIENT_TX_FEE".to_string(),
        10 => "INSUFFICIENT_PAYER_BALANCE".to_string(),
        11 => "DUPLICATE_TRANSACTION".to_string(),
        12 => "BUSY".to_string(),
        13 => "NOT_SUPPORTED".to_string(),
        other => format!("status code {other}"),
    }
}

/// Builds, signs and submits an HBAR transfer to Testnet.
pub async fn send_testnet_transfer(
    key_type: KeyType,
    secret: &[u8; 32],
    sender: &str,
    receiver: &str,
    tinybars: i64,
    memo: &str,
) -> Result<SendReceipt, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    let sender_id = parse_account(sender)?;
    let receiver_id = parse_account(receiver)?;
    // Start slightly in the past so a small clock skew is not rejected.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?;
    let start = now.saturating_sub(Duration::from_secs(10));
    let params = TransferParams {
        sender: sender_id,
        receiver: receiver_id,
        tinybars,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: start.as_secs() as i64,
        valid_start_nanos: start.subsec_nanos() as i32,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: memo.to_string(),
    };
    let body = transfer_body_bytes(&params)?;
    let encoded = sign_body(&body, key_type, secret)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, TRANSFER_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the transfer: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!(
            "0.0.{}-{}-{:09}",
            sender_id.2, params.valid_start_seconds, params.valid_start_nanos
        ),
    })
}

/// Creates a new Testnet account controlled by `new_key`, funded with
/// `initial_tinybars` from the payer, who signs with `payer_key`. The new
/// account ID is not in the node's reply; it shows up on the mirror node once
/// consensus is reached.
pub async fn create_testnet_account(
    new_key: (KeyType, &[u8; 32]),
    payer: &str,
    payer_key: (KeyType, &[u8; 32]),
    initial_tinybars: i64,
) -> Result<SendReceipt, String> {
    if !(0..=MAX_INITIAL_TINYBARS).contains(&initial_tinybars) {
        return Err("The opening balance is out of range.".to_string());
    }
    let payer_id = parse_account(payer)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?;
    let start = now.saturating_sub(Duration::from_secs(10));
    let params = CreateAccountParams {
        payer: payer_id,
        key_type: new_key.0,
        public_key: wallet_core::key_import::public_key_bytes(new_key.0, new_key.1)?,
        initial_tinybars: initial_tinybars as u64,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: start.as_secs() as i64,
        valid_start_nanos: start.subsec_nanos() as i32,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: String::new(),
    };
    let body = create_account_body_bytes(&params)?;
    let encoded = sign_body(&body, payer_key.0, payer_key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, CREATE_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the account creation: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!(
            "0.0.{}-{}-{:09}",
            payer_id.2, params.valid_start_seconds, params.valid_start_nanos
        ),
    })
}

/// Replaces the key of a Testnet account with the new key. The account pays
/// the fee, and both the current key and the new key sign, as the network requires.
pub async fn change_testnet_account_key(
    account: &str,
    current: (KeyType, &[u8; 32]),
    new: (KeyType, &[u8; 32]),
) -> Result<SendReceipt, String> {
    let account_id = parse_account(account)?;
    let new_public_key = wallet_core::key_import::public_key_bytes(new.0, new.1)?;
    if new.0 == current.0
        && new_public_key == wallet_core::key_import::public_key_bytes(current.0, current.1)?
    {
        return Err("The account already uses that key.".to_string());
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?;
    let start = now.saturating_sub(Duration::from_secs(10));
    let params = ChangeKeyParams {
        account: account_id,
        new_key_type: new.0,
        new_public_key,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: start.as_secs() as i64,
        valid_start_nanos: start.subsec_nanos() as i32,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    let body = change_key_body_bytes(&params)?;
    let encoded = sign_body_multi(&body, &[current, new])?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, UPDATE_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the key change: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!(
            "0.0.{}-{}-{:09}",
            account_id.2, params.valid_start_seconds, params.valid_start_nanos
        ),
    })
}

/// Deletes a Testnet account and sends its remaining HBAR to `recipient`.
/// `signers` must hold the keys of the deleted account and of the recipient
/// (they may be the same key, which signs once). When `recipient_key_held` is
/// true the recipient pays the fee, so an emptied account can still be deleted.
pub async fn delete_testnet_account(
    account: &str,
    recipient: &str,
    recipient_key_held: bool,
    signers: &[(KeyType, &[u8; 32])],
) -> Result<SendReceipt, String> {
    let account_id = parse_account(account)?;
    let recipient_id = parse_account(recipient)?;
    let payer = if recipient_key_held {
        recipient_id
    } else {
        account_id
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?;
    let start = now.saturating_sub(Duration::from_secs(10));
    let params = DeleteAccountParams {
        delete_account: account_id,
        transfer_account: recipient_id,
        payer,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: start.as_secs() as i64,
        valid_start_nanos: start.subsec_nanos() as i32,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    let body = delete_account_body_bytes(&params)?;
    // One signature per distinct key.
    let mut unique: Vec<(KeyType, &[u8; 32])> = Vec::new();
    for signer in signers {
        let public = wallet_core::key_import::public_key_bytes(signer.0, signer.1)?;
        let seen = unique.iter().any(|(kind, secret)| {
            *kind == signer.0
                && wallet_core::key_import::public_key_bytes(*kind, secret)
                    .ok()
                    .as_ref()
                    == Some(&public)
        });
        if !seen {
            unique.push(*signer);
        }
    }
    let encoded = sign_body_multi(&body, &unique)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, DELETE_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the account deletion: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!(
            "0.0.{}-{}-{:09}",
            payer.2, params.valid_start_seconds, params.valid_start_nanos
        ),
    })
}

/// Parses a token amount such as `1000` or `12.34` into the token's smallest
/// unit, given its decimals. Zero is allowed (a token can start with no supply).
pub fn parse_token_amount(input: &str, decimals: u32) -> Result<u64, String> {
    let text = input.trim();
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits_ok = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty())
        || !digits_ok(whole)
        || !digits_ok(fraction)
        || fraction.len() > decimals as usize
    {
        return Err(format!(
            "Enter an amount with at most {decimals} decimal places."
        ));
    }
    let too_large = || "The amount is too large.".to_string();
    let whole: u128 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| too_large())?
    };
    let fraction_units: u128 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = decimals as usize)
            .parse()
            .map_err(|_| too_large())?
    };
    let scale = 10u128.checked_pow(decimals).ok_or_else(too_large)?;
    let units = whole
        .checked_mul(scale)
        .and_then(|v| v.checked_add(fraction_units))
        .filter(|v| *v <= i64::MAX as u128)
        .ok_or_else(too_large)?;
    Ok(units as u64)
}

fn start_time() -> Result<(i64, i32), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?;
    // Start slightly in the past so a small clock skew is not rejected.
    let start = now.saturating_sub(Duration::from_secs(10));
    Ok((start.as_secs() as i64, start.subsec_nanos() as i32))
}

/// Creates a fungible token whose treasury is `payer`, and whose admin and
/// supply keys are the payer's key. The token ID shows up on the mirror node
/// once consensus is reached.
pub async fn create_testnet_token(
    payer: &str,
    key: (KeyType, &[u8; 32]),
    name: &str,
    symbol: &str,
    decimals: u32,
    initial_supply: u64,
) -> Result<SendReceipt, String> {
    if decimals > MAX_TOKEN_DECIMALS {
        return Err("Decimals can be 18 at most.".to_string());
    }
    let payer_id = parse_account(payer)?;
    let (seconds, nanos) = start_time()?;
    let params = CreateTokenParams {
        payer: payer_id,
        name: name.trim().to_string(),
        symbol: symbol.trim().to_string(),
        decimals,
        initial_supply,
        key_type: key.0,
        public_key: wallet_core::key_import::public_key_bytes(key.0, key.1)?,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_TOKEN_CREATE_FEE_TINYBARS,
    };
    let body = create_token_body_bytes(&params)?;
    let encoded = sign_body(&body, key.0, key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, TOKEN_CREATE_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the token creation: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!("0.0.{}-{}-{:09}", payer_id.2, seconds, nanos),
    })
}

/// Associates (or dissociates) the payer's own account with a token. The
/// account's key signs, and the account also pays the fee.
pub async fn change_testnet_token_association(
    account_id: &str,
    key: (KeyType, &[u8; 32]),
    token: &str,
    associate: bool,
) -> Result<SendReceipt, String> {
    let payer_id = parse_account(account_id)?;
    let token_id = parse_account(token)?;
    let (seconds, nanos) = start_time()?;
    let params = TokenAssociationParams {
        payer: payer_id,
        token: token_id,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    let body = token_association_body_bytes(&params, associate)?;
    let encoded = sign_body(&body, key.0, key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let path = if associate {
        TOKEN_ASSOCIATE_PATH
    } else {
        TOKEN_DISSOCIATE_PATH
    };
    let response = submit(transaction, path).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the request: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!("0.0.{}-{}-{:09}", payer_id.2, seconds, nanos),
    })
}

/// Sends `amount` (in the token's smallest unit) of a fungible token from the
/// sender's own account. The sender's key signs and pays the fee. `decimals`
/// is pinned in the transaction so a wrong scale is rejected by the network.
pub async fn send_testnet_token(
    key: (KeyType, &[u8; 32]),
    sender: &str,
    receiver: &str,
    token: &str,
    amount: u64,
    decimals: u32,
    memo: &str,
) -> Result<SendReceipt, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    let amount = i64::try_from(amount).map_err(|_| "The amount is too large.".to_string())?;
    let sender_id = parse_account(sender)?;
    let (seconds, nanos) = start_time()?;
    let params = TokenTransferParams {
        sender: sender_id,
        receiver: parse_account(receiver)?,
        token: parse_account(token)?,
        amount,
        decimals,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: memo.to_string(),
    };
    let body = token_transfer_body_bytes(&params)?;
    let encoded = sign_body(&body, key.0, key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, TRANSFER_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the token transfer: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!("0.0.{}-{}-{:09}", sender_id.2, seconds, nanos),
    })
}

/// Deletes a token. `payer` pays the fee; the key must be the token's admin key.
pub async fn delete_testnet_token(
    payer: &str,
    key: (KeyType, &[u8; 32]),
    token: &str,
) -> Result<SendReceipt, String> {
    let payer_id = parse_account(payer)?;
    let token_id = parse_account(token)?;
    let (seconds, nanos) = start_time()?;
    let params = DeleteTokenParams {
        payer: payer_id,
        token: token_id,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    let body = delete_token_body_bytes(&params)?;
    let encoded = sign_body(&body, key.0, key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let response = submit(transaction, TOKEN_DELETE_PATH).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the token deletion: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!("0.0.{}-{}-{:09}", payer_id.2, seconds, nanos),
    })
}

/// Mints (`mint`) or burns `amount` (in the token's smallest unit) of a token.
/// `payer` pays the fee; the key must be the token's supply key.
pub async fn change_testnet_token_supply(
    payer: &str,
    key: (KeyType, &[u8; 32]),
    token: &str,
    amount: u64,
    mint: bool,
) -> Result<SendReceipt, String> {
    let payer_id = parse_account(payer)?;
    let (seconds, nanos) = start_time()?;
    let params = TokenSupplyParams {
        payer: payer_id,
        token: parse_account(token)?,
        amount,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    let body = token_supply_body_bytes(&params, mint)?;
    let encoded = sign_body(&body, key.0, key.1)?;
    let transaction =
        <Transaction as prost::Message>::decode(encoded.as_slice()).map_err(|e| e.to_string())?;
    let (path, action) = if mint {
        (TOKEN_MINT_PATH, "mint")
    } else {
        (TOKEN_BURN_PATH, "burn")
    };
    let response = submit(transaction, path).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the token {action}: {}",
            precheck_message(response.precheck_code)
        ));
    }
    Ok(SendReceipt {
        transaction_id: format!("0.0.{}-{}-{:09}", payer_id.2, seconds, nanos),
    })
}

pub(crate) async fn submit(
    transaction: Transaction,
    path: &'static str,
) -> Result<TransactionResponse, String> {
    let channel = Endpoint::from_static(TESTNET_NODE_URL)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .connect()
        .await
        .map_err(|e| format!("Could not reach the Testnet node: {e}"))?;
    let mut client = tonic::client::Grpc::new(channel);
    client
        .ready()
        .await
        .map_err(|e| format!("Testnet node unavailable: {e}"))?;
    let request = tonic::Request::new(transaction);
    let response = client
        .unary::<_, TransactionResponse, _>(
            request,
            PathAndQuery::from_static(path),
            tonic::codec::ProstCodec::default(),
        )
        .await
        .map_err(|e| format!("Testnet node error: {}", e.message()))?;
    Ok(response.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_amounts_in_smallest_units() {
        assert_eq!(parse_token_amount("1000", 0).unwrap(), 1000);
        assert_eq!(parse_token_amount("12.34", 2).unwrap(), 1234);
        assert_eq!(parse_token_amount("12.3", 2).unwrap(), 1230);
        assert_eq!(parse_token_amount(".5", 1).unwrap(), 5);
        assert_eq!(parse_token_amount("0", 2).unwrap(), 0);
        assert!(parse_token_amount("1.234", 2).is_err());
        assert!(parse_token_amount("1.5", 0).is_err());
        assert!(parse_token_amount("", 2).is_err());
        assert!(parse_token_amount("-1", 2).is_err());
        assert!(parse_token_amount("9223372036854775808", 0).is_err());
        assert_eq!(
            parse_token_amount("9223372036854775807", 0).unwrap(),
            i64::MAX as u64
        );
        assert!(parse_token_amount("99999999999999999999999999999999999999999", 18).is_err());
    }

    #[test]
    fn parses_hbar_amounts() {
        assert_eq!(parse_hbar("1").unwrap(), 100_000_000);
        assert_eq!(parse_hbar(" 0.5 ").unwrap(), 50_000_000);
        assert_eq!(parse_hbar(".25").unwrap(), 25_000_000);
        assert_eq!(parse_hbar("12.34567890").unwrap(), 1_234_567_890);
        assert_eq!(parse_hbar("0.00000001").unwrap(), 1);
    }

    #[test]
    fn rejects_bad_hbar_amounts() {
        for bad in [
            "",
            ".",
            "0",
            "0.0",
            "-1",
            "1e3",
            "1,5",
            "abc",
            "1.123456789",
            "99999999999999999999",
        ] {
            assert!(parse_hbar(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn accounts_parse_to_numbers() {
        assert_eq!(parse_account("0.0.1234").unwrap(), (0, 0, 1234));
        assert!(parse_account("1.0.5").is_err());
    }

    #[test]
    fn precheck_codes_are_named() {
        assert_eq!(precheck_message(7), "INVALID_SIGNATURE");
        assert_eq!(precheck_message(999), "status code 999");
    }
}

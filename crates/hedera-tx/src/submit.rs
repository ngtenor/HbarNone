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

use crate::{Transaction, TransactionResponse, TransferParams, sign_body, transfer_body_bytes};

/// Testnet node account 0.0.3 and its plain-gRPC endpoint.
pub const TESTNET_NODE_ACCOUNT: (i64, i64, i64) = (0, 0, 3);
pub const TESTNET_NODE_URL: &str = "http://34.94.106.61:50211";
/// Upper bound on the fee the payer agrees to (1 HBAR). The real fee is far lower.
pub const MAX_FEE_TINYBARS: u64 = 100_000_000;
const MAX_MEMO_BYTES: usize = 100;
const TINYBARS_PER_HBAR: u64 = 100_000_000;
const TRANSFER_PATH: &str = "/proto.CryptoService/cryptoTransfer";

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

fn parse_account(id: &str) -> Result<(i64, i64, i64), String> {
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
    let response = submit(transaction).await?;
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

async fn submit(transaction: Transaction) -> Result<TransactionResponse, String> {
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
            PathAndQuery::from_static(TRANSFER_PATH),
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

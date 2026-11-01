//! Read-only client for Hedera's public Testnet mirror node.
//!
//! The base URL is a compile-time constant, redirects are disabled and every
//! response is size-limited, so a wallet can never be pointed at another host.
//! Nothing here handles keys or recovery phrases; only the public key and
//! account IDs are sent.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wallet_core::wallet_service::validate_account_id;

pub const TESTNET_BASE_URL: &str = "https://testnet.mirrornode.hedera.com";
pub const TRANSACTION_LIMIT: usize = 10;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TEXT_CHARS: usize = 64;
const MAX_ACCOUNT_MATCHES: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransactionSummary {
    pub transaction_id: String,
    pub kind: String,
    pub result: String,
    pub consensus_time: String,
    /// Net change of this account's balance in tinybars (negative when it paid).
    pub net_tinybars: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountOverview {
    pub account_id: String,
    pub balance_tinybars: u64,
    pub transactions: Vec<TransactionSummary>,
}

#[derive(Deserialize)]
struct AccountsResponse {
    accounts: Vec<AccountEntry>,
}

#[derive(Deserialize)]
struct AccountEntry {
    account: String,
    balance: Option<BalanceEntry>,
    #[serde(default)]
    deleted: Option<bool>,
}

#[derive(Deserialize)]
struct BalanceEntry {
    balance: u64,
}

#[derive(Deserialize)]
struct TransactionsResponse {
    transactions: Vec<TransactionEntry>,
}

#[derive(Deserialize)]
struct TransactionEntry {
    transaction_id: String,
    name: Option<String>,
    result: Option<String>,
    consensus_timestamp: Option<String>,
    #[serde(default)]
    transfers: Vec<TransferEntry>,
}

#[derive(Deserialize)]
struct TransferEntry {
    account: String,
    amount: i64,
}

fn clean_text(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT_CHARS)
        .collect()
}

/// Converts a mirror timestamp such as `1700000000.123456789` to RFC 3339.
fn format_consensus_time(timestamp: &str) -> String {
    let seconds = timestamp.split('.').next().unwrap_or_default();
    seconds
        .parse::<i64>()
        .ok()
        .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
        .map(|time| time.to_rfc3339())
        .unwrap_or_default()
}

fn validate_public_key_hex(public_key: &str) -> Result<String, String> {
    let lowered = public_key.trim().to_ascii_lowercase();
    if matches!(lowered.len(), 64 | 66) && lowered.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(lowered)
    } else {
        Err("public key must be 64 (Ed25519) or 66 (ECDSA) hexadecimal characters".to_string())
    }
}

pub fn parse_account_matches(body: &[u8]) -> Result<Vec<String>, String> {
    let response: AccountsResponse = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    response
        .accounts
        .iter()
        .filter(|entry| entry.deleted != Some(true))
        .take(MAX_ACCOUNT_MATCHES)
        .map(|entry| validate_account_id(&entry.account))
        .collect()
}

/// Whether the mirror node reports this account as deleted.
pub fn parse_deleted(body: &[u8], expected_account: &str) -> Result<bool, String> {
    let response: AccountEntry = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    if validate_account_id(&response.account)? != expected_account {
        return Err("the mirror node returned a different account".to_string());
    }
    Ok(response.deleted == Some(true))
}

pub fn parse_balance(body: &[u8], expected_account: &str) -> Result<u64, String> {
    let response: AccountEntry = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    if validate_account_id(&response.account)? != expected_account {
        return Err("the mirror node returned a different account".to_string());
    }
    Ok(response.balance.map(|entry| entry.balance).unwrap_or(0))
}

pub fn parse_transactions(
    body: &[u8],
    account_id: &str,
) -> Result<Vec<TransactionSummary>, String> {
    let response: TransactionsResponse = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    Ok(response
        .transactions
        .iter()
        .take(TRANSACTION_LIMIT)
        .map(|entry| TransactionSummary {
            transaction_id: clean_text(&entry.transaction_id),
            kind: clean_text(entry.name.as_deref().unwrap_or("UNKNOWN")),
            result: clean_text(entry.result.as_deref().unwrap_or("UNKNOWN")),
            consensus_time: format_consensus_time(
                entry.consensus_timestamp.as_deref().unwrap_or_default(),
            ),
            net_tinybars: entry
                .transfers
                .iter()
                .filter(|transfer| transfer.account == account_id)
                .fold(0i64, |total, transfer| {
                    total.saturating_add(transfer.amount)
                }),
        })
        .collect())
}

pub struct MirrorClient {
    http: reqwest::Client,
    base_url: &'static str,
}

impl MirrorClient {
    pub fn testnet() -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .https_only(true)
            .user_agent("HederaVault")
            .build()
            .map_err(|_| "could not start the network client".to_string())?;
        Ok(Self {
            http,
            base_url: TESTNET_BASE_URL,
        })
    }

    /// Returns `None` when the mirror node answers 404.
    async fn get(&self, path_and_query: &str) -> Result<Option<Vec<u8>>, String> {
        let url = format!("{}{}", self.base_url, path_and_query);
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| "could not reach the Hedera Testnet mirror node".to_string())?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("the mirror node answered with status {status}"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "the connection to the mirror node was interrupted".to_string())?
        {
            if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err("the mirror node response was too large".to_string());
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Some(body))
    }

    /// Accounts on Testnet whose key is this public key (hex): 64 characters
    /// for Ed25519, 66 for compressed ECDSA.
    pub async fn find_accounts_by_public_key(
        &self,
        public_key_hex: &str,
    ) -> Result<Vec<String>, String> {
        let key = validate_public_key_hex(public_key_hex)?;
        let path = format!("/api/v1/accounts?account.publickey={key}&limit={MAX_ACCOUNT_MATCHES}");
        match self.get(&path).await? {
            Some(body) => parse_account_matches(&body),
            None => Ok(Vec::new()),
        }
    }

    /// Whether the account is marked deleted on Testnet (`false` if unknown).
    pub async fn account_is_deleted(&self, account_id: &str) -> Result<bool, String> {
        let account_id = validate_account_id(account_id)?;
        match self.get(&format!("/api/v1/accounts/{account_id}")).await? {
            Some(body) => parse_deleted(&body, &account_id),
            None => Ok(false),
        }
    }

    /// Balance only (one request), or `None` if the account is unknown on Testnet.
    pub async fn account_balance(&self, account_id: &str) -> Result<Option<u64>, String> {
        let account_id = validate_account_id(account_id)?;
        match self.get(&format!("/api/v1/accounts/{account_id}")).await? {
            Some(body) => parse_balance(&body, &account_id).map(Some),
            None => Ok(None),
        }
    }

    /// Balance and recent transactions, or `None` if the account is unknown on Testnet.
    pub async fn account_overview(
        &self,
        account_id: &str,
    ) -> Result<Option<AccountOverview>, String> {
        let account_id = validate_account_id(account_id)?;
        let Some(account_body) = self.get(&format!("/api/v1/accounts/{account_id}")).await? else {
            return Ok(None);
        };
        let balance_tinybars = parse_balance(&account_body, &account_id)?;
        let transactions_path = format!(
            "/api/v1/transactions?account.id={account_id}&limit={TRANSACTION_LIMIT}&order=desc"
        );
        let transactions = match self.get(&transactions_path).await? {
            Some(body) => parse_transactions(&body, &account_id)?,
            None => Vec::new(),
        };
        Ok(Some(AccountOverview {
            account_id,
            balance_tinybars,
            transactions,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_deleted_flag_and_checks_the_account() {
        assert!(parse_deleted(br#"{"account":"0.0.5","deleted":true}"#, "0.0.5").unwrap());
        assert!(!parse_deleted(br#"{"account":"0.0.5","deleted":false}"#, "0.0.5").unwrap());
        assert!(!parse_deleted(br#"{"account":"0.0.5"}"#, "0.0.5").unwrap());
        assert!(parse_deleted(br#"{"account":"0.0.6","deleted":true}"#, "0.0.5").is_err());
    }

    #[test]
    fn parses_account_matches_and_rejects_bad_ids() {
        let body = br#"{"accounts":[{"account":"0.0.1001"},{"account":"0.0.00042"}],"links":{}}"#;
        assert_eq!(
            parse_account_matches(body).unwrap(),
            vec!["0.0.1001", "0.0.42"]
        );
        assert!(
            parse_account_matches(br#"{"accounts":[]}"#)
                .unwrap()
                .is_empty()
        );
        assert!(parse_account_matches(br#"{"accounts":[{"account":"1.2.3"}]}"#).is_err());
        assert!(parse_account_matches(b"not json").is_err());
        assert!(parse_account_matches(br#"{"nope":1}"#).is_err());
    }

    #[test]
    fn parses_balance_and_checks_the_account() {
        let body = br#"{"account":"0.0.1001","balance":{"balance":250000000,"tokens":[]}}"#;
        assert_eq!(parse_balance(body, "0.0.1001").unwrap(), 250_000_000);
        assert!(parse_balance(body, "0.0.1002").is_err());
        assert_eq!(
            parse_balance(br#"{"account":"0.0.1001"}"#, "0.0.1001").unwrap(),
            0
        );
        assert!(
            parse_balance(
                br#"{"account":"0.0.1001","balance":{"balance":-5}}"#,
                "0.0.1001"
            )
            .is_err()
        );
    }

    #[test]
    fn parses_transactions_with_net_amount_for_the_account() {
        let body = br#"{"transactions":[
            {"transaction_id":"0.0.1001-1700000000-123","name":"CRYPTOTRANSFER","result":"SUCCESS",
             "consensus_timestamp":"1700000000.123456789",
             "transfers":[{"account":"0.0.1001","amount":-100100000},
                          {"account":"0.0.2002","amount":100000000},
                          {"account":"0.0.3","amount":100000}]},
            {"transaction_id":"0.0.2002-1700000100-9","name":"CRYPTOTRANSFER","result":"SUCCESS",
             "consensus_timestamp":"1700000100.000000001",
             "transfers":[{"account":"0.0.1001","amount":500},{"account":"0.0.1001","amount":250}]},
            {"transaction_id":"x\u0007y","consensus_timestamp":"bad"}
        ]}"#;
        let list = parse_transactions(body, "0.0.1001").unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].net_tinybars, -100_100_000);
        assert_eq!(list[0].kind, "CRYPTOTRANSFER");
        assert_eq!(list[0].consensus_time, "2023-11-14T22:13:20+00:00");
        assert_eq!(list[1].net_tinybars, 750);
        assert_eq!(list[2].transaction_id, "xy");
        assert_eq!(list[2].kind, "UNKNOWN");
        assert_eq!(list[2].consensus_time, "");
        assert_eq!(list[2].net_tinybars, 0);
    }

    #[test]
    fn transaction_list_is_capped_and_text_is_bounded() {
        let long = "A".repeat(500);
        let entries: Vec<String> = (0..25)
            .map(|i| format!(r#"{{"transaction_id":"{long}","name":"N{i}"}}"#))
            .collect();
        let body = format!(r#"{{"transactions":[{}]}}"#, entries.join(","));
        let list = parse_transactions(body.as_bytes(), "0.0.1").unwrap();
        assert_eq!(list.len(), TRANSACTION_LIMIT);
        assert!(
            list.iter()
                .all(|t| t.transaction_id.chars().count() == MAX_TEXT_CHARS)
        );
    }

    #[test]
    fn public_key_must_be_64_hex_characters() {
        let key = "ab".repeat(32);
        assert_eq!(validate_public_key_hex(&key.to_uppercase()).unwrap(), key);
        assert!(validate_public_key_hex("abc").is_err());
        assert!(validate_public_key_hex(&"zz".repeat(32)).is_err());
        assert!(validate_public_key_hex(&format!("{key}&limit=1")).is_err());
    }

    #[test]
    fn base_url_is_fixed_https_testnet() {
        assert!(TESTNET_BASE_URL.starts_with("https://testnet."));
        let client = MirrorClient::testnet().unwrap();
        assert_eq!(client.base_url, TESTNET_BASE_URL);
    }
}

//! Read-only client for Hedera's public Testnet mirror node.
//!
//! The base URL is a compile-time constant, redirects are disabled and every
//! response is size-limited, so a wallet can never be pointed at another host.
//! Nothing here handles keys or recovery phrases; only the public key and
//! account IDs are sent.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use wallet_core::account_checksum::with_checksum;
use wallet_core::vault_container::Network;
use wallet_core::wallet_service::validate_account_id;

pub const TESTNET_BASE_URL: &str = "https://testnet.mirrornode.hedera.com";
pub const TRANSACTION_LIMIT: usize = 10;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_TEXT_CHARS: usize = 64;
const MAX_ACCOUNT_MATCHES: usize = 25;
const MAX_TOKENS: usize = 50;
const MAX_DECIMALS: u32 = 18;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransactionSummary {
    pub transaction_id: String,
    pub kind: String,
    pub result: String,
    pub consensus_time: String,
    /// Net change of this account's balance in tinybars (negative when it paid).
    pub net_tinybars: i64,
    /// The other account in the transfer, with checksum; empty when there is none.
    pub counterparty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountToken {
    pub token_id: String,
    pub symbol: String,
    pub name: String,
    /// Smallest units, as text because a balance can exceed what JavaScript holds exactly.
    pub balance: String,
    pub decimals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub token_type: String,
    pub total_supply: String,
    pub treasury: String,
    pub deleted: bool,
    pub has_admin_key: bool,
    /// Lowercase hex of the admin key when it is a single ED25519 or ECDSA key.
    pub admin_key: Option<String>,
    pub memo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountOverview {
    pub account_id: String,
    pub balance_tinybars: u64,
    pub transactions: Vec<TransactionSummary>,
    pub tokens: Vec<AccountToken>,
}

/// The mirror node sends some integers as JSON numbers and some as strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum NumberOrText {
    Number(u64),
    Text(String),
    Other(serde::de::IgnoredAny),
}

impl NumberOrText {
    fn text(&self) -> String {
        match self {
            Self::Number(number) => number.to_string(),
            Self::Text(text) => text.clone(),
            Self::Other(_) => String::new(),
        }
    }
}

#[derive(Deserialize)]
struct TokenRelationshipsResponse {
    tokens: Vec<TokenRelationship>,
}

#[derive(Deserialize)]
struct TokenRelationship {
    token_id: String,
    balance: NumberOrText,
}

#[derive(Deserialize)]
struct TokenInfoEntry {
    token_id: String,
    name: Option<String>,
    symbol: Option<String>,
    decimals: Option<NumberOrText>,
    #[serde(rename = "type")]
    token_type: Option<String>,
    total_supply: Option<NumberOrText>,
    treasury_account_id: Option<String>,
    deleted: Option<bool>,
    admin_key: Option<serde_json::Value>,
    memo: Option<String>,
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

/// Token IDs and balances held by the account, in smallest units. Malformed
/// entries are dropped, so a hostile response cannot inject text.
pub fn parse_token_relationships(body: &[u8]) -> Result<Vec<(String, String)>, String> {
    let response: TokenRelationshipsResponse = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    Ok(response
        .tokens
        .iter()
        .take(MAX_TOKENS)
        .filter_map(|entry| {
            let balance = entry.balance.text();
            let valid = !balance.is_empty() && balance.bytes().all(|b| b.is_ascii_digit());
            let id = validate_account_id(&entry.token_id).ok()?;
            valid.then_some((id, balance))
        })
        .collect())
}

fn single_key_hex(key: &serde_json::Value) -> Option<String> {
    let kind = key.get("_type")?.as_str()?;
    let hex = key.get("key")?.as_str()?;
    let well_formed = matches!(kind, "ED25519" | "ECDSA_SECP256K1")
        && !hex.is_empty()
        && hex.len() <= 132
        && hex.bytes().all(|b| b.is_ascii_hexdigit());
    well_formed.then(|| hex.to_ascii_lowercase())
}

pub fn parse_token_info(body: &[u8], expected_token: &str) -> Result<TokenInfo, String> {
    let entry: TokenInfoEntry = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    if validate_account_id(&entry.token_id)? != expected_token {
        return Err("the mirror node returned a different token".to_string());
    }
    let digits = |value: Option<&NumberOrText>| {
        value
            .map(NumberOrText::text)
            .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .unwrap_or_else(|| "0".to_string())
    };
    Ok(TokenInfo {
        token_id: expected_token.to_string(),
        name: clean_text(entry.name.as_deref().unwrap_or_default()),
        symbol: clean_text(entry.symbol.as_deref().unwrap_or_default()),
        decimals: digits(entry.decimals.as_ref())
            .parse::<u32>()
            .unwrap_or(0)
            .min(MAX_DECIMALS),
        token_type: clean_text(entry.token_type.as_deref().unwrap_or("UNKNOWN")),
        total_supply: digits(entry.total_supply.as_ref()),
        treasury: entry
            .treasury_account_id
            .as_deref()
            .and_then(|id| validate_account_id(id).ok())
            .unwrap_or_default(),
        deleted: entry.deleted == Some(true),
        has_admin_key: entry.admin_key.as_ref().is_some_and(|key| !key.is_null()),
        admin_key: entry.admin_key.as_ref().and_then(single_key_hex),
        memo: clean_text(entry.memo.as_deref().unwrap_or_default()),
    })
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
            counterparty: counterparty(&entry.transfers, account_id),
        })
        .collect())
}

/// The other account that received the most (or, for an incoming payment, sent the most),
/// shown with its checksum. Fee and node accounts are small, so the largest mover wins.
fn counterparty(transfers: &[TransferEntry], account_id: &str) -> String {
    let net: i64 = transfers
        .iter()
        .filter(|transfer| transfer.account == account_id)
        .fold(0i64, |total, transfer| {
            total.saturating_add(transfer.amount)
        });
    let others = transfers
        .iter()
        .filter(|transfer| transfer.account != account_id);
    let best = if net > 0 {
        others
            .filter(|transfer| transfer.amount < 0)
            .min_by_key(|transfer| transfer.amount)
    } else {
        others
            .filter(|transfer| transfer.amount > 0)
            .max_by_key(|transfer| transfer.amount)
    };
    best.and_then(|transfer| validate_account_id(&transfer.account).ok())
        .and_then(|id| with_checksum(Network::Testnet, &id).ok())
        .unwrap_or_default()
}

/// Accepts only `shard.realm.num-seconds-nanos`, so the value is safe in a URL path.
pub fn validate_transaction_id(id: &str) -> Result<String, String> {
    let trimmed = id.trim();
    let mut dashes = trimmed.split('-');
    let (Some(account), Some(seconds), Some(nanos), None) =
        (dashes.next(), dashes.next(), dashes.next(), dashes.next())
    else {
        return Err("invalid transaction ID".to_string());
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if validate_account_id(account).is_err() || !digits(seconds) || !digits(nanos) {
        return Err("invalid transaction ID".to_string());
    }
    Ok(trimmed.to_string())
}

/// The consensus result (for example `SUCCESS`) of the first record for this
/// transaction, or `None` if the mirror node has not seen it yet.
pub fn parse_transaction_result(body: &[u8], expected_id: &str) -> Result<Option<String>, String> {
    let response: TransactionsResponse = serde_json::from_slice(body)
        .map_err(|_| "unexpected response from the mirror node".to_string())?;
    Ok(response
        .transactions
        .iter()
        .find(|entry| entry.transaction_id == expected_id)
        .and_then(|entry| entry.result.as_deref())
        .map(clean_text))
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

    /// Consensus result of a submitted transaction; `None` until Testnet has it.
    pub async fn transaction_result(&self, transaction_id: &str) -> Result<Option<String>, String> {
        let id = validate_transaction_id(transaction_id)?;
        match self.get(&format!("/api/v1/transactions/{id}")).await? {
            Some(body) => parse_transaction_result(&body, &id),
            None => Ok(None),
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
        let tokens = self.account_tokens(&account_id).await?;
        Ok(Some(AccountOverview {
            account_id,
            balance_tinybars,
            transactions,
            tokens,
        }))
    }

    /// Fungible tokens the account holds, with their details. A token whose
    /// details cannot be read is still listed, by ID.
    async fn account_tokens(&self, account_id: &str) -> Result<Vec<AccountToken>, String> {
        let path = format!("/api/v1/accounts/{account_id}/tokens?limit={MAX_TOKENS}");
        let held = match self.get(&path).await? {
            Some(body) => parse_token_relationships(&body)?,
            None => Vec::new(),
        };
        let mut tokens = Vec::new();
        for (token_id, balance) in held {
            let info = self.token_info(&token_id).await.ok().flatten();
            match info {
                Some(info) if info.token_type == "NON_FUNGIBLE_UNIQUE" || info.deleted => {}
                Some(info) => tokens.push(AccountToken {
                    token_id,
                    symbol: info.symbol,
                    name: info.name,
                    balance,
                    decimals: info.decimals,
                }),
                None => tokens.push(AccountToken {
                    token_id,
                    symbol: String::new(),
                    name: String::new(),
                    balance,
                    decimals: 0,
                }),
            }
        }
        Ok(tokens)
    }

    /// Details of any token, or `None` if Testnet does not know it.
    pub async fn token_info(&self, token_id: &str) -> Result<Option<TokenInfo>, String> {
        let token_id = validate_account_id(token_id)
            .map_err(|_| "token IDs look like 0.0.12345".to_string())?;
        match self.get(&format!("/api/v1/tokens/{token_id}")).await? {
            Some(body) => parse_token_info(&body, &token_id).map(Some),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_balances_and_drops_malformed_entries() {
        let body = br#"{"tokens":[
            {"token_id":"0.0.8","balance":12345},
            {"token_id":"0.0.9","balance":"99999999999999999999"},
            {"token_id":"bad","balance":1},
            {"token_id":"0.0.10","balance":-5}
        ]}"#;
        assert_eq!(
            parse_token_relationships(body).unwrap(),
            vec![
                ("0.0.8".to_string(), "12345".to_string()),
                ("0.0.9".to_string(), "99999999999999999999".to_string())
            ]
        );
        assert!(parse_token_relationships(b"nope").is_err());
    }

    #[test]
    fn parses_token_info_with_text_numbers_and_checks_the_token() {
        let body = br#"{"token_id":"0.0.8","name":"Alpha\u0007","symbol":"ALP","decimals":"2",
            "type":"FUNGIBLE_COMMON","total_supply":"1000000","treasury_account_id":"0.0.5",
            "deleted":false,"admin_key":{"_type":"ED25519","key":"AB"},"memo":"hi"}"#;
        let info = parse_token_info(body, "0.0.8").unwrap();
        assert_eq!(info.admin_key.as_deref(), Some("ab"));
        assert_eq!(info.name, "Alpha");
        assert_eq!(info.decimals, 2);
        assert_eq!(info.total_supply, "1000000");
        assert_eq!(info.treasury, "0.0.5");
        assert!(info.has_admin_key && !info.deleted);
        assert!(parse_token_info(body, "0.0.9").is_err());
        let bare = parse_token_info(
            br#"{"token_id":"0.0.8","admin_key":null,"decimals":99}"#,
            "0.0.8",
        )
        .unwrap();
        assert!(!bare.has_admin_key && bare.admin_key.is_none());
        let listed = parse_token_info(
            br#"{"token_id":"0.0.8","admin_key":{"_type":"ProtobufEncoded","key":"zz"}}"#,
            "0.0.8",
        )
        .unwrap();
        assert!(listed.has_admin_key && listed.admin_key.is_none());
        assert_eq!(bare.decimals, 18);
    }

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
    fn validates_transaction_ids_and_reads_the_result() {
        assert!(validate_transaction_id("0.0.1001-1700000000-123").is_ok());
        assert!(validate_transaction_id("0.0.1001-1700000000").is_err());
        assert!(validate_transaction_id("0.0.1001-17/../x-1").is_err());
        let body = br#"{"transactions":[{"transaction_id":"0.0.1001-1700000000-123","result":"SUCCESS"}]}"#;
        assert_eq!(
            parse_transaction_result(body, "0.0.1001-1700000000-123").unwrap(),
            Some("SUCCESS".to_string())
        );
        assert_eq!(
            parse_transaction_result(body, "0.0.1001-1700000000-124").unwrap(),
            None
        );
        assert!(parse_transaction_result(b"nope", "0.0.1001-1-1").is_err());
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
        assert_eq!(
            list[0].counterparty,
            with_checksum(Network::Testnet, "0.0.2002").unwrap()
        );
        assert_eq!(list[1].counterparty, "");
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

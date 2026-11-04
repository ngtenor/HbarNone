//! Minimal hand-written Hedera protobuf layer: build and sign an HBAR transfer.
//!
//! Field numbers follow hashgraph/hedera-protobufs (`services/*.proto`).

use ed25519_dalek::Signer;
use k256::ecdsa::SigningKey as EcdsaKey;
use k256::ecdsa::signature::hazmat::PrehashSigner;
use prost::Message;
use sha3::{Digest, Keccak256};
use wallet_core::key_import::{KeyType, public_key_bytes};

pub mod offline;
pub mod submit;

#[derive(Clone, PartialEq, Message)]
pub struct AccountId {
    #[prost(int64, tag = "1")]
    pub shard: i64,
    #[prost(int64, tag = "2")]
    pub realm: i64,
    #[prost(int64, tag = "3")]
    pub num: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct Timestamp {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
    #[prost(int32, tag = "2")]
    pub nanos: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Duration {
    #[prost(int64, tag = "1")]
    pub seconds: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct TransactionId {
    #[prost(message, optional, tag = "1")]
    pub valid_start: Option<Timestamp>,
    #[prost(message, optional, tag = "2")]
    pub account: Option<AccountId>,
}

#[derive(Clone, PartialEq, Message)]
pub struct AccountAmount {
    #[prost(message, optional, tag = "1")]
    pub account: Option<AccountId>,
    #[prost(sint64, tag = "2")]
    pub amount: i64,
}

#[derive(Clone, PartialEq, Message)]
pub struct TransferList {
    #[prost(message, repeated, tag = "1")]
    pub amounts: Vec<AccountAmount>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CryptoTransferBody {
    #[prost(message, optional, tag = "1")]
    pub transfers: Option<TransferList>,
    #[prost(message, repeated, tag = "2")]
    pub token_transfers: Vec<TokenTransferList>,
}

/// Fungible amounts of one token moving between accounts (debits negative).
#[derive(Clone, PartialEq, Message)]
pub struct TokenTransferList {
    #[prost(message, optional, tag = "1")]
    pub token: Option<TokenId>,
    #[prost(message, repeated, tag = "2")]
    pub transfers: Vec<AccountAmount>,
    #[prost(message, optional, tag = "4")]
    pub expected_decimals: Option<UInt32Value>,
}

/// `google.protobuf.UInt32Value`.
#[derive(Clone, PartialEq, Message)]
pub struct UInt32Value {
    #[prost(uint32, tag = "1")]
    pub value: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct TransactionBody {
    #[prost(message, optional, tag = "1")]
    pub transaction_id: Option<TransactionId>,
    #[prost(message, optional, tag = "2")]
    pub node_account: Option<AccountId>,
    #[prost(uint64, tag = "3")]
    pub fee: u64,
    #[prost(message, optional, tag = "4")]
    pub valid_duration: Option<Duration>,
    #[prost(string, tag = "6")]
    pub memo: String,
    #[prost(message, optional, tag = "11")]
    pub crypto_create: Option<CryptoCreateBody>,
    #[prost(message, optional, tag = "12")]
    pub crypto_delete: Option<CryptoDeleteBody>,
    #[prost(message, optional, tag = "14")]
    pub crypto_transfer: Option<CryptoTransferBody>,
    #[prost(message, optional, tag = "15")]
    pub crypto_update: Option<CryptoUpdateBody>,
    #[prost(message, optional, tag = "29")]
    pub token_creation: Option<TokenCreateBody>,
    #[prost(message, optional, tag = "35")]
    pub token_deletion: Option<TokenDeleteBody>,
    #[prost(message, optional, tag = "40")]
    pub token_association: Option<TokenAssociationBody>,
    #[prost(message, optional, tag = "41")]
    pub token_dissociation: Option<TokenAssociationBody>,
}

/// The body of both `TokenAssociate` and `TokenDissociate`.
#[derive(Clone, PartialEq, Message)]
pub struct TokenAssociationBody {
    #[prost(message, optional, tag = "1")]
    pub account: Option<AccountId>,
    #[prost(message, repeated, tag = "2")]
    pub tokens: Vec<TokenId>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TokenId {
    #[prost(int64, tag = "1")]
    pub shard: i64,
    #[prost(int64, tag = "2")]
    pub realm: i64,
    #[prost(int64, tag = "3")]
    pub num: i64,
}

/// A fungible token whose treasury, admin key and supply key are all one key.
#[derive(Clone, PartialEq, Message)]
pub struct TokenCreateBody {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub symbol: String,
    #[prost(uint32, tag = "3")]
    pub decimals: u32,
    #[prost(uint64, tag = "4")]
    pub initial_supply: u64,
    #[prost(message, optional, tag = "5")]
    pub treasury: Option<AccountId>,
    #[prost(message, optional, tag = "6")]
    pub admin_key: Option<Key>,
    #[prost(message, optional, tag = "10")]
    pub supply_key: Option<Key>,
    #[prost(message, optional, tag = "14")]
    pub auto_renew_account: Option<AccountId>,
    #[prost(message, optional, tag = "15")]
    pub auto_renew_period: Option<Duration>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TokenDeleteBody {
    #[prost(message, optional, tag = "1")]
    pub token: Option<TokenId>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CryptoDeleteBody {
    #[prost(message, optional, tag = "1")]
    pub transfer_account: Option<AccountId>,
    #[prost(message, optional, tag = "2")]
    pub delete_account: Option<AccountId>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CryptoUpdateBody {
    #[prost(message, optional, tag = "2")]
    pub account_to_update: Option<AccountId>,
    #[prost(message, optional, tag = "3")]
    pub key: Option<Key>,
}

/// `Key` with only the single-key variants we need (ed25519 = 2, ECDSA secp256k1 = 7).
#[derive(Clone, PartialEq, Message)]
pub struct Key {
    #[prost(bytes = "vec", optional, tag = "2")]
    pub ed25519: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "7")]
    pub ecdsa_secp256k1: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct CryptoCreateBody {
    #[prost(message, optional, tag = "1")]
    pub key: Option<Key>,
    #[prost(uint64, tag = "2")]
    pub initial_balance: u64,
    #[prost(message, optional, tag = "9")]
    pub auto_renew_period: Option<Duration>,
    #[prost(string, tag = "13")]
    pub memo: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct SignaturePair {
    #[prost(bytes = "vec", tag = "1")]
    pub pub_key_prefix: Vec<u8>,
    #[prost(bytes = "vec", optional, tag = "3")]
    pub ed25519: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "6")]
    pub ecdsa_secp256k1: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SignatureMap {
    #[prost(message, repeated, tag = "1")]
    pub pairs: Vec<SignaturePair>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SignedTransaction {
    #[prost(bytes = "vec", tag = "1")]
    pub body_bytes: Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub sig_map: Option<SignatureMap>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Transaction {
    #[prost(bytes = "vec", tag = "5")]
    pub signed_transaction_bytes: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
pub struct TransactionResponse {
    #[prost(int32, tag = "1")]
    pub precheck_code: i32,
    #[prost(uint64, tag = "2")]
    pub cost: u64,
}

/// Parameters of a single-recipient HBAR transfer. Accounts are (shard, realm, num).
pub struct TransferParams {
    pub sender: (i64, i64, i64),
    pub receiver: (i64, i64, i64),
    pub tinybars: i64,
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
    pub memo: String,
}

fn account(id: (i64, i64, i64)) -> AccountId {
    AccountId {
        shard: id.0,
        realm: id.1,
        num: id.2,
    }
}

/// Serialised `TransactionBody` for a transfer of `tinybars` from sender to receiver.
pub fn transfer_body_bytes(p: &TransferParams) -> Result<Vec<u8>, String> {
    transfer_body_bytes_valid_for(p, 120)
}

/// Like [`transfer_body_bytes`], but valid for `valid_seconds` after its start time.
pub fn transfer_body_bytes_valid_for(
    p: &TransferParams,
    valid_seconds: i64,
) -> Result<Vec<u8>, String> {
    if p.tinybars <= 0 {
        return Err("Amount must be greater than zero.".into());
    }
    if p.sender == p.receiver {
        return Err("Sender and recipient must differ.".into());
    }
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.sender)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration {
            seconds: valid_seconds,
        }),
        memo: p.memo.clone(),
        crypto_create: None,
        crypto_delete: None,
        crypto_update: None,
        token_creation: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
        crypto_transfer: Some(CryptoTransferBody {
            transfers: Some(TransferList {
                amounts: vec![
                    AccountAmount {
                        account: Some(account(p.sender)),
                        amount: -p.tinybars,
                    },
                    AccountAmount {
                        account: Some(account(p.receiver)),
                        amount: p.tinybars,
                    },
                ],
            }),
            token_transfers: Vec::new(),
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a fungible token transfer between two accounts.
pub struct TokenTransferParams {
    pub sender: (i64, i64, i64),
    pub receiver: (i64, i64, i64),
    pub token: (i64, i64, i64),
    /// Amount in the token's smallest unit.
    pub amount: i64,
    /// The decimals the sender saw; the network rejects the transfer if they differ.
    pub decimals: u32,
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
    pub memo: String,
}

pub fn token_transfer_body_bytes(p: &TokenTransferParams) -> Result<Vec<u8>, String> {
    if p.amount <= 0 {
        return Err("Amount must be greater than zero.".into());
    }
    if p.sender == p.receiver {
        return Err("Sender and recipient must differ.".into());
    }
    if p.token.0 != 0 || p.token.1 != 0 || p.token.2 <= 0 {
        return Err("Invalid token ID.".into());
    }
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.sender)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration { seconds: 120 }),
        memo: p.memo.clone(),
        crypto_create: None,
        crypto_delete: None,
        crypto_update: None,
        token_creation: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
        crypto_transfer: Some(CryptoTransferBody {
            transfers: None,
            token_transfers: vec![TokenTransferList {
                token: Some(TokenId {
                    shard: p.token.0,
                    realm: p.token.1,
                    num: p.token.2,
                }),
                transfers: vec![
                    AccountAmount {
                        account: Some(account(p.sender)),
                        amount: -p.amount,
                    },
                    AccountAmount {
                        account: Some(account(p.receiver)),
                        amount: p.amount,
                    },
                ],
                expected_decimals: Some(UInt32Value { value: p.decimals }),
            }],
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `CryptoCreate` that funds a new account from the payer.
pub struct CreateAccountParams {
    pub payer: (i64, i64, i64),
    pub key_type: KeyType,
    /// Public key of the new account (32-byte Ed25519 or 33-byte compressed secp256k1).
    pub public_key: Vec<u8>,
    pub initial_tinybars: u64,
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
    pub memo: String,
}

/// Ninety days, a common auto-renew period that the network accepts.
pub(crate) const AUTO_RENEW_SECONDS: i64 = 7_776_000;

/// Serialised `TransactionBody` for creating an account controlled by `public_key`.
pub fn create_account_body_bytes(p: &CreateAccountParams) -> Result<Vec<u8>, String> {
    create_account_body_bytes_valid_for(p, 120)
}

/// Like [`create_account_body_bytes`], but valid for `valid_seconds`.
pub fn create_account_body_bytes_valid_for(
    p: &CreateAccountParams,
    valid_seconds: i64,
) -> Result<Vec<u8>, String> {
    let key = key_message(p.key_type, &p.public_key)?;
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.payer)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration {
            seconds: valid_seconds,
        }),
        memo: String::new(),
        crypto_create: Some(CryptoCreateBody {
            key: Some(key),
            initial_balance: p.initial_tinybars,
            auto_renew_period: Some(Duration {
                seconds: AUTO_RENEW_SECONDS,
            }),
            memo: p.memo.clone(),
        }),
        crypto_transfer: None,
        crypto_delete: None,
        crypto_update: None,
        token_creation: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `CryptoDelete`. The deleted account's remaining HBAR goes to
/// `transfer_account`; the network requires both accounts' keys to sign.
pub struct DeleteAccountParams {
    pub delete_account: (i64, i64, i64),
    pub transfer_account: (i64, i64, i64),
    /// Pays the transaction fee (one of the two accounts).
    pub payer: (i64, i64, i64),
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
}

/// Serialised `TransactionBody` that deletes an account and sweeps its HBAR.
pub fn delete_account_body_bytes(p: &DeleteAccountParams) -> Result<Vec<u8>, String> {
    delete_account_body_bytes_valid_for(p, 120)
}

/// Like [`delete_account_body_bytes`], but valid for `valid_seconds`.
pub fn delete_account_body_bytes_valid_for(
    p: &DeleteAccountParams,
    valid_seconds: i64,
) -> Result<Vec<u8>, String> {
    if p.delete_account == p.transfer_account {
        return Err("The recipient must be a different account.".into());
    }
    if p.payer != p.delete_account && p.payer != p.transfer_account {
        return Err("The fee payer must be one of the two accounts.".into());
    }
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.payer)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration {
            seconds: valid_seconds,
        }),
        memo: String::new(),
        crypto_create: None,
        crypto_transfer: None,
        crypto_update: None,
        token_creation: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
        crypto_delete: Some(CryptoDeleteBody {
            transfer_account: Some(account(p.transfer_account)),
            delete_account: Some(account(p.delete_account)),
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `CryptoUpdate` that replaces an account's key. The account
/// pays for the transaction itself.
pub struct ChangeKeyParams {
    pub account: (i64, i64, i64),
    pub new_key_type: KeyType,
    pub new_public_key: Vec<u8>,
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
}

fn key_message(key_type: KeyType, public_key: &[u8]) -> Result<Key, String> {
    match (key_type, public_key.len()) {
        (KeyType::Ed25519, 32) => Ok(Key {
            ed25519: Some(public_key.to_vec()),
            ecdsa_secp256k1: None,
        }),
        (KeyType::Ecdsa, 33) => Ok(Key {
            ed25519: None,
            ecdsa_secp256k1: Some(public_key.to_vec()),
        }),
        _ => Err("The public key has an unexpected length.".into()),
    }
}

/// Serialised `TransactionBody` that sets a new key on `account`. The network
/// requires signatures from both the current key and the new key.
pub fn change_key_body_bytes(p: &ChangeKeyParams) -> Result<Vec<u8>, String> {
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.account)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration { seconds: 120 }),
        memo: String::new(),
        crypto_create: None,
        crypto_delete: None,
        crypto_transfer: None,
        token_creation: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
        crypto_update: Some(CryptoUpdateBody {
            account_to_update: Some(account(p.account)),
            key: Some(key_message(p.new_key_type, &p.new_public_key)?),
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `TokenCreate` for a fungible token. The payer is the
/// treasury and auto-renew account, and its key is the admin and supply key.
pub struct CreateTokenParams {
    pub payer: (i64, i64, i64),
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    /// In the token's smallest unit.
    pub initial_supply: u64,
    pub key_type: KeyType,
    pub public_key: Vec<u8>,
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
}

pub const MAX_TOKEN_DECIMALS: u32 = 18;
pub const MAX_TOKEN_TEXT_BYTES: usize = 100;

pub fn check_token_text(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > MAX_TOKEN_TEXT_BYTES {
        return Err(format!("The token {label} must be 1 to 100 bytes."));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(format!(
            "The token {label} cannot contain control characters."
        ));
    }
    Ok(())
}

pub fn create_token_body_bytes(p: &CreateTokenParams) -> Result<Vec<u8>, String> {
    check_token_text("name", &p.name)?;
    check_token_text("symbol", &p.symbol)?;
    if p.decimals > MAX_TOKEN_DECIMALS {
        return Err("Decimals can be 18 at most.".into());
    }
    if p.initial_supply > i64::MAX as u64 {
        return Err("The initial supply is too large.".into());
    }
    let key = key_message(p.key_type, &p.public_key)?;
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.payer)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration { seconds: 120 }),
        memo: String::new(),
        crypto_create: None,
        crypto_delete: None,
        crypto_transfer: None,
        crypto_update: None,
        token_deletion: None,
        token_association: None,
        token_dissociation: None,
        token_creation: Some(TokenCreateBody {
            name: p.name.clone(),
            symbol: p.symbol.clone(),
            decimals: p.decimals,
            initial_supply: p.initial_supply,
            treasury: Some(account(p.payer)),
            admin_key: Some(key.clone()),
            supply_key: Some(key),
            auto_renew_account: Some(account(p.payer)),
            auto_renew_period: Some(Duration {
                seconds: AUTO_RENEW_SECONDS,
            }),
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `TokenDelete`. The payer signs along with the token's admin key.
pub struct DeleteTokenParams {
    pub payer: (i64, i64, i64),
    pub token: (i64, i64, i64),
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
}

pub fn delete_token_body_bytes(p: &DeleteTokenParams) -> Result<Vec<u8>, String> {
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.payer)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration { seconds: 120 }),
        memo: String::new(),
        crypto_create: None,
        crypto_delete: None,
        crypto_transfer: None,
        crypto_update: None,
        token_creation: None,
        token_association: None,
        token_dissociation: None,
        token_deletion: Some(TokenDeleteBody {
            token: Some(TokenId {
                shard: p.token.0,
                realm: p.token.1,
                num: p.token.2,
            }),
        }),
    };
    Ok(body.encode_to_vec())
}

/// Parameters of a `TokenAssociate` or `TokenDissociate` for the payer's own account.
pub struct TokenAssociationParams {
    pub payer: (i64, i64, i64),
    pub token: (i64, i64, i64),
    pub node: (i64, i64, i64),
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub max_fee_tinybars: u64,
}

pub fn token_association_body_bytes(
    p: &TokenAssociationParams,
    associate: bool,
) -> Result<Vec<u8>, String> {
    if p.token.0 != 0 || p.token.1 != 0 || p.token.2 <= 0 {
        return Err("Invalid token ID.".into());
    }
    let change = TokenAssociationBody {
        account: Some(account(p.payer)),
        tokens: vec![TokenId {
            shard: p.token.0,
            realm: p.token.1,
            num: p.token.2,
        }],
    };
    let (association, dissociation) = if associate {
        (Some(change), None)
    } else {
        (None, Some(change))
    };
    let body = TransactionBody {
        transaction_id: Some(TransactionId {
            valid_start: Some(Timestamp {
                seconds: p.valid_start_seconds,
                nanos: p.valid_start_nanos,
            }),
            account: Some(account(p.payer)),
        }),
        node_account: Some(account(p.node)),
        fee: p.max_fee_tinybars,
        valid_duration: Some(Duration { seconds: 120 }),
        memo: String::new(),
        crypto_create: None,
        crypto_delete: None,
        crypto_transfer: None,
        crypto_update: None,
        token_creation: None,
        token_deletion: None,
        token_association: association,
        token_dissociation: dissociation,
    };
    Ok(body.encode_to_vec())
}

/// Signs `body_bytes` and returns the serialised `Transaction` ready to submit.
/// Ed25519 signs the bytes directly; ECDSA signs their Keccak-256 hash (r || s).
pub fn sign_body(
    body_bytes: &[u8],
    key_type: KeyType,
    secret: &[u8; 32],
) -> Result<Vec<u8>, String> {
    sign_body_multi(body_bytes, &[(key_type, secret)])
}

/// Like [`sign_body`] but with several signers, for transactions the network
/// requires more than one key to authorise (such as changing an account's key).
pub fn sign_body_multi(
    body_bytes: &[u8],
    signers: &[(KeyType, &[u8; 32])],
) -> Result<Vec<u8>, String> {
    let mut pairs = Vec::with_capacity(signers.len());
    for (key_type, secret) in signers {
        pairs.push(signature_pair(body_bytes, *key_type, secret)?);
    }
    let signed = SignedTransaction {
        body_bytes: body_bytes.to_vec(),
        sig_map: Some(SignatureMap { pairs }),
    };
    Ok(Transaction {
        signed_transaction_bytes: signed.encode_to_vec(),
    }
    .encode_to_vec())
}

pub(crate) fn signature_pair(
    body_bytes: &[u8],
    key_type: KeyType,
    secret: &[u8; 32],
) -> Result<SignaturePair, String> {
    let public = public_key_bytes(key_type, secret)?;
    Ok(match key_type {
        KeyType::Ed25519 => {
            let key = ed25519_dalek::SigningKey::from_bytes(secret);
            SignaturePair {
                pub_key_prefix: public,
                ed25519: Some(key.sign(body_bytes).to_bytes().to_vec()),
                ecdsa_secp256k1: None,
            }
        }
        KeyType::Ecdsa => {
            let key = EcdsaKey::from_slice(secret).map_err(|e| e.to_string())?;
            let digest = Keccak256::digest(body_bytes);
            let signature: k256::ecdsa::Signature =
                key.sign_prehash(&digest).map_err(|e| e.to_string())?;
            SignaturePair {
                pub_key_prefix: public,
                ed25519: None,
                ecdsa_secp256k1: Some(signature.to_bytes().to_vec()),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Verifier;
    use k256::ecdsa::signature::hazmat::PrehashVerifier;

    fn params() -> TransferParams {
        TransferParams {
            sender: (0, 0, 1001),
            receiver: (0, 0, 1002),
            tinybars: 100_000_000,
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 200_000_000,
            memo: String::new(),
        }
    }

    #[test]
    fn create_account_body_carries_key_balance_and_renewal() {
        let p = CreateAccountParams {
            payer: (0, 0, 1001),
            key_type: KeyType::Ecdsa,
            public_key: vec![2u8; 33],
            initial_tinybars: 50_000_000,
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 200_000_000,
            memo: String::new(),
        };
        let body =
            TransactionBody::decode(create_account_body_bytes(&p).unwrap().as_slice()).unwrap();
        assert!(body.crypto_transfer.is_none());
        let create = body.crypto_create.unwrap();
        assert_eq!(create.initial_balance, 50_000_000);
        assert_eq!(create.auto_renew_period.unwrap().seconds, 7_776_000);
        let key = create.key.unwrap();
        assert_eq!(key.ecdsa_secp256k1.unwrap(), vec![2u8; 33]);
        assert!(key.ed25519.is_none());

        let mut bad = p;
        bad.public_key = vec![2u8; 32];
        assert!(create_account_body_bytes(&bad).is_err());
    }

    #[test]
    fn change_key_body_and_two_signatures() {
        let old = [3u8; 32];
        let new = [4u8; 32];
        let new_public = public_key_bytes(KeyType::Ecdsa, &new).unwrap();
        let p = ChangeKeyParams {
            account: (0, 0, 1001),
            new_key_type: KeyType::Ecdsa,
            new_public_key: new_public.clone(),
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 200_000_000,
        };
        let bytes = change_key_body_bytes(&p).unwrap();
        let body = TransactionBody::decode(bytes.as_slice()).unwrap();
        // cryptoUpdateAccount is TransactionBody field 15.
        assert!(body.crypto_transfer.is_none() && body.crypto_create.is_none());
        let update = body.crypto_update.unwrap();
        assert_eq!(update.account_to_update.unwrap().num, 1001);
        assert_eq!(update.key.unwrap().ecdsa_secp256k1.unwrap(), new_public);

        let tx =
            sign_body_multi(&bytes, &[(KeyType::Ed25519, &old), (KeyType::Ecdsa, &new)]).unwrap();
        let outer = Transaction::decode(tx.as_slice()).unwrap();
        let signed = SignedTransaction::decode(outer.signed_transaction_bytes.as_slice()).unwrap();
        let pairs = signed.sig_map.unwrap().pairs;
        assert_eq!(pairs.len(), 2);
        assert!(pairs[0].ed25519.is_some());
        assert!(pairs[1].ecdsa_secp256k1.is_some());

        let mut bad = p;
        bad.new_public_key = vec![1u8; 10];
        assert!(change_key_body_bytes(&bad).is_err());
    }

    #[test]
    fn delete_body_names_both_accounts_and_checks_payer() {
        let p = DeleteAccountParams {
            delete_account: (0, 0, 1001),
            transfer_account: (0, 0, 1002),
            payer: (0, 0, 1002),
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 200_000_000,
        };
        let body =
            TransactionBody::decode(delete_account_body_bytes(&p).unwrap().as_slice()).unwrap();
        // cryptoDelete is TransactionBody field 12; transfer is field 1, delete is field 2.
        let delete = body.crypto_delete.unwrap();
        assert_eq!(delete.transfer_account.unwrap().num, 1002);
        assert_eq!(delete.delete_account.unwrap().num, 1001);
        assert_eq!(body.transaction_id.unwrap().account.unwrap().num, 1002);
        assert!(body.crypto_create.is_none() && body.crypto_update.is_none());

        let mut same = DeleteAccountParams { ..p };
        same.transfer_account = same.delete_account;
        assert!(delete_account_body_bytes(&same).is_err());
        let mut stranger = DeleteAccountParams { ..same };
        stranger.transfer_account = (0, 0, 1002);
        stranger.payer = (0, 0, 7);
        assert!(delete_account_body_bytes(&stranger).is_err());
    }

    #[test]
    fn token_create_body_uses_payer_as_treasury_and_key() {
        let p = CreateTokenParams {
            payer: (0, 0, 1001),
            name: "Gold".into(),
            symbol: "GLD".into(),
            decimals: 2,
            initial_supply: 100_000,
            key_type: KeyType::Ed25519,
            public_key: vec![7u8; 32],
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 2_000_000_000,
        };
        let body =
            TransactionBody::decode(create_token_body_bytes(&p).unwrap().as_slice()).unwrap();
        let create = body.token_creation.unwrap();
        assert_eq!(create.name, "Gold");
        assert_eq!(create.decimals, 2);
        assert_eq!(create.initial_supply, 100_000);
        assert_eq!(create.treasury.unwrap().num, 1001);
        assert_eq!(create.admin_key, create.supply_key);
        assert!(create.admin_key.is_some());
        assert!(body.token_deletion.is_none() && body.crypto_transfer.is_none());

        let mut bad = CreateTokenParams {
            name: String::new(),
            ..p
        };
        assert!(create_token_body_bytes(&bad).is_err());
        bad.name = "Gold".into();
        bad.decimals = 19;
        assert!(create_token_body_bytes(&bad).is_err());
        bad.decimals = 0;
        bad.initial_supply = u64::MAX;
        assert!(create_token_body_bytes(&bad).is_err());
    }

    #[test]
    fn association_bodies_name_the_account_and_token() {
        let p = TokenAssociationParams {
            payer: (0, 0, 1001),
            token: (0, 0, 4242),
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 100_000_000,
        };
        let on =
            TransactionBody::decode(token_association_body_bytes(&p, true).unwrap().as_slice())
                .unwrap();
        let change = on.token_association.unwrap();
        assert_eq!(change.account.unwrap().num, 1001);
        assert_eq!(change.tokens.len(), 1);
        assert_eq!(change.tokens[0].num, 4242);
        assert!(on.token_dissociation.is_none());
        let off =
            TransactionBody::decode(token_association_body_bytes(&p, false).unwrap().as_slice())
                .unwrap();
        assert!(off.token_association.is_none());
        assert_eq!(off.token_dissociation.unwrap().tokens[0].num, 4242);
        let bad = TokenAssociationParams {
            token: (0, 0, 0),
            ..p
        };
        assert!(token_association_body_bytes(&bad, true).is_err());
    }

    #[test]
    fn token_delete_body_names_the_token() {
        let p = DeleteTokenParams {
            payer: (0, 0, 1001),
            token: (0, 0, 4242),
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 100_000_000,
        };
        let body =
            TransactionBody::decode(delete_token_body_bytes(&p).unwrap().as_slice()).unwrap();
        assert_eq!(body.token_deletion.unwrap().token.unwrap().num, 4242);
        assert!(body.token_creation.is_none());
    }

    #[test]
    fn wire_format_matches_hand_computed_bytes() {
        // Zero-valued shard and realm are omitted; num is field 3.
        assert_eq!(account((0, 0, 3)).encode_to_vec(), vec![0x18, 0x03]);
        // Amount is sint64 (zigzag): 1 -> 2, -1 -> 1.
        let credit = AccountAmount {
            account: None,
            amount: 1,
        };
        assert_eq!(credit.encode_to_vec(), vec![0x10, 0x02]);
        let debit = AccountAmount {
            account: None,
            amount: -1,
        };
        assert_eq!(debit.encode_to_vec(), vec![0x10, 0x01]);
        // Transaction wraps signedTransactionBytes as field 5.
        let tx = Transaction {
            signed_transaction_bytes: vec![0xAA],
        };
        assert_eq!(tx.encode_to_vec(), vec![0x2A, 0x01, 0xAA]);
        // cryptoTransfer is TransactionBody field 14 (tag 0x72).
        let body = TransactionBody {
            crypto_transfer: Some(CryptoTransferBody {
                transfers: None,
                token_transfers: Vec::new(),
            }),
            ..Default::default()
        };
        assert_eq!(body.encode_to_vec(), vec![0x72, 0x00]);
    }

    #[test]
    fn token_transfer_body_moves_units_and_pins_decimals() {
        let p = TokenTransferParams {
            sender: (0, 0, 1001),
            receiver: (0, 0, 1002),
            token: (0, 0, 5005),
            amount: 1250,
            decimals: 2,
            node: (0, 0, 3),
            valid_start_seconds: 1_700_000_000,
            valid_start_nanos: 5,
            max_fee_tinybars: 100_000_000,
            memo: "hi".to_string(),
        };
        let body =
            TransactionBody::decode(token_transfer_body_bytes(&p).unwrap().as_slice()).unwrap();
        let transfer = body.crypto_transfer.unwrap();
        assert!(transfer.transfers.is_none());
        let list = &transfer.token_transfers[0];
        assert_eq!(list.token.as_ref().unwrap().num, 5005);
        assert_eq!(list.expected_decimals.as_ref().unwrap().value, 2);
        assert_eq!(list.transfers[0].account.as_ref().unwrap().num, 1001);
        assert_eq!(list.transfers[0].amount, -1250);
        assert_eq!(list.transfers[1].account.as_ref().unwrap().num, 1002);
        assert_eq!(list.transfers[1].amount, 1250);
        let zero = TokenTransferParams { amount: 0, ..p };
        assert!(token_transfer_body_bytes(&zero).is_err());
    }

    #[test]
    fn body_round_trips_with_expected_amounts() {
        let bytes = transfer_body_bytes(&params()).unwrap();
        let body = TransactionBody::decode(bytes.as_slice()).unwrap();
        assert_eq!(body.fee, 200_000_000);
        assert_eq!(body.valid_duration.unwrap().seconds, 120);
        let list = body.crypto_transfer.unwrap().transfers.unwrap().amounts;
        assert_eq!(list[0].amount, -100_000_000);
        assert_eq!(list[0].account.as_ref().unwrap().num, 1001);
        assert_eq!(list[1].amount, 100_000_000);
        assert_eq!(list[1].account.as_ref().unwrap().num, 1002);
    }

    #[test]
    fn rejects_bad_amounts_and_self_transfer() {
        let mut p = params();
        p.tinybars = 0;
        assert!(transfer_body_bytes(&p).is_err());
        let mut p = params();
        p.receiver = p.sender;
        assert!(transfer_body_bytes(&p).is_err());
    }

    fn unwrap_pair(tx: &[u8]) -> (Vec<u8>, SignaturePair) {
        let outer = Transaction::decode(tx).unwrap();
        let mut signed =
            SignedTransaction::decode(outer.signed_transaction_bytes.as_slice()).unwrap();
        let pair = signed.sig_map.take().unwrap().pairs.remove(0);
        (signed.body_bytes, pair)
    }

    #[test]
    fn ed25519_signature_verifies_over_raw_body() {
        let secret = [7u8; 32];
        let body = transfer_body_bytes(&params()).unwrap();
        let (b, pair) = unwrap_pair(&sign_body(&body, KeyType::Ed25519, &secret).unwrap());
        assert_eq!(b, body);
        let key = ed25519_dalek::SigningKey::from_bytes(&secret).verifying_key();
        assert_eq!(pair.pub_key_prefix, key.to_bytes().to_vec());
        assert!(pair.ecdsa_secp256k1.is_none());
        let sig = ed25519_dalek::Signature::from_slice(&pair.ed25519.unwrap()).unwrap();
        assert!(key.verify(&body, &sig).is_ok());
    }

    #[test]
    fn ecdsa_signature_verifies_over_keccak_of_body() {
        let secret = [9u8; 32];
        let body = transfer_body_bytes(&params()).unwrap();
        let (_, pair) = unwrap_pair(&sign_body(&body, KeyType::Ecdsa, &secret).unwrap());
        assert_eq!(pair.pub_key_prefix.len(), 33);
        assert!(pair.ed25519.is_none());
        let sig_bytes = pair.ecdsa_secp256k1.unwrap();
        assert_eq!(sig_bytes.len(), 64);
        let key = EcdsaKey::from_slice(&secret).unwrap();
        let sig = k256::ecdsa::Signature::from_slice(&sig_bytes).unwrap();
        let digest = Keccak256::digest(&body);
        assert!(key.verifying_key().verify_prehash(&digest, &sig).is_ok());
    }
}

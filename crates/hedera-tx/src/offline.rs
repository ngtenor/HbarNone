//! Offline signing for watch-only wallets.
//!
//! The app builds an unsigned HBAR transfer and hands it to the user as text
//! (`HVTX1-REQ:<hex>`, suitable for a QR code). A separate signer returns
//! `HVTX1-SIG:<hex>`, which the app verifies and submits. Payloads use
//! uppercase hex so they fit QR alphanumeric mode.

use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::Verifier;
use k256::ecdsa::VerifyingKey as EcdsaVerifyingKey;
use k256::ecdsa::signature::hazmat::PrehashVerifier;
use prost::Message;
use sha3::{Digest, Keccak256};
use wallet_core::key_import::KeyType;

use crate::AUTO_RENEW_SECONDS;
use crate::submit::{
    CREATE_PATH, DELETE_PATH, MAX_FEE_TINYBARS, MAX_INITIAL_TINYBARS, MAX_MEMO_BYTES,
    TESTNET_NODE_ACCOUNT, TOKEN_ASSOCIATE_PATH, TOKEN_DISSOCIATE_PATH, TRANSFER_PATH, UPDATE_PATH,
    parse_account, submit,
};
use crate::{
    ChangeKeyParams, CreateAccountParams, DeleteAccountParams, NftTransferParams, SignatureMap,
    SignaturePair, SignedTransaction, TokenAssociationParams, TokenTransferParams, Transaction,
    TransactionBody, TransferParams, change_key_body_bytes_valid_for,
    create_account_body_bytes_valid_for, delete_account_body_bytes_valid_for,
    nft_transfer_body_bytes_valid_for, signature_pair, token_association_body_bytes_valid_for,
    token_transfer_body_bytes_valid_for, transfer_body_bytes_valid_for,
};

pub const REQUEST_PREFIX: &str = "HVTX1-REQ:";
pub const RESPONSE_PREFIX: &str = "HVTX1-SIG:";
/// Hedera's maximum transaction validity window.
pub const VALID_SECONDS: i64 = 180;
const BACKDATE_SECONDS: u64 = 10;

/// The one thing a request does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Transfer {
        receiver: String,
        tinybars: i64,
    },
    /// Deletes `account` and sends its remaining HBAR to `recipient`.
    DeleteAccount {
        account: String,
        recipient: String,
    },
    /// Creates an account controlled by the public key `key_hex`.
    CreateAccount {
        key_type: KeyType,
        key_hex: String,
        initial_tinybars: i64,
    },
    /// Sends `amount` (smallest unit) of a fungible token. `decimals` is the sender's
    /// expectation, which the network checks.
    TokenTransfer {
        token: String,
        receiver: String,
        amount: i64,
        decimals: u32,
    },
    /// Sends one NFT of `token`, identified by its serial number.
    NftTransfer {
        token: String,
        serial_number: i64,
        receiver: String,
    },
    /// Lets the payer's account hold `token`.
    TokenAssociate {
        token: String,
    },
    /// Stops the payer's account holding `token`.
    TokenDissociate {
        token: String,
    },
    /// Replaces the payer's account key with `key_hex`. The network requires
    /// signatures from both the current key and the new key.
    KeyUpdate {
        key_type: KeyType,
        key_hex: String,
    },
}

/// What a request will do, decoded from the transaction body itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDescription {
    /// The account that pays the fee and whose key signs.
    pub payer: String,
    pub action: Action,
    /// The transaction memo (transfers) or the new account's memo (creation).
    pub memo: String,
    pub fee_limit_tinybars: u64,
    pub valid_start_seconds: i64,
    pub valid_start_nanos: i32,
    pub expires_at_seconds: i64,
}

impl RequestDescription {
    /// A one-line summary for logs and the example signer.
    pub fn action_text(&self) -> String {
        match &self.action {
            Action::Transfer { receiver, tinybars } => {
                format!("send {tinybars} tinybars to {receiver}")
            }
            Action::DeleteAccount { account, recipient } => {
                format!("delete {account} and send its HBAR to {recipient}")
            }
            Action::CreateAccount {
                key_hex,
                initial_tinybars,
                ..
            } => format!("create an account with key {key_hex} and {initial_tinybars} tinybars"),
            Action::TokenTransfer {
                token,
                receiver,
                amount,
                decimals,
            } => {
                format!("send {amount} units of token {token} ({decimals} decimals) to {receiver}")
            }
            Action::NftTransfer {
                token,
                serial_number,
                receiver,
            } => {
                format!("send NFT {token} serial {serial_number} to {receiver}")
            }
            Action::TokenAssociate { token } => format!("associate token {token}"),
            Action::TokenDissociate { token } => format!("dissociate token {token}"),
            Action::KeyUpdate { key_hex, .. } => {
                format!("replace the account's key with {key_hex}")
            }
        }
    }
    pub fn transaction_id(&self) -> String {
        format!(
            "{}-{}-{:09}",
            self.payer, self.valid_start_seconds, self.valid_start_nanos
        )
    }
}

pub struct UnsignedRequest {
    pub payload: String,
    pub description: RequestDescription,
}

fn now_seconds() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "The system clock is before 1970.".to_string())?
        .as_secs())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("The text is not valid.".to_string());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

fn strip<'a>(text: &'a str, prefix: &str, what: &str) -> Result<&'a str, String> {
    let compact = text.trim();
    compact
        .strip_prefix(prefix)
        .ok_or_else(|| format!("This is not a signing {what} from HBarNone."))
}

/// Builds an unsigned transfer from `sender` to `receiver`, valid for 180 seconds.
pub fn build_request(
    sender: &str,
    receiver: &str,
    tinybars: i64,
    memo: &str,
) -> Result<UnsignedRequest, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    let (seconds, nanos) = start_time()?;
    let params = TransferParams {
        sender: parse_account(sender)?,
        receiver: parse_account(receiver)?,
        tinybars,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: memo.to_string(),
    };
    finish(transfer_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

/// Builds an unsigned deletion of `account`, sending its HBAR to `recipient`,
/// who pays the fee. Both accounts must be controlled by the signing key.
pub fn build_delete_request(account: &str, recipient: &str) -> Result<UnsignedRequest, String> {
    let (seconds, nanos) = start_time()?;
    let recipient_id = parse_account(recipient)?;
    let params = DeleteAccountParams {
        delete_account: parse_account(account)?,
        transfer_account: recipient_id,
        payer: recipient_id,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    finish(delete_account_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

/// Builds an unsigned creation of an account controlled by `public_key`,
/// funded and paid for by `payer`.
pub fn build_create_request(
    payer: &str,
    key_type: KeyType,
    public_key: &[u8],
    initial_tinybars: i64,
    memo: &str,
) -> Result<UnsignedRequest, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    if !(0..=MAX_INITIAL_TINYBARS).contains(&initial_tinybars) {
        return Err("The opening balance is out of range.".to_string());
    }
    let (seconds, nanos) = start_time()?;
    let params = CreateAccountParams {
        payer: parse_account(payer)?,
        key_type,
        public_key: public_key.to_vec(),
        initial_tinybars: initial_tinybars as u64,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: memo.to_string(),
    };
    finish(create_account_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

/// Builds an unsigned token transfer. `amount` is in the token's smallest unit.
pub fn build_token_transfer_request(
    sender: &str,
    receiver: &str,
    token: &str,
    amount: i64,
    decimals: u32,
    memo: &str,
) -> Result<UnsignedRequest, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    let (seconds, nanos) = start_time()?;
    let params = TokenTransferParams {
        sender: parse_account(sender)?,
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
    finish(token_transfer_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

/// Builds an unsigned NFT transfer of one serial number.
pub fn build_nft_transfer_request(
    sender: &str,
    receiver: &str,
    token: &str,
    serial_number: i64,
    memo: &str,
) -> Result<UnsignedRequest, String> {
    if memo.len() > MAX_MEMO_BYTES {
        return Err("The memo is too long (100 bytes at most).".to_string());
    }
    let (seconds, nanos) = start_time()?;
    let params = NftTransferParams {
        sender: parse_account(sender)?,
        receiver: parse_account(receiver)?,
        token: parse_account(token)?,
        serial_number,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
        memo: memo.to_string(),
    };
    finish(nft_transfer_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

/// Builds an unsigned association (or dissociation) of `token` with `payer`'s own account.
pub fn build_token_association_request(
    payer: &str,
    token: &str,
    associate: bool,
) -> Result<UnsignedRequest, String> {
    let (seconds, nanos) = start_time()?;
    let params = TokenAssociationParams {
        payer: parse_account(payer)?,
        token: parse_account(token)?,
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    finish(token_association_body_bytes_valid_for(
        &params,
        associate,
        VALID_SECONDS,
    )?)
}

/// Builds an unsigned key change: `account` pays and its key is replaced by
/// `public_key`. Both the current key and the new key must sign.
pub fn build_key_update_request(
    account: &str,
    key_type: KeyType,
    public_key: &[u8],
) -> Result<UnsignedRequest, String> {
    let (seconds, nanos) = start_time()?;
    let params = ChangeKeyParams {
        account: parse_account(account)?,
        new_key_type: key_type,
        new_public_key: public_key.to_vec(),
        node: TESTNET_NODE_ACCOUNT,
        valid_start_seconds: seconds,
        valid_start_nanos: nanos,
        max_fee_tinybars: MAX_FEE_TINYBARS,
    };
    finish(change_key_body_bytes_valid_for(&params, VALID_SECONDS)?)
}

fn start_time() -> Result<(i64, i32), String> {
    Ok((now_seconds()?.saturating_sub(BACKDATE_SECONDS) as i64, 0))
}

fn finish(body: Vec<u8>) -> Result<UnsignedRequest, String> {
    let description = describe_body(&body)?;
    Ok(UnsignedRequest {
        payload: format!("{REQUEST_PREFIX}{}", to_hex(&body)),
        description,
    })
}

fn account_text(id: &crate::AccountId) -> String {
    format!("{}.{}.{}", id.shard, id.realm, id.num)
}

/// Decodes and strictly validates a request body: exactly one supported action
/// to the Testnet node, with nothing else hidden in it.
pub fn describe_body(body_bytes: &[u8]) -> Result<RequestDescription, String> {
    let bad = || "The request is not a supported transaction.".to_string();
    let body = TransactionBody::decode(body_bytes).map_err(|_| bad())?;
    if body.encode_to_vec() != body_bytes {
        return Err(bad());
    }
    let id = body.transaction_id.as_ref().ok_or_else(bad)?;
    let start = id.valid_start.as_ref().ok_or_else(bad)?;
    let payer = id.account.as_ref().ok_or_else(bad)?;
    let node = body.node_account.as_ref().ok_or_else(bad)?;
    let (ns, nr, nn) = TESTNET_NODE_ACCOUNT;
    if (node.shard, node.realm, node.num) != (ns, nr, nn) {
        return Err(bad());
    }
    let valid = body.valid_duration.as_ref().ok_or_else(bad)?.seconds;
    let actions = usize::from(body.crypto_transfer.is_some())
        + usize::from(body.crypto_create.is_some())
        + usize::from(body.crypto_delete.is_some())
        + usize::from(body.crypto_update.is_some())
        + usize::from(body.token_association.is_some())
        + usize::from(body.token_dissociation.is_some());
    let on_main = |a: &crate::AccountId| a.shard == 0 && a.realm == 0 && a.num > 0;
    if !(1..=VALID_SECONDS).contains(&valid)
        || body.fee > MAX_FEE_TINYBARS
        || body.memo.len() > MAX_MEMO_BYTES
        || body.token_creation.is_some()
        || body.token_deletion.is_some()
        || body.token_mint.is_some()
        || body.token_burn.is_some()
        || actions != 1
        || !on_main(payer)
        || !(0..1_000_000_000).contains(&start.nanos)
    {
        return Err(bad());
    }
    let token_text = |t: &crate::TokenId| format!("{}.{}.{}", t.shard, t.realm, t.num);
    let token_ok = |t: &crate::TokenId| t.shard == 0 && t.realm == 0 && t.num > 0;
    let (action, memo) = if let Some(update) = body.crypto_update.as_ref() {
        let account = update.account_to_update.as_ref().ok_or_else(bad)?;
        let key = update.key.as_ref().ok_or_else(bad)?;
        let (key_type, key_bytes) = match (&key.ed25519, &key.ecdsa_secp256k1) {
            (Some(k), None) if k.len() == 32 => (KeyType::Ed25519, k),
            (None, Some(k)) if k.len() == 33 => (KeyType::Ecdsa, k),
            _ => return Err(bad()),
        };
        if account != payer || !body.memo.is_empty() {
            return Err(bad());
        }
        (
            Action::KeyUpdate {
                key_type,
                key_hex: key_bytes.iter().map(|b| format!("{b:02x}")).collect(),
            },
            String::new(),
        )
    } else if let Some(change) = body
        .token_association
        .as_ref()
        .or(body.token_dissociation.as_ref())
    {
        let [token] = change.tokens.as_slice() else {
            return Err(bad());
        };
        if change.account.as_ref() != Some(payer) || !token_ok(token) || !body.memo.is_empty() {
            return Err(bad());
        }
        let token = token_text(token);
        let action = if body.token_association.is_some() {
            Action::TokenAssociate { token }
        } else {
            Action::TokenDissociate { token }
        };
        (action, String::new())
    } else if let Some(transfer) = body
        .crypto_transfer
        .as_ref()
        .filter(|t| !t.token_transfers.is_empty())
    {
        let [list] = transfer.token_transfers.as_slice() else {
            return Err(bad());
        };
        let token = list.token.as_ref().ok_or_else(bad)?;
        if transfer.transfers.is_some() || !token_ok(token) {
            return Err(bad());
        }
        if !list.nft_transfers.is_empty() {
            // An NFT transfer: exactly one serial number, nothing fungible.
            let [nft] = list.nft_transfers.as_slice() else {
                return Err(bad());
            };
            let sender = nft.sender.as_ref().ok_or_else(bad)?;
            let receiver = nft.receiver.as_ref().ok_or_else(bad)?;
            if !list.transfers.is_empty()
                || list.expected_decimals.is_some()
                || sender != payer
                || receiver == payer
                || !on_main(receiver)
                || nft.serial_number <= 0
            {
                return Err(bad());
            }
            (
                Action::NftTransfer {
                    token: token_text(token),
                    serial_number: nft.serial_number,
                    receiver: account_text(receiver),
                },
                body.memo.clone(),
            )
        } else {
            let decimals = list.expected_decimals.as_ref().ok_or_else(bad)?.value;
            let [debit, credit] = list.transfers.as_slice() else {
                return Err(bad());
            };
            let debit_account = debit.account.as_ref().ok_or_else(bad)?;
            let credit_account = credit.account.as_ref().ok_or_else(bad)?;
            if debit_account != payer
                || credit_account == payer
                || !on_main(credit_account)
                || debit.amount >= 0
                || debit.amount.checked_neg() != Some(credit.amount)
            {
                return Err(bad());
            }
            (
                Action::TokenTransfer {
                    token: token_text(token),
                    receiver: account_text(credit_account),
                    amount: credit.amount,
                    decimals,
                },
                body.memo.clone(),
            )
        }
    } else if let Some(transfer) = &body.crypto_transfer {
        let amounts = &transfer.transfers.as_ref().ok_or_else(bad)?.amounts;
        let [debit, credit] = amounts.as_slice() else {
            return Err(bad());
        };
        let debit_account = debit.account.as_ref().ok_or_else(bad)?;
        let credit_account = credit.account.as_ref().ok_or_else(bad)?;
        if debit_account != payer
            || credit_account == payer
            || debit.amount >= 0
            || debit.amount.checked_neg() != Some(credit.amount)
            || !on_main(credit_account)
        {
            return Err(bad());
        }
        (
            Action::Transfer {
                receiver: account_text(credit_account),
                tinybars: credit.amount,
            },
            body.memo.clone(),
        )
    } else if let Some(delete) = &body.crypto_delete {
        let account = delete.delete_account.as_ref().ok_or_else(bad)?;
        let recipient = delete.transfer_account.as_ref().ok_or_else(bad)?;
        if !body.memo.is_empty()
            || account == recipient
            || !on_main(account)
            || !on_main(recipient)
            || (payer != account && payer != recipient)
        {
            return Err(bad());
        }
        (
            Action::DeleteAccount {
                account: account_text(account),
                recipient: account_text(recipient),
            },
            String::new(),
        )
    } else {
        let create = body.crypto_create.as_ref().ok_or_else(bad)?;
        let key = create.key.as_ref().ok_or_else(bad)?;
        let (key_type, key_bytes) = match (&key.ed25519, &key.ecdsa_secp256k1) {
            (Some(k), None) if k.len() == 32 => (KeyType::Ed25519, k),
            (None, Some(k)) if k.len() == 33 => (KeyType::Ecdsa, k),
            _ => return Err(bad()),
        };
        let renew = create.auto_renew_period.as_ref().ok_or_else(bad)?.seconds;
        let initial = i64::try_from(create.initial_balance).map_err(|_| bad())?;
        if !body.memo.is_empty()
            || renew != AUTO_RENEW_SECONDS
            || !(0..=MAX_INITIAL_TINYBARS).contains(&initial)
            || create.memo.len() > MAX_MEMO_BYTES
        {
            return Err(bad());
        }
        (
            Action::CreateAccount {
                key_type,
                key_hex: key_bytes.iter().map(|b| format!("{b:02x}")).collect(),
                initial_tinybars: initial,
            },
            create.memo.clone(),
        )
    };
    Ok(RequestDescription {
        payer: account_text(payer),
        action,
        memo,
        fee_limit_tinybars: body.fee,
        valid_start_seconds: start.seconds,
        valid_start_nanos: start.nanos,
        expires_at_seconds: start.seconds.saturating_add(valid),
    })
}

/// Decodes a request payload into its body bytes and description.
pub fn decode_request(payload: &str) -> Result<(Vec<u8>, RequestDescription), String> {
    let body = from_hex(strip(payload, REQUEST_PREFIX, "request")?)?;
    let description = describe_body(&body)?;
    Ok((body, description))
}

/// Signs a request the way an offline signer would. Used by the example signer and tests.
pub fn sign_request(payload: &str, key_type: KeyType, secret: &[u8; 32]) -> Result<String, String> {
    let (body, _) = decode_request(payload)?;
    let pair = signature_pair(&body, key_type, secret)?;
    let signature = pair
        .ed25519
        .or(pair.ecdsa_secp256k1)
        .ok_or_else(|| "No signature produced.".to_string())?;
    let tag = match key_type {
        KeyType::Ed25519 => 0u8,
        KeyType::Ecdsa => 1u8,
    };
    let mut raw = vec![tag];
    raw.extend_from_slice(&pair.pub_key_prefix);
    raw.extend_from_slice(&signature);
    Ok(format!("{RESPONSE_PREFIX}{}", to_hex(&raw)))
}

pub struct VerifiedRequest {
    pub transaction: Transaction,
    pub description: RequestDescription,
    pub key_type: KeyType,
    /// Lowercase hex of the signer's public key.
    pub public_key_hex: String,
}

fn verify(body: &[u8], key_type: KeyType, public: &[u8], signature: &[u8]) -> Result<(), String> {
    let fail = || "The signature does not match the request.".to_string();
    match key_type {
        KeyType::Ed25519 => {
            let key =
                ed25519_dalek::VerifyingKey::from_bytes(public.try_into().map_err(|_| fail())?)
                    .map_err(|_| fail())?;
            let sig = ed25519_dalek::Signature::from_slice(signature).map_err(|_| fail())?;
            key.verify(body, &sig).map_err(|_| fail())
        }
        KeyType::Ecdsa => {
            let key = EcdsaVerifyingKey::from_sec1_bytes(public).map_err(|_| fail())?;
            let sig = k256::ecdsa::Signature::from_slice(signature).map_err(|_| fail())?;
            key.verify_prehash(&Keccak256::digest(body), &sig)
                .map_err(|_| fail())
        }
    }
}

/// Checks the response against the request and assembles the signed transaction.
/// The caller must still check that `public_key_hex` belongs to the payer.
pub fn assemble(request: &str, response: &str) -> Result<VerifiedRequest, String> {
    let (body, description) = decode_request(request)?;
    let raw = from_hex(strip(response, RESPONSE_PREFIX, "signature")?)?;
    let bad = || "This is not a valid signature from a signer.".to_string();
    let (&tag, rest) = raw.split_first().ok_or_else(bad)?;
    let (key_type, key_len) = match tag {
        0 => (KeyType::Ed25519, 32),
        1 => (KeyType::Ecdsa, 33),
        _ => return Err(bad()),
    };
    if rest.len() != key_len + 64 {
        return Err(bad());
    }
    let (public, signature) = rest.split_at(key_len);
    verify(&body, key_type, public, signature)?;
    let pair = SignaturePair {
        pub_key_prefix: public.to_vec(),
        ed25519: (key_type == KeyType::Ed25519).then(|| signature.to_vec()),
        ecdsa_secp256k1: (key_type == KeyType::Ecdsa).then(|| signature.to_vec()),
    };
    let signed = SignedTransaction {
        body_bytes: body,
        sig_map: Some(SignatureMap { pairs: vec![pair] }),
    };
    Ok(VerifiedRequest {
        transaction: Transaction {
            signed_transaction_bytes: signed.encode_to_vec(),
        },
        description,
        key_type,
        public_key_hex: public.iter().map(|b| format!("{b:02x}")).collect(),
    })
}

/// Seconds left before the request expires (zero or negative once it has).
pub fn seconds_remaining(description: &RequestDescription) -> Result<i64, String> {
    Ok(description.expires_at_seconds - now_seconds()? as i64)
}

/// A request with one or more verified signatures collected from signers.
pub struct PartiallySigned {
    body: Vec<u8>,
    pub description: RequestDescription,
    pairs: Vec<SignaturePair>,
    /// Lowercase hex public keys, one per collected signature.
    pub signer_keys: Vec<String>,
}

/// Checks each response against the request. A duplicate signature (the same
/// public key twice) is kept once. Used for actions that need more than one
/// key, such as a key change signed by the current and the new key.
pub fn collect_signatures(request: &str, responses: &[String]) -> Result<PartiallySigned, String> {
    let (body, description) = decode_request(request)?;
    let mut collected = PartiallySigned {
        body,
        description,
        pairs: Vec::new(),
        signer_keys: Vec::new(),
    };
    let bad = || "This is not a valid signature from a signer.".to_string();
    for response in responses {
        let raw = from_hex(strip(response, RESPONSE_PREFIX, "signature")?)?;
        let (&tag, rest) = raw.split_first().ok_or_else(bad)?;
        let (key_type, key_len) = match tag {
            0 => (KeyType::Ed25519, 32),
            1 => (KeyType::Ecdsa, 33),
            _ => return Err(bad()),
        };
        if rest.len() != key_len + 64 {
            return Err(bad());
        }
        let (public, signature) = rest.split_at(key_len);
        verify(&collected.body, key_type, public, signature)?;
        let key_hex: String = public.iter().map(|b| format!("{b:02x}")).collect();
        if collected.signer_keys.contains(&key_hex) {
            continue;
        }
        collected.pairs.push(SignaturePair {
            pub_key_prefix: public.to_vec(),
            ed25519: (key_type == KeyType::Ed25519).then(|| signature.to_vec()),
            ecdsa_secp256k1: (key_type == KeyType::Ecdsa).then(|| signature.to_vec()),
        });
        collected.signer_keys.push(key_hex);
    }
    if collected.pairs.is_empty() {
        return Err("No valid signature was provided.".to_string());
    }
    Ok(collected)
}

impl PartiallySigned {
    /// Adds a signature from a key held in the vault (for example when the
    /// new key of a key change belongs to a hot wallet). A key that has
    /// already signed is not added twice.
    pub fn sign_locally(&mut self, key_type: KeyType, secret: &[u8; 32]) -> Result<(), String> {
        let key_hex = wallet_core::key_import::public_key_hex(key_type, secret)?.to_lowercase();
        if self.signer_keys.contains(&key_hex) {
            return Ok(());
        }
        let pair = signature_pair(&self.body, key_type, secret)?;
        self.pairs.push(pair);
        self.signer_keys.push(key_hex);
        Ok(())
    }

    /// True when the collected signatures satisfy the account's key
    /// structure. The signatures were each verified against the exact body
    /// bytes when collected; this only evaluates the threshold.
    pub fn satisfies(&self, tree: &crate::key_tree::KeyTree) -> bool {
        tree.is_satisfied_by(&self.signer_keys)
    }

    /// Leaf keys of the account's key structure that have not signed yet
    /// (lowercase hex), for progress messages.
    pub fn missing_keys(&self, tree: &crate::key_tree::KeyTree) -> Vec<String> {
        tree.leaf_keys()
            .into_iter()
            .map(|(_, hex)| hex)
            .filter(|hex| !self.signer_keys.contains(hex))
            .collect()
    }

    /// Submits the fully signed request and returns its transaction ID.
    pub async fn submit(self) -> Result<String, String> {
        if seconds_remaining(&self.description)? <= 0 {
            return Err("The signing request has expired. Create a new one.".to_string());
        }
        let signed = SignedTransaction {
            body_bytes: self.body,
            sig_map: Some(SignatureMap { pairs: self.pairs }),
        };
        let transaction = Transaction {
            signed_transaction_bytes: signed.encode_to_vec(),
        };
        submit_action(transaction, &self.description).await
    }
}

/// Submits a verified request to Testnet and returns its transaction ID.
pub async fn submit_verified(verified: &VerifiedRequest) -> Result<String, String> {
    if seconds_remaining(&verified.description)? <= 0 {
        return Err("The signing request has expired. Create a new one.".to_string());
    }
    submit_action(verified.transaction.clone(), &verified.description).await
}

async fn submit_action(
    transaction: Transaction,
    description: &RequestDescription,
) -> Result<String, String> {
    let (path, what) = match description.action {
        Action::Transfer { .. } => (TRANSFER_PATH, "transfer"),
        Action::DeleteAccount { .. } => (DELETE_PATH, "account deletion"),
        Action::CreateAccount { .. } => (CREATE_PATH, "account creation"),
        Action::TokenTransfer { .. } => (TRANSFER_PATH, "token transfer"),
        Action::NftTransfer { .. } => (TRANSFER_PATH, "NFT transfer"),
        Action::TokenAssociate { .. } => (TOKEN_ASSOCIATE_PATH, "token association"),
        Action::TokenDissociate { .. } => (TOKEN_DISSOCIATE_PATH, "token dissociation"),
        Action::KeyUpdate { .. } => (UPDATE_PATH, "key change"),
    };
    let response = submit(transaction, path).await?;
    if response.precheck_code != 0 {
        return Err(format!(
            "The node rejected the {what}: {}",
            crate::submit::precheck_message(response.precheck_code)
        ));
    }
    Ok(description.transaction_id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sign_body;

    fn request() -> UnsignedRequest {
        build_request("0.0.1001", "0.0.1002", 150_000_000, "hi").unwrap()
    }

    #[test]
    fn request_describes_the_transfer() {
        let r = request();
        assert!(r.payload.starts_with(REQUEST_PREFIX));
        let d = &r.description;
        assert_eq!(d.payer, "0.0.1001");
        assert_eq!(
            d.action,
            Action::Transfer {
                receiver: "0.0.1002".to_string(),
                tinybars: 150_000_000
            }
        );
        assert_eq!(d.expires_at_seconds - d.valid_start_seconds, VALID_SECONDS);
        assert!(seconds_remaining(d).unwrap() > 150);
        assert_eq!(decode_request(&r.payload).unwrap().1, r.description);
    }

    #[test]
    fn round_trip_both_key_types() {
        let r = request();
        for (kt, secret) in [(KeyType::Ed25519, [7u8; 32]), (KeyType::Ecdsa, [9u8; 32])] {
            let sig = sign_request(&r.payload, kt, &secret).unwrap();
            let v = assemble(&r.payload, &sig).unwrap();
            assert_eq!(v.key_type, kt);
            let expected = wallet_core::key_import::public_key_hex(kt, &secret).unwrap();
            assert_eq!(v.public_key_hex, expected.to_lowercase());
            let direct = sign_body(&decode_request(&r.payload).unwrap().0, kt, &secret).unwrap();
            if kt == KeyType::Ed25519 {
                assert_eq!(v.transaction.encode_to_vec(), direct);
            }
        }
    }

    #[test]
    fn key_update_round_trip_with_two_signatures() {
        let old_secret = [7u8; 32];
        let new_secret = [8u8; 32];
        let new_public =
            wallet_core::key_import::public_key_bytes(KeyType::Ed25519, &new_secret).unwrap();
        let r = build_key_update_request("0.0.1001", KeyType::Ed25519, &new_public).unwrap();
        let new_hex: String = new_public.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(r.description.payer, "0.0.1001");
        assert_eq!(
            r.description.action,
            Action::KeyUpdate {
                key_type: KeyType::Ed25519,
                key_hex: new_hex.clone(),
            }
        );
        assert_eq!(decode_request(&r.payload).unwrap().1, r.description);

        // Two distinct signatures are collected; a repeat is kept once.
        let sig_old = sign_request(&r.payload, KeyType::Ed25519, &old_secret).unwrap();
        let sig_new = sign_request(&r.payload, KeyType::Ed25519, &new_secret).unwrap();
        let collected = collect_signatures(
            &r.payload,
            &[sig_old.clone(), sig_new.clone(), sig_old.clone()],
        )
        .unwrap();
        assert_eq!(collected.signer_keys.len(), 2);
        assert!(collected.signer_keys.contains(&new_hex));

        // The new key can also sign locally, and never twice.
        let mut partial = collect_signatures(&r.payload, &[sig_old]).unwrap();
        partial.sign_locally(KeyType::Ed25519, &new_secret).unwrap();
        partial.sign_locally(KeyType::Ed25519, &new_secret).unwrap();
        assert_eq!(partial.signer_keys.len(), 2);

        // A signature for a different request is refused.
        let other = build_key_update_request("0.0.1002", KeyType::Ed25519, &new_public).unwrap();
        assert!(collect_signatures(&other.payload, &[sig_new]).is_err());
    }

    #[test]
    fn collected_signatures_are_checked_against_a_key_tree() {
        use crate::key_tree::KeyTree;
        let secrets = [[1u8; 32], [2u8; 32], [3u8; 32]];
        let keys = secrets
            .iter()
            .map(|secret| {
                let hex =
                    wallet_core::key_import::public_key_hex(KeyType::Ed25519, secret).unwrap();
                KeyTree::single(KeyType::Ed25519, &hex).unwrap()
            })
            .collect::<Vec<_>>();
        let tree = KeyTree::Threshold {
            required: 2,
            keys: keys.clone(),
        };

        let r = request();
        let sign = |secret| sign_request(&r.payload, KeyType::Ed25519, secret).unwrap();

        // One signature of three: verified, but below the threshold.
        let one = collect_signatures(&r.payload, &[sign(&secrets[0])]).unwrap();
        assert!(!one.satisfies(&tree));
        assert_eq!(one.missing_keys(&tree).len(), 2);

        // The same signature twice is kept once and stays below the threshold.
        let dup = collect_signatures(&r.payload, &[sign(&secrets[0]), sign(&secrets[0])]).unwrap();
        assert!(!dup.satisfies(&tree));

        // A verified signature from a key outside the structure never counts.
        let outsider = [9u8; 32];
        let unrelated =
            collect_signatures(&r.payload, &[sign(&secrets[0]), sign(&outsider)]).unwrap();
        assert!(!unrelated.satisfies(&tree));
        let outsider_hex = wallet_core::key_import::public_key_hex(KeyType::Ed25519, &outsider)
            .unwrap()
            .to_lowercase();
        assert!(!tree.contains_key(&outsider_hex));

        // Any two of the three listed keys satisfy the threshold.
        let two = collect_signatures(&r.payload, &[sign(&secrets[1]), sign(&secrets[2])]).unwrap();
        assert!(two.satisfies(&tree));
        assert_eq!(two.missing_keys(&tree).len(), 1);
    }

    #[test]
    fn rejects_signature_for_another_request() {
        let a = request();
        let b = build_request("0.0.1001", "0.0.1003", 1, "").unwrap();
        let sig = sign_request(&b.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        assert!(assemble(&a.payload, &sig).is_err());
    }

    #[test]
    fn rejects_garbage_and_tampering() {
        let r = request();
        assert!(decode_request("hello").is_err());
        assert!(decode_request(&format!("{REQUEST_PREFIX}ZZ")).is_err());
        let sig = sign_request(&r.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        assert!(assemble(&r.payload, "HVTX1-SIG:00").is_err());
        let mut flipped = sig.clone();
        let last = flipped.pop().unwrap();
        flipped.push(if last == '0' { '1' } else { '0' });
        assert!(assemble(&r.payload, &flipped).is_err());
    }

    #[test]
    fn rejects_hidden_or_foreign_bodies() {
        let r = request();
        let (body, _) = decode_request(&r.payload).unwrap();
        // Trailing unknown field (tag 99, varint 1) is not allowed.
        let mut extra = body.clone();
        extra.extend_from_slice(&[0x98, 0x06, 0x01]);
        assert!(describe_body(&extra).is_err());
        // Wrong direction: credit smaller than debit.
        let mut decoded = TransactionBody::decode(body.as_slice()).unwrap();
        decoded
            .crypto_transfer
            .as_mut()
            .unwrap()
            .transfers
            .as_mut()
            .unwrap()
            .amounts[1]
            .amount = 5;
        assert!(describe_body(&decoded.encode_to_vec()).is_err());
        // Wrong node.
        let mut decoded = TransactionBody::decode(body.as_slice()).unwrap();
        decoded.node_account.as_mut().unwrap().num = 4;
        assert!(describe_body(&decoded.encode_to_vec()).is_err());
        // Validity longer than the network allows.
        let mut decoded = TransactionBody::decode(body.as_slice()).unwrap();
        decoded.valid_duration.as_mut().unwrap().seconds = 600;
        assert!(describe_body(&decoded.encode_to_vec()).is_err());
    }

    #[test]
    fn expired_request_is_not_submitted() {
        let r = request();
        let sig = sign_request(&r.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        let mut v = assemble(&r.payload, &sig).unwrap();
        v.description.expires_at_seconds = 1;
        let rt = tokio_free_block(submit_verified(&v));
        assert!(rt.unwrap_err().contains("expired"));
    }

    // The expiry check returns before any await point that needs a runtime.
    fn tokio_free_block<F: std::future::Future>(f: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut f = Box::pin(f);
        let mut cx = Context::from_waker(Waker::noop());
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("unexpected pending"),
        }
    }

    #[test]
    fn delete_request_names_both_accounts_and_is_signed_by_one_key() {
        let r = build_delete_request("0.0.1001", "0.0.1002").unwrap();
        assert_eq!(r.description.payer, "0.0.1002");
        assert_eq!(
            r.description.action,
            Action::DeleteAccount {
                account: "0.0.1001".to_string(),
                recipient: "0.0.1002".to_string()
            }
        );
        assert!(build_delete_request("0.0.1001", "0.0.1001").is_err());
        let sig = sign_request(&r.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        assert!(assemble(&r.payload, &sig).is_ok());
    }

    #[test]
    fn create_request_shows_the_new_key_and_balance() {
        let key = wallet_core::key_import::public_key_bytes(KeyType::Ecdsa, &[9u8; 32]).unwrap();
        let r = build_create_request("0.0.1001", KeyType::Ecdsa, &key, 250_000_000, "n").unwrap();
        assert_eq!(r.description.payer, "0.0.1001");
        assert_eq!(r.description.memo, "n");
        assert_eq!(
            r.description.action,
            Action::CreateAccount {
                key_type: KeyType::Ecdsa,
                key_hex: key.iter().map(|b| format!("{b:02x}")).collect(),
                initial_tinybars: 250_000_000
            }
        );
        assert!(build_create_request("0.0.1001", KeyType::Ecdsa, &key[1..], 1, "").is_err());
        assert!(build_create_request("0.0.1001", KeyType::Ecdsa, &key, -1, "").is_err());
        assert!(build_create_request("0.0.1001", KeyType::Ecdsa, &key, i64::MAX, "").is_err());
    }

    #[test]
    fn rejects_bodies_that_mix_actions() {
        let r = build_delete_request("0.0.1001", "0.0.1002").unwrap();
        let (body, _) = decode_request(&r.payload).unwrap();
        let mut mixed = TransactionBody::decode(body.as_slice()).unwrap();
        mixed.crypto_transfer = request_body().crypto_transfer;
        assert!(describe_body(&mixed.encode_to_vec()).is_err());
        let mut stranger = TransactionBody::decode(body.as_slice()).unwrap();
        stranger
            .transaction_id
            .as_mut()
            .unwrap()
            .account
            .as_mut()
            .unwrap()
            .num = 5;
        assert!(describe_body(&stranger.encode_to_vec()).is_err());
    }

    #[test]
    fn token_requests_describe_and_sign() {
        let r =
            build_token_transfer_request("0.0.1001", "0.0.1002", "0.0.5005", 250, 2, "t").unwrap();
        assert_eq!(r.description.payer, "0.0.1001");
        assert_eq!(r.description.memo, "t");
        assert_eq!(
            r.description.action,
            Action::TokenTransfer {
                token: "0.0.5005".to_string(),
                receiver: "0.0.1002".to_string(),
                amount: 250,
                decimals: 2
            }
        );
        assert_eq!(
            r.description.expires_at_seconds - r.description.valid_start_seconds,
            VALID_SECONDS
        );
        let sig = sign_request(&r.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        assert!(assemble(&r.payload, &sig).is_ok());

        let a = build_token_association_request("0.0.1001", "0.0.5005", true).unwrap();
        assert_eq!(
            a.description.action,
            Action::TokenAssociate {
                token: "0.0.5005".to_string()
            }
        );
        let d = build_token_association_request("0.0.1001", "0.0.5005", false).unwrap();
        assert_eq!(
            d.description.action,
            Action::TokenDissociate {
                token: "0.0.5005".to_string()
            }
        );
        assert!(build_token_transfer_request("0.0.1001", "0.0.1001", "0.0.5", 1, 0, "").is_err());
        assert!(build_token_transfer_request("0.0.1001", "0.0.1002", "0.0.5", 0, 0, "").is_err());
    }

    #[test]
    fn rejects_tampered_token_bodies() {
        let r =
            build_token_transfer_request("0.0.1001", "0.0.1002", "0.0.5005", 10, 0, "").unwrap();
        let (body, _) = decode_request(&r.payload).unwrap();
        // An HBAR transfer hidden alongside the token transfer.
        let mut mixed = TransactionBody::decode(body.as_slice()).unwrap();
        mixed.crypto_transfer.as_mut().unwrap().transfers =
            request_body().crypto_transfer.unwrap().transfers;
        assert!(describe_body(&mixed.encode_to_vec()).is_err());
        // A second token list.
        let mut two = TransactionBody::decode(body.as_slice()).unwrap();
        let list = two.crypto_transfer.as_ref().unwrap().token_transfers[0].clone();
        two.crypto_transfer
            .as_mut()
            .unwrap()
            .token_transfers
            .push(list);
        assert!(describe_body(&two.encode_to_vec()).is_err());
        // Association of another account.
        let a = build_token_association_request("0.0.1001", "0.0.5005", true).unwrap();
        let (abody, _) = decode_request(&a.payload).unwrap();
        let mut other = TransactionBody::decode(abody.as_slice()).unwrap();
        other
            .token_association
            .as_mut()
            .unwrap()
            .account
            .as_mut()
            .unwrap()
            .num = 9;
        assert!(describe_body(&other.encode_to_vec()).is_err());
        // Two tokens at once.
        let mut many = TransactionBody::decode(abody.as_slice()).unwrap();
        let first = many.token_association.as_ref().unwrap().tokens[0].clone();
        many.token_association.as_mut().unwrap().tokens.push(first);
        assert!(describe_body(&many.encode_to_vec()).is_err());
    }

    fn request_body() -> TransactionBody {
        TransactionBody::decode(decode_request(&request().payload).unwrap().0.as_slice()).unwrap()
    }

    #[test]
    fn nft_requests_describe_and_sign() {
        let r = build_nft_transfer_request("0.0.1001", "0.0.1002", "0.0.5005", 7, "art").unwrap();
        assert_eq!(r.description.payer, "0.0.1001");
        assert_eq!(r.description.memo, "art");
        assert_eq!(
            r.description.action,
            Action::NftTransfer {
                token: "0.0.5005".to_string(),
                serial_number: 7,
                receiver: "0.0.1002".to_string(),
            }
        );
        assert_eq!(
            r.description.expires_at_seconds - r.description.valid_start_seconds,
            VALID_SECONDS
        );
        assert_eq!(decode_request(&r.payload).unwrap().1, r.description);
        let sig = sign_request(&r.payload, KeyType::Ed25519, &[7u8; 32]).unwrap();
        assert!(assemble(&r.payload, &sig).is_ok());
        assert!(build_nft_transfer_request("0.0.1001", "0.0.1001", "0.0.5", 1, "").is_err());
        assert!(build_nft_transfer_request("0.0.1001", "0.0.1002", "0.0.5", 0, "").is_err());
        assert!(build_nft_transfer_request("0.0.1001", "0.0.1002", "0.0.5", -3, "").is_err());
        assert!(build_nft_transfer_request("0.0.1001", "0.0.1002", "1.2.5", 1, "").is_err());
    }

    #[test]
    fn rejects_tampered_nft_bodies() {
        let r = build_nft_transfer_request("0.0.1001", "0.0.1002", "0.0.5005", 7, "").unwrap();
        let (body, _) = decode_request(&r.payload).unwrap();
        let edit = |change: fn(&mut TransactionBody)| {
            let mut decoded = TransactionBody::decode(body.as_slice()).unwrap();
            change(&mut decoded);
            decoded.encode_to_vec()
        };
        // Fungible amounts hidden alongside the NFT transfer.
        let with_amounts = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().token_transfers[0].transfers =
                vec![crate::AccountAmount {
                    account: Some(crate::AccountId {
                        shard: 0,
                        realm: 0,
                        num: 1001,
                    }),
                    amount: -1,
                }]
        });
        assert!(describe_body(&with_amounts).is_err());
        // A second serial number.
        let two_serials = edit(|b| {
            let list = &mut b.crypto_transfer.as_mut().unwrap().token_transfers[0];
            let extra = list.nft_transfers[0].clone();
            list.nft_transfers.push(extra);
        });
        assert!(describe_body(&two_serials).is_err());
        // A sender other than the payer.
        let other_sender = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().token_transfers[0].nft_transfers[0]
                .sender
                .as_mut()
                .unwrap()
                .num = 9
        });
        assert!(describe_body(&other_sender).is_err());
        // The payer sending to itself.
        let to_self = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().token_transfers[0].nft_transfers[0]
                .receiver
                .as_mut()
                .unwrap()
                .num = 1001
        });
        assert!(describe_body(&to_self).is_err());
        // A zero serial number.
        let zero_serial = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().token_transfers[0].nft_transfers[0].serial_number =
                0
        });
        assert!(describe_body(&zero_serial).is_err());
        // Decimals do not belong on an NFT transfer.
        let with_decimals = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().token_transfers[0].expected_decimals =
                Some(crate::UInt32Value { value: 2 })
        });
        assert!(describe_body(&with_decimals).is_err());
        // An HBAR transfer hidden alongside the NFT transfer.
        let with_hbar = edit(|b| {
            b.crypto_transfer.as_mut().unwrap().transfers = Some(crate::TransferList {
                amounts: Vec::new(),
            })
        });
        assert!(describe_body(&with_hbar).is_err());
    }
}

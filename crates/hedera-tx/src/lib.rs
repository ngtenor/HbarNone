//! Minimal hand-written Hedera protobuf layer: build and sign an HBAR transfer.
//!
//! Field numbers follow hashgraph/hedera-protobufs (`services/*.proto`).

use ed25519_dalek::Signer;
use k256::ecdsa::SigningKey as EcdsaKey;
use k256::ecdsa::signature::hazmat::PrehashSigner;
use prost::Message;
use sha3::{Digest, Keccak256};
use wallet_core::key_import::{KeyType, public_key_bytes};

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
        valid_duration: Some(Duration { seconds: 120 }),
        memo: p.memo.clone(),
        crypto_create: None,
        crypto_delete: None,
        crypto_update: None,
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
const AUTO_RENEW_SECONDS: i64 = 7_776_000;

/// Serialised `TransactionBody` for creating an account controlled by `public_key`.
pub fn create_account_body_bytes(p: &CreateAccountParams) -> Result<Vec<u8>, String> {
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
        valid_duration: Some(Duration { seconds: 120 }),
        memo: String::new(),
        crypto_create: None,
        crypto_transfer: None,
        crypto_update: None,
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
        crypto_update: Some(CryptoUpdateBody {
            account_to_update: Some(account(p.account)),
            key: Some(key_message(p.new_key_type, &p.new_public_key)?),
        }),
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

fn signature_pair(
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
            crypto_transfer: Some(CryptoTransferBody { transfers: None }),
            ..Default::default()
        };
        assert_eq!(body.encode_to_vec(), vec![0x72, 0x00]);
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

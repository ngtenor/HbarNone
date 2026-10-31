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
    #[prost(message, optional, tag = "14")]
    pub crypto_transfer: Option<CryptoTransferBody>,
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

/// Signs `body_bytes` and returns the serialised `Transaction` ready to submit.
/// Ed25519 signs the bytes directly; ECDSA signs their Keccak-256 hash (r || s).
pub fn sign_body(
    body_bytes: &[u8],
    key_type: KeyType,
    secret: &[u8; 32],
) -> Result<Vec<u8>, String> {
    let public = public_key_bytes(key_type, secret)?;
    let pair = match key_type {
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
    };
    let signed = SignedTransaction {
        body_bytes: body_bytes.to_vec(),
        sig_map: Some(SignatureMap { pairs: vec![pair] }),
    };
    Ok(Transaction {
        signed_transaction_bytes: signed.encode_to_vec(),
    }
    .encode_to_vec())
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

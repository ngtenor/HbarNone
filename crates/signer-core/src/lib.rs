//! Offline signer core.
//!
//! Reads a `HVTX1-REQ:` request, decodes the transaction body itself, and
//! signs only if the body is exactly an HBAR transfer it can show in full.
//! Anything else (an unknown field, another transaction type, extra transfer
//! lines, a long validity window) is rejected. It never touches the network.

use ed25519_dalek::Signer;
use k256::ecdsa::SigningKey as EcdsaKey;
use k256::ecdsa::signature::hazmat::PrehashSigner;
use prost::Message;
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

pub const REQUEST_PREFIX: &str = "HVTX1-REQ:";
pub const RESPONSE_PREFIX: &str = "HVTX1-SIG:";
/// Longest validity window the signer accepts (Hedera's own maximum).
pub const MAX_VALID_SECONDS: i64 = 180;
/// Highest fee limit the signer accepts: 1 HBAR.
pub const MAX_FEE_TINYBARS: u64 = 100_000_000;
pub const MAX_MEMO_BYTES: usize = 100;
/// The only node the app targets (Testnet 0.0.3).
const NODE_NUM: i64 = 3;
/// A request larger than this is not a plain transfer.
const MAX_REQUEST_HEX_CHARS: usize = 2_000;

#[derive(Clone, PartialEq, Message)]
struct AccountId {
    #[prost(int64, tag = "1")]
    shard: i64,
    #[prost(int64, tag = "2")]
    realm: i64,
    #[prost(int64, tag = "3")]
    num: i64,
}

#[derive(Clone, PartialEq, Message)]
struct Timestamp {
    #[prost(int64, tag = "1")]
    seconds: i64,
    #[prost(int32, tag = "2")]
    nanos: i32,
}

#[derive(Clone, PartialEq, Message)]
struct Duration {
    #[prost(int64, tag = "1")]
    seconds: i64,
}

#[derive(Clone, PartialEq, Message)]
struct TransactionId {
    #[prost(message, optional, tag = "1")]
    valid_start: Option<Timestamp>,
    #[prost(message, optional, tag = "2")]
    account: Option<AccountId>,
}

#[derive(Clone, PartialEq, Message)]
struct AccountAmount {
    #[prost(message, optional, tag = "1")]
    account: Option<AccountId>,
    #[prost(sint64, tag = "2")]
    amount: i64,
}

#[derive(Clone, PartialEq, Message)]
struct TransferList {
    #[prost(message, repeated, tag = "1")]
    amounts: Vec<AccountAmount>,
}

#[derive(Clone, PartialEq, Message)]
struct CryptoTransferBody {
    #[prost(message, optional, tag = "1")]
    transfers: Option<TransferList>,
}

/// Only the fields of a plain transfer. Any other field is dropped on decode,
/// so re-encoding no longer matches the input and the request is rejected.
#[derive(Clone, PartialEq, Message)]
struct TransactionBody {
    #[prost(message, optional, tag = "1")]
    transaction_id: Option<TransactionId>,
    #[prost(message, optional, tag = "2")]
    node_account: Option<AccountId>,
    #[prost(uint64, tag = "3")]
    fee: u64,
    #[prost(message, optional, tag = "4")]
    valid_duration: Option<Duration>,
    #[prost(string, tag = "6")]
    memo: String,
    #[prost(message, optional, tag = "14")]
    crypto_transfer: Option<CryptoTransferBody>,
}

/// Every field the signer shows before signing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSummary {
    pub payer: String,
    pub recipient: String,
    pub tinybars: i64,
    pub memo: String,
    pub node: String,
    pub fee_limit_tinybars: u64,
    pub valid_start_seconds: i64,
    pub expires_at_seconds: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reject {
    NotARequest,
    BadEncoding,
    NotAPlainTransfer(&'static str),
    Expired,
    NotYetValid,
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reject::NotARequest => write!(f, "This is not a HederaVault signing request."),
            Reject::BadEncoding => write!(f, "The request text is damaged or incomplete."),
            Reject::NotAPlainTransfer(why) => {
                write!(f, "Refusing to sign: not a plain HBAR transfer ({why}).")
            }
            Reject::Expired => write!(f, "This request has expired. Ask for a new one."),
            Reject::NotYetValid => write!(f, "This request starts in the future. Check the clock."),
        }
    }
}

impl std::error::Error for Reject {}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn id_text(id: &AccountId) -> String {
    format!("{}.{}.{}", id.shard, id.realm, id.num)
}

/// Decodes and strictly validates the body. Returns the body bytes to sign
/// and what the user must be shown.
pub fn parse_request(request: &str) -> Result<(Vec<u8>, TransferSummary), Reject> {
    let compact: String = request.split_whitespace().collect();
    let hex = compact
        .strip_prefix(REQUEST_PREFIX)
        .ok_or(Reject::NotARequest)?;
    if hex.len() > MAX_REQUEST_HEX_CHARS {
        return Err(Reject::NotAPlainTransfer("too large"));
    }
    let bytes = from_hex(hex).ok_or(Reject::BadEncoding)?;
    let summary = summarise(&bytes)?;
    Ok((bytes, summary))
}

fn summarise(bytes: &[u8]) -> Result<TransferSummary, Reject> {
    use Reject::NotAPlainTransfer as No;
    let body = TransactionBody::decode(bytes).map_err(|_| Reject::BadEncoding)?;
    if body.encode_to_vec() != bytes {
        return Err(No("unexpected or unordered fields"));
    }
    let id = body
        .transaction_id
        .as_ref()
        .ok_or(No("no transaction id"))?;
    let start = id.valid_start.as_ref().ok_or(No("no start time"))?;
    let payer = id.account.as_ref().ok_or(No("no payer"))?;
    let node = body.node_account.as_ref().ok_or(No("no node"))?;
    if (node.shard, node.realm, node.num) != (0, 0, NODE_NUM) {
        return Err(No("unexpected node"));
    }
    let valid = body
        .valid_duration
        .as_ref()
        .ok_or(No("no validity window"))?
        .seconds;
    if !(1..=MAX_VALID_SECONDS).contains(&valid) {
        return Err(No("validity window out of range"));
    }
    if body.fee > MAX_FEE_TINYBARS {
        return Err(No("fee limit too high"));
    }
    if body.memo.len() > MAX_MEMO_BYTES || body.memo.chars().any(char::is_control) {
        return Err(No("memo is too long or has control characters"));
    }
    if !(0..1_000_000_000).contains(&start.nanos) {
        return Err(No("bad start time"));
    }
    let list = body
        .crypto_transfer
        .as_ref()
        .and_then(|t| t.transfers.as_ref())
        .ok_or(No("no transfer"))?;
    let [debit, credit] = list.amounts.as_slice() else {
        return Err(No("must be exactly one payer and one recipient"));
    };
    let from = debit.account.as_ref().ok_or(No("missing account"))?;
    let to = credit.account.as_ref().ok_or(No("missing account"))?;
    let on_main = |a: &AccountId| a.shard == 0 && a.realm == 0 && a.num > 0;
    if from != payer
        || to == payer
        || !on_main(from)
        || !on_main(to)
        || debit.amount >= 0
        || debit.amount.checked_neg() != Some(credit.amount)
    {
        return Err(No("amounts do not form a simple payment"));
    }
    Ok(TransferSummary {
        payer: id_text(payer),
        recipient: id_text(to),
        tinybars: credit.amount,
        memo: body.memo.clone(),
        node: id_text(node),
        fee_limit_tinybars: body.fee,
        valid_start_seconds: start.seconds,
        expires_at_seconds: start.seconds.saturating_add(valid),
    })
}

/// Fails if the request is not valid at `now_seconds` (Unix time).
pub fn check_time(summary: &TransferSummary, now_seconds: i64) -> Result<(), Reject> {
    if now_seconds >= summary.expires_at_seconds {
        return Err(Reject::Expired);
    }
    // The network refuses a start time in the future; allow a little clock skew.
    if summary.valid_start_seconds > now_seconds + 30 {
        return Err(Reject::NotYetValid);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Ed25519,
    Ecdsa,
}

/// A secret key that can only sign. It has no `Debug`, `Display` or accessor.
pub struct SigningKey {
    kind: KeyKind,
    secret: Zeroizing<[u8; 32]>,
}

impl SigningKey {
    pub fn new(kind: KeyKind, secret: [u8; 32]) -> Result<Self, String> {
        let key = Self {
            kind,
            secret: Zeroizing::new(secret),
        };
        key.public_key()?;
        Ok(key)
    }

    pub fn kind(&self) -> KeyKind {
        self.kind
    }

    /// Raw Ed25519 key (32 bytes) or compressed secp256k1 key (33 bytes).
    pub fn public_key(&self) -> Result<Vec<u8>, String> {
        match self.kind {
            KeyKind::Ed25519 => Ok(ed25519_dalek::SigningKey::from_bytes(&self.secret)
                .verifying_key()
                .to_bytes()
                .to_vec()),
            KeyKind::Ecdsa => {
                let key = EcdsaKey::from_slice(self.secret.as_slice())
                    .map_err(|_| "not a valid secp256k1 private key".to_string())?;
                Ok(key
                    .verifying_key()
                    .to_encoded_point(true)
                    .as_bytes()
                    .to_vec())
            }
        }
    }

    pub fn public_key_hex(&self) -> Result<String, String> {
        Ok(self
            .public_key()?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }

    fn sign_body(&self, body: &[u8]) -> Result<Vec<u8>, String> {
        match self.kind {
            KeyKind::Ed25519 => Ok(ed25519_dalek::SigningKey::from_bytes(&self.secret)
                .sign(body)
                .to_bytes()
                .to_vec()),
            KeyKind::Ecdsa => {
                let key =
                    EcdsaKey::from_slice(self.secret.as_slice()).map_err(|e| e.to_string())?;
                let signature: k256::ecdsa::Signature = key
                    .sign_prehash(&Keccak256::digest(body))
                    .map_err(|e| e.to_string())?;
                Ok(signature.to_bytes().to_vec())
            }
        }
    }
}

/// Signs a request the user has approved and returns the `HVTX1-SIG:` reply.
/// The request is re-parsed here, so a caller cannot sign bytes it did not show.
pub fn sign(request: &str, key: &SigningKey, now_seconds: i64) -> Result<String, String> {
    let (body, summary) = parse_request(request).map_err(|e| e.to_string())?;
    check_time(&summary, now_seconds).map_err(|e| e.to_string())?;
    let tag = match key.kind {
        KeyKind::Ed25519 => 0u8,
        KeyKind::Ecdsa => 1u8,
    };
    let mut raw = vec![tag];
    raw.extend_from_slice(&key.public_key()?);
    raw.extend_from_slice(&key.sign_body(&body)?);
    Ok(format!("{RESPONSE_PREFIX}{}", to_hex(&raw)))
}

/// Formats tinybars as `1.50000000`.
pub fn format_hbar(tinybars: i64) -> String {
    format!("{}.{:08}", tinybars / 100_000_000, tinybars % 100_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hedera_tx::offline::{assemble, build_request};

    const NOW: i64 = 1_700_000_000;

    fn request(memo: &str) -> String {
        build_request("0.0.1001", "0.0.1002", 150_000_000, memo)
            .unwrap()
            .payload
    }

    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn with_body(edit: impl FnOnce(&mut TransactionBody)) -> String {
        let (bytes, _) = parse_request(&request("hi")).unwrap();
        let mut body = TransactionBody::decode(bytes.as_slice()).unwrap();
        edit(&mut body);
        format!("{REQUEST_PREFIX}{}", to_hex(&body.encode_to_vec()))
    }

    fn lines(body: &mut TransactionBody) -> &mut Vec<AccountAmount> {
        &mut body
            .crypto_transfer
            .as_mut()
            .unwrap()
            .transfers
            .as_mut()
            .unwrap()
            .amounts
    }

    #[test]
    fn shows_every_field_of_an_app_request() {
        let (_, s) = parse_request(&request("lunch")).unwrap();
        assert_eq!(
            (s.payer.as_str(), s.recipient.as_str()),
            ("0.0.1001", "0.0.1002")
        );
        assert_eq!(s.tinybars, 150_000_000);
        assert_eq!(s.memo, "lunch");
        assert_eq!(s.node, "0.0.3");
        assert_eq!(s.fee_limit_tinybars, MAX_FEE_TINYBARS);
        assert_eq!(s.expires_at_seconds - s.valid_start_seconds, 180);
        assert_eq!(format_hbar(s.tinybars), "1.50000000");
    }

    #[test]
    fn signatures_are_accepted_by_the_app_for_both_key_types() {
        let req = request("");
        for (kind, secret) in [(KeyKind::Ed25519, [7u8; 32]), (KeyKind::Ecdsa, [9u8; 32])] {
            let key = SigningKey::new(kind, secret).unwrap();
            let reply = sign(&req, &key, now()).unwrap();
            let verified = assemble(&req, &reply).unwrap();
            assert_eq!(verified.public_key_hex, key.public_key_hex().unwrap());
        }
    }

    #[test]
    fn rejects_other_transaction_types_and_hidden_fields() {
        // A key update smuggled next to a transfer (field 15, empty).
        let mut extra = parse_request(&request("")).unwrap().0;
        extra.extend_from_slice(&[0x7A, 0x00]);
        assert!(summarise(&extra).is_err());
        // Unknown trailing field.
        let mut unknown = parse_request(&request("")).unwrap().0;
        unknown.extend_from_slice(&[0x98, 0x06, 0x01]);
        assert!(summarise(&unknown).is_err());
        // Three transfer lines.
        let three = with_body(|b| {
            lines(b).push(AccountAmount {
                account: None,
                amount: 0,
            })
        });
        assert!(parse_request(&three).is_err());
    }

    #[test]
    fn rejects_unsafe_values() {
        let wrong_way = with_body(|b| {
            lines(b)[0].amount = 100;
            lines(b)[1].amount = -100;
        });
        let other_payer = with_body(|b| lines(b)[0].account.as_mut().unwrap().num = 5);
        let long_window = with_body(|b| b.valid_duration.as_mut().unwrap().seconds = 181);
        let high_fee = with_body(|b| b.fee = MAX_FEE_TINYBARS + 1);
        let other_node = with_body(|b| b.node_account.as_mut().unwrap().num = 4);
        let escape_memo = with_body(|b| b.memo = "a\u{1b}[2Jb".to_string());
        let other_shard = with_body(|b| lines(b)[1].account.as_mut().unwrap().shard = 1);
        for bad in [
            wrong_way,
            other_payer,
            long_window,
            high_fee,
            other_node,
            escape_memo,
            other_shard,
        ] {
            assert!(parse_request(&bad).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn rejects_garbage_input() {
        for text in [
            "",
            "hello",
            "HVTX1-REQ:",
            "HVTX1-REQ:ZZ",
            "HVTX1-REQ:0A",
            "HVTX1-SIG:00",
        ] {
            assert!(parse_request(text).is_err(), "{text}");
        }
        let long = format!("{REQUEST_PREFIX}{}", "00".repeat(2_000));
        assert!(parse_request(&long).is_err());
    }

    #[test]
    fn tolerates_whitespace_from_copy_and_paste() {
        let req = request("x");
        let wrapped: String = req
            .as_bytes()
            .chunks(40)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect::<Vec<_>>()
            .join("\r\n  ");
        assert!(parse_request(&wrapped).is_ok());
    }

    #[test]
    fn refuses_expired_and_future_requests() {
        let (_, s) = parse_request(&request("")).unwrap();
        assert_eq!(check_time(&s, s.valid_start_seconds + 20), Ok(()));
        assert_eq!(check_time(&s, s.expires_at_seconds), Err(Reject::Expired));
        assert_eq!(
            check_time(&s, s.valid_start_seconds - 60),
            Err(Reject::NotYetValid)
        );
        let key = SigningKey::new(KeyKind::Ed25519, [7u8; 32]).unwrap();
        assert!(sign(&request(""), &key, NOW).is_err());
    }

    #[test]
    fn rejects_an_invalid_ecdsa_secret() {
        assert!(SigningKey::new(KeyKind::Ecdsa, [0u8; 32]).is_err());
    }

    #[test]
    fn survives_mutated_and_truncated_input() {
        let bytes = parse_request(&request("fuzz")).unwrap().0;
        for i in 0..bytes.len() {
            let _ = summarise(&bytes[..i]);
            let mut copy = bytes.clone();
            for flip in [0x01u8, 0x80, 0xFF] {
                copy[i] ^= flip;
                let _ = summarise(&copy);
                copy[i] ^= flip;
            }
        }
    }
}

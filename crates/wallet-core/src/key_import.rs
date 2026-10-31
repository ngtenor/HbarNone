use ed25519_dalek::SigningKey;
use k256::SecretKey;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyType {
    Ed25519,
    Ecdsa,
}

/// DER prefixes (PKCS#8) of the 32-byte private keys that Hedera tools export.
const ED25519_DER_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];
const ECDSA_DER_PREFIX: [u8; 18] = [
    0x30, 0x30, 0x02, 0x01, 0x00, 0x30, 0x07, 0x06, 0x05, 0x2b, 0x81, 0x04, 0x00, 0x0a, 0x04, 0x22,
    0x04, 0x20,
];

/// A parsed private key. `key_type` is `None` for a bare 32-byte hex key,
/// which carries no marker saying which curve it belongs to.
pub struct ParsedPrivateKey {
    pub secret: Zeroizing<[u8; 32]>,
    pub key_type: Option<KeyType>,
}

pub fn parse_private_key(input: &str) -> Result<ParsedPrivateKey, String> {
    let cleaned: String = input.split_whitespace().collect();
    let hex_text = cleaned
        .strip_prefix("0x")
        .or_else(|| cleaned.strip_prefix("0X"))
        .unwrap_or(&cleaned);
    if hex_text.is_empty() || hex_text.len() > 200 {
        return Err("enter a private key as hexadecimal text".to_string());
    }
    let bytes = Zeroizing::new(
        hex::decode(hex_text)
            .map_err(|_| "the private key must contain only hexadecimal characters".to_string())?,
    );

    let (key_type, secret_bytes): (Option<KeyType>, &[u8]) = match bytes.len() {
        32 => (None, &bytes[..]),
        48 if bytes.starts_with(&ED25519_DER_PREFIX) => {
            (Some(KeyType::Ed25519), &bytes[ED25519_DER_PREFIX.len()..])
        }
        50 if bytes.starts_with(&ECDSA_DER_PREFIX) => {
            (Some(KeyType::Ecdsa), &bytes[ECDSA_DER_PREFIX.len()..])
        }
        _ => {
            return Err(
                "unrecognised private key: expected 32 bytes of hex, or a DER-encoded Ed25519 or ECDSA (secp256k1) key"
                    .to_string(),
            );
        }
    };

    let mut secret = Zeroizing::new([0u8; 32]);
    secret.copy_from_slice(secret_bytes);
    if secret.iter().all(|byte| *byte == 0) {
        return Err("the private key is not valid".to_string());
    }
    if key_type == Some(KeyType::Ecdsa) && SecretKey::from_slice(&secret[..]).is_err() {
        return Err("the private key is not a valid ECDSA (secp256k1) key".to_string());
    }
    Ok(ParsedPrivateKey { secret, key_type })
}

/// Returns whether a bare 32-byte key is also a valid secp256k1 scalar.
pub fn is_valid_ecdsa_secret(secret: &[u8; 32]) -> bool {
    SecretKey::from_slice(&secret[..]).is_ok()
}

/// Raw 32-byte public key for Ed25519, or the 33-byte compressed point for ECDSA.
pub fn public_key_bytes(key_type: KeyType, secret: &[u8; 32]) -> Result<Vec<u8>, String> {
    match key_type {
        KeyType::Ed25519 => Ok(SigningKey::from_bytes(secret)
            .verifying_key()
            .to_bytes()
            .to_vec()),
        KeyType::Ecdsa => {
            let key = SecretKey::from_slice(&secret[..])
                .map_err(|_| "the private key is not a valid ECDSA (secp256k1) key".to_string())?;
            Ok(key.public_key().to_sec1_bytes().to_vec())
        }
    }
}

pub fn public_key_hex(key_type: KeyType, secret: &[u8; 32]) -> Result<String, String> {
    public_key_bytes(key_type, secret).map(hex::encode)
}

/// The Ethereum-style address of an ECDSA key: the last 20 bytes of the
/// Keccak-256 hash of the uncompressed public key (without its 0x04 prefix).
pub fn evm_address(secret: &[u8; 32]) -> Result<String, String> {
    let key = SecretKey::from_slice(&secret[..])
        .map_err(|_| "the private key is not a valid ECDSA (secp256k1) key".to_string())?;
    let point = key.public_key().to_sec1_bytes();
    let uncompressed = k256::PublicKey::from_sec1_bytes(&point)
        .map_err(|_| "could not read the public key".to_string())?;
    let encoded = uncompressed.to_encoded_point(false);
    let hash = Keccak256::digest(&encoded.as_bytes()[1..]);
    Ok(format!("0x{}", hex::encode(&hash[12..])))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Private key 1 on secp256k1: its public key and Ethereum address are
    // widely published test values.
    const ONE: &str = "0000000000000000000000000000000000000000000000000000000000000001";

    #[test]
    fn derives_the_known_secp256k1_public_key_and_evm_address() {
        let parsed = parse_private_key(ONE).unwrap();
        assert_eq!(parsed.key_type, None);
        assert_eq!(
            public_key_hex(KeyType::Ecdsa, &parsed.secret).unwrap(),
            "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        );
        assert_eq!(
            evm_address(&parsed.secret).unwrap(),
            "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf"
        );
    }

    #[test]
    fn derives_the_rfc8032_ed25519_public_key() {
        // RFC 8032 section 7.1, test 1.
        let secret = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let parsed = parse_private_key(secret).unwrap();
        assert_eq!(
            public_key_hex(KeyType::Ed25519, &parsed.secret).unwrap(),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
    }

    #[test]
    fn detects_the_key_type_of_der_encoded_keys() {
        let ecdsa =
            parse_private_key(&format!("3030020100300706052b8104000a04220420{ONE}")).unwrap();
        assert_eq!(ecdsa.key_type, Some(KeyType::Ecdsa));
        assert_eq!(hex::encode(&ecdsa.secret[..]), ONE);

        let ed = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let ed25519 = parse_private_key(&format!("302e020100300506032b657004220420{ed}")).unwrap();
        assert_eq!(ed25519.key_type, Some(KeyType::Ed25519));
        assert_eq!(hex::encode(&ed25519.secret[..]), ed);
    }

    #[test]
    fn accepts_a_0x_prefix_and_surrounding_whitespace() {
        let parsed = parse_private_key(&format!("  0x{ONE}\n")).unwrap();
        assert_eq!(hex::encode(&parsed.secret[..]), ONE);
    }

    #[test]
    fn rejects_malformed_keys() {
        assert!(parse_private_key("").is_err());
        assert!(parse_private_key("zz").is_err());
        assert!(parse_private_key("abcd").is_err());
        assert!(parse_private_key(&"00".repeat(32)).is_err());
        assert!(parse_private_key(&format!("302e020100300706052b8104000a04220420{ONE}")).is_err());
        // The secp256k1 group order is not a valid private key.
        let order = "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141";
        assert!(
            parse_private_key(&format!("3030020100300706052b8104000a04220420{order}")).is_err()
        );
        assert!(!is_valid_ecdsa_secret(
            &hex::decode(order).unwrap().try_into().unwrap()
        ));
    }
}

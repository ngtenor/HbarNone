//! Generic Hedera key structures for multisig accounts.
//!
//! A [`KeyTree`] is a single public key or an M-of-N threshold over child
//! trees; Hedera's `KeyList` is read as the N-of-N case. The tree is
//! independent of transaction type and never touches signatures: evaluation
//! takes lowercase hex public keys whose signatures were already verified
//! against the exact body bytes elsewhere (`offline::collect_signatures`).
//! Anything outside these shapes — contract keys, RSA keys, empty or
//! oversized structures — is rejected: unknown means reject.

use crate::{Key, KeyList, ThresholdKey};
use prost::Message;
use wallet_core::key_import::KeyType;

/// Most leaf keys one tree may hold (a 3-of-5 fits comfortably).
pub const MAX_TREE_KEYS: usize = 10;
/// Deepest allowed nesting: a single key is depth 0, a threshold of single
/// keys is depth 1, and one further nested level is allowed.
pub const MAX_TREE_DEPTH: usize = 2;

/// A single key or an M-of-N threshold over child trees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyTree {
    Single {
        key_type: KeyType,
        /// Lowercase hex (64 characters Ed25519, 66 characters ECDSA).
        public_key_hex: String,
    },
    /// `required` of the `keys` must sign. N-of-N is Hedera's `KeyList`.
    Threshold { required: u32, keys: Vec<KeyTree> },
}

fn hex_len(key_type: KeyType) -> usize {
    match key_type {
        KeyType::Ed25519 => 64,
        KeyType::Ecdsa => 66,
    }
}

fn type_label(key_type: KeyType) -> &'static str {
    match key_type {
        KeyType::Ed25519 => "Ed25519",
        KeyType::Ecdsa => "ECDSA",
    }
}

impl KeyTree {
    /// A validated single-key leaf; the hex is normalised to lowercase.
    pub fn single(key_type: KeyType, public_key_hex: &str) -> Result<KeyTree, String> {
        let tree = KeyTree::Single {
            key_type,
            public_key_hex: public_key_hex.trim().to_ascii_lowercase(),
        };
        tree.validate()?;
        Ok(tree)
    }

    /// Checks the whole tree is a shape this app accepts: valid hex leaves,
    /// thresholds within `1..=n`, no duplicate entries inside one list, and
    /// the size and depth caps.
    pub fn validate(&self) -> Result<(), String> {
        if self.leaf_count() > MAX_TREE_KEYS {
            return Err(format!(
                "A key structure may hold at most {MAX_TREE_KEYS} keys."
            ));
        }
        self.validate_node(0)
    }

    fn validate_node(&self, depth: usize) -> Result<(), String> {
        match self {
            KeyTree::Single {
                key_type,
                public_key_hex,
            } => {
                let expected = hex_len(*key_type);
                let valid = public_key_hex.len() == expected
                    && public_key_hex
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
                if !valid {
                    return Err(format!(
                        "A public key must be {expected} lowercase hexadecimal characters."
                    ));
                }
                Ok(())
            }
            KeyTree::Threshold { required, keys } => {
                if depth >= MAX_TREE_DEPTH {
                    return Err(format!(
                        "A key structure may be nested at most {MAX_TREE_DEPTH} levels deep."
                    ));
                }
                if keys.is_empty() {
                    return Err("A key structure must list at least one key.".into());
                }
                if *required < 1 || *required as usize > keys.len() {
                    return Err(format!(
                        "The threshold must be between 1 and {} for this structure.",
                        keys.len()
                    ));
                }
                for (index, key) in keys.iter().enumerate() {
                    if keys[..index].contains(key) {
                        return Err("A key structure must not repeat a key.".into());
                    }
                    key.validate_node(depth + 1)?;
                }
                Ok(())
            }
        }
    }

    /// The protobuf `Key` message for this tree. A threshold is always
    /// encoded as a `ThresholdKey`, including the N-of-N case.
    pub fn to_key(&self) -> Result<Key, String> {
        self.validate()?;
        Ok(self.to_key_unchecked())
    }

    /// Lowercase hex of the serialised protobuf `Key` message for this tree.
    pub fn to_hex(&self) -> Result<String, String> {
        Ok(self
            .to_key()?
            .encode_to_vec()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }

    fn to_key_unchecked(&self) -> Key {
        match self {
            KeyTree::Single {
                key_type,
                public_key_hex,
            } => {
                // Validation guarantees well-formed hex of the right length.
                let bytes: Vec<u8> = (0..public_key_hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&public_key_hex[i..i + 2], 16).unwrap_or(0))
                    .collect();
                match key_type {
                    KeyType::Ed25519 => Key {
                        ed25519: Some(bytes),
                        ..Key::default()
                    },
                    KeyType::Ecdsa => Key {
                        ecdsa_secp256k1: Some(bytes),
                        ..Key::default()
                    },
                }
            }
            KeyTree::Threshold { required, keys } => Key {
                threshold_key: Some(ThresholdKey {
                    threshold: *required,
                    keys: Some(KeyList {
                        keys: keys.iter().map(KeyTree::to_key_unchecked).collect(),
                    }),
                }),
                ..Key::default()
            },
        }
    }

    /// Reads a protobuf `Key` message, accepting single keys, threshold keys
    /// and key lists (as N-of-N) and rejecting everything else.
    pub fn from_key(key: &Key) -> Result<KeyTree, String> {
        let tree = Self::from_key_node(key)?;
        tree.validate()?;
        Ok(tree)
    }

    /// Reads serialised protobuf `Key` bytes (for example a mirror-node
    /// `ProtobufEncoded` account key).
    pub fn from_bytes(bytes: &[u8]) -> Result<KeyTree, String> {
        let key =
            Key::decode(bytes).map_err(|_| "This is not a valid key structure.".to_string())?;
        Self::from_key(&key)
    }

    /// Reads an account key as the mirror node reports it: `kind` is the
    /// mirror's `_type` and `hex` the key field. Single keys and
    /// protobuf-encoded structures are accepted; anything else is rejected.
    pub fn from_mirror(kind: &str, hex: &str) -> Result<KeyTree, String> {
        match kind {
            "ED25519" => KeyTree::single(KeyType::Ed25519, hex),
            "ECDSA_SECP256K1" => KeyTree::single(KeyType::Ecdsa, hex),
            "ProtobufEncoded" => {
                let cleaned = hex.trim().to_ascii_lowercase();
                if cleaned.is_empty() || cleaned.len() % 2 != 0 {
                    return Err("This is not a valid key structure.".into());
                }
                let bytes = (0..cleaned.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok())
                    .collect::<Option<Vec<u8>>>()
                    .ok_or("This is not a valid key structure.")?;
                Self::from_bytes(&bytes)
            }
            _ => Err("This account uses a key type the app does not support.".into()),
        }
    }

    fn from_key_node(key: &Key) -> Result<KeyTree, String> {
        let set = [
            key.ed25519.is_some(),
            key.ecdsa_secp256k1.is_some(),
            key.threshold_key.is_some(),
            key.key_list.is_some(),
        ];
        if set.iter().filter(|&&s| s).count() != 1 {
            return Err("This key structure holds a key type the app does not support.".into());
        }
        let single = |key_type: KeyType, bytes: &[u8]| KeyTree::Single {
            key_type,
            public_key_hex: bytes.iter().map(|b| format!("{b:02x}")).collect(),
        };
        if let Some(bytes) = &key.ed25519 {
            return Ok(single(KeyType::Ed25519, bytes));
        }
        if let Some(bytes) = &key.ecdsa_secp256k1 {
            return Ok(single(KeyType::Ecdsa, bytes));
        }
        let (required, list) = if let Some(threshold) = &key.threshold_key {
            let list = threshold
                .keys
                .as_ref()
                .ok_or("This key structure lists no keys.")?;
            (threshold.threshold, list)
        } else {
            let list = key.key_list.as_ref().expect("checked above");
            (list.keys.len() as u32, list)
        };
        let keys = list
            .keys
            .iter()
            .map(Self::from_key_node)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(KeyTree::Threshold { required, keys })
    }

    /// How many signatures the top level needs and how many keys it lists;
    /// a single key is (1, 1).
    pub fn required_and_total(&self) -> (u32, usize) {
        match self {
            KeyTree::Single { .. } => (1, 1),
            KeyTree::Threshold { required, keys } => (*required, keys.len()),
        }
    }

    /// Every leaf key in the tree, in display order.
    pub fn leaf_keys(&self) -> Vec<(KeyType, String)> {
        let mut leaves = Vec::new();
        self.collect_leaves(&mut leaves);
        leaves
    }

    fn collect_leaves(&self, leaves: &mut Vec<(KeyType, String)>) {
        match self {
            KeyTree::Single {
                key_type,
                public_key_hex,
            } => leaves.push((*key_type, public_key_hex.clone())),
            KeyTree::Threshold { keys, .. } => {
                for key in keys {
                    key.collect_leaves(leaves);
                }
            }
        }
    }

    fn leaf_count(&self) -> usize {
        match self {
            KeyTree::Single { .. } => 1,
            KeyTree::Threshold { keys, .. } => keys.iter().map(KeyTree::leaf_count).sum(),
        }
    }

    /// True when this exact lowercase hex public key is one of the leaves.
    pub fn contains_key(&self, public_key_hex: &str) -> bool {
        let wanted = public_key_hex.to_ascii_lowercase();
        self.leaf_keys().iter().any(|(_, hex)| *hex == wanted)
    }

    /// Generic threshold evaluation: true when the keys that have signed
    /// (lowercase hex, each verified elsewhere) satisfy this structure.
    /// Independent of transaction type.
    pub fn is_satisfied_by(&self, signer_keys: &[String]) -> bool {
        match self {
            KeyTree::Single { public_key_hex, .. } => signer_keys.contains(public_key_hex),
            KeyTree::Threshold { required, keys } => {
                let satisfied = keys
                    .iter()
                    .filter(|key| key.is_satisfied_by(signer_keys))
                    .count();
                satisfied >= *required as usize
            }
        }
    }

    /// Plain-language description, one line per node, full keys shown:
    /// `2 of 3:` followed by one indented line per key.
    pub fn describe(&self) -> String {
        let mut lines = Vec::new();
        self.describe_node(0, &mut lines);
        lines.join("\n")
    }

    fn describe_node(&self, indent: usize, lines: &mut Vec<String>) {
        let pad = "  ".repeat(indent);
        match self {
            KeyTree::Single {
                key_type,
                public_key_hex,
            } => lines.push(format!("{pad}{} {public_key_hex}", type_label(*key_type))),
            KeyTree::Threshold { required, keys } => {
                lines.push(format!("{pad}{required} of {}:", keys.len()));
                for key in keys {
                    key.describe_node(indent + 1, lines);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(n: u8) -> KeyTree {
        KeyTree::single(KeyType::Ed25519, &format!("{n:02x}").repeat(32)).unwrap()
    }

    fn two_of_three() -> KeyTree {
        KeyTree::Threshold {
            required: 2,
            keys: vec![ed(1), ed(2), ed(3)],
        }
    }

    fn hex(n: u8) -> String {
        format!("{n:02x}").repeat(32)
    }

    #[test]
    fn single_keys_validate_and_normalise() {
        let ecdsa_hex = format!("02{}", "ab".repeat(32));
        let key = KeyTree::single(KeyType::Ecdsa, &ecdsa_hex.to_uppercase()).unwrap();
        assert_eq!(
            key,
            KeyTree::Single {
                key_type: KeyType::Ecdsa,
                public_key_hex: ecdsa_hex,
            }
        );
        assert!(KeyTree::single(KeyType::Ed25519, &"ab".repeat(31)).is_err());
        assert!(KeyTree::single(KeyType::Ed25519, &"zz".repeat(32)).is_err());
        assert!(KeyTree::single(KeyType::Ecdsa, &"ab".repeat(32)).is_err());
    }

    #[test]
    fn round_trips_two_of_three_and_three_of_five() {
        for tree in [
            two_of_three(),
            KeyTree::Threshold {
                required: 3,
                keys: (1..=5).map(ed).collect(),
            },
        ] {
            let key = tree.to_key().unwrap();
            assert_eq!(KeyTree::from_key(&key).unwrap(), tree);
            assert_eq!(KeyTree::from_bytes(&key.encode_to_vec()).unwrap(), tree);
        }
    }

    #[test]
    fn key_list_reads_as_n_of_n() {
        let key = Key {
            key_list: Some(KeyList {
                keys: vec![
                    ed(1).to_key().unwrap(),
                    ed(2).to_key().unwrap(),
                    ed(3).to_key().unwrap(),
                ],
            }),
            ..Key::default()
        };
        let tree = KeyTree::from_key(&key).unwrap();
        assert_eq!(
            tree,
            KeyTree::Threshold {
                required: 3,
                keys: vec![ed(1), ed(2), ed(3)],
            }
        );
    }

    #[test]
    fn nested_structures_round_trip_within_the_depth_cap() {
        let nested = KeyTree::Threshold {
            required: 2,
            keys: vec![ed(9), two_of_three()],
        };
        let key = nested.to_key().unwrap();
        assert_eq!(KeyTree::from_key(&key).unwrap(), nested);

        let too_deep = KeyTree::Threshold {
            required: 1,
            keys: vec![KeyTree::Threshold {
                required: 1,
                keys: vec![KeyTree::Threshold {
                    required: 1,
                    keys: vec![ed(1)],
                }],
            }],
        };
        assert!(too_deep.validate().is_err());
    }

    #[test]
    fn rejects_malformed_trees() {
        let shape = |required, keys: Vec<KeyTree>| KeyTree::Threshold { required, keys };
        assert!(shape(0, vec![ed(1)]).validate().is_err());
        assert!(shape(4, vec![ed(1), ed(2), ed(3)]).validate().is_err());
        assert!(shape(1, vec![]).validate().is_err());
        assert!(shape(1, vec![ed(1), ed(1)]).validate().is_err());
        assert!(
            shape(2, (0..=(MAX_TREE_KEYS as u8)).map(ed).collect())
                .validate()
                .is_err()
        );
    }

    #[test]
    fn rejects_unsupported_or_empty_keys() {
        // An unsupported variant (for example a contract key) decodes to an
        // empty message; a key with two variants set is also not a valid key.
        assert!(KeyTree::from_key(&Key::default()).is_err());
        let both = Key {
            ed25519: Some(vec![1u8; 32]),
            ecdsa_secp256k1: Some(vec![2u8; 33]),
            ..Key::default()
        };
        assert!(KeyTree::from_key(&both).is_err());
        let empty_threshold = Key {
            threshold_key: Some(ThresholdKey {
                threshold: 1,
                keys: None,
            }),
            ..Key::default()
        };
        assert!(KeyTree::from_key(&empty_threshold).is_err());
        assert!(KeyTree::from_bytes(&[0xff, 0xff]).is_err());
    }

    #[test]
    fn evaluates_thresholds_generically() {
        let tree = two_of_three();
        assert!(!tree.is_satisfied_by(&[]));
        assert!(!tree.is_satisfied_by(&[hex(1)]));
        assert!(tree.is_satisfied_by(&[hex(1), hex(3)]));
        assert!(tree.is_satisfied_by(&[hex(1), hex(2), hex(3)]));
        // Unrelated keys never count towards the threshold.
        assert!(!tree.is_satisfied_by(&[hex(1), hex(9)]));
        assert!(tree.contains_key(&hex(2).to_uppercase()));
        assert!(!tree.contains_key(&hex(9)));
        assert_eq!(tree.required_and_total(), (2, 3));

        // Nested: the inner 2-of-3 counts as one satisfied branch.
        let nested = KeyTree::Threshold {
            required: 2,
            keys: vec![ed(9), two_of_three()],
        };
        assert!(!nested.is_satisfied_by(&[hex(9), hex(1)]));
        assert!(nested.is_satisfied_by(&[hex(9), hex(1), hex(2)]));
        assert!(!nested.is_satisfied_by(&[hex(1), hex(2)]));
    }

    #[test]
    fn reads_mirror_keys_of_each_kind() {
        let single = KeyTree::from_mirror("ED25519", &hex(1).to_uppercase()).unwrap();
        assert_eq!(single, ed(1));
        let ecdsa_hex = format!("02{}", "ab".repeat(32));
        assert_eq!(
            KeyTree::from_mirror("ECDSA_SECP256K1", &ecdsa_hex).unwrap(),
            KeyTree::single(KeyType::Ecdsa, &ecdsa_hex).unwrap()
        );
        let tree = two_of_three();
        let encoded: String = tree
            .to_key()
            .unwrap()
            .encode_to_vec()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            KeyTree::from_mirror("ProtobufEncoded", &encoded).unwrap(),
            tree
        );
        assert!(KeyTree::from_mirror("RSA_3072", &hex(1)).is_err());
        assert!(KeyTree::from_mirror("ProtobufEncoded", "zz").is_err());
        assert!(KeyTree::from_mirror("ProtobufEncoded", "abc").is_err());
        assert!(KeyTree::from_mirror("ProtobufEncoded", "").is_err());
    }

    #[test]
    fn describes_the_structure_in_full() {
        let nested = KeyTree::Threshold {
            required: 2,
            keys: vec![ed(9), two_of_three()],
        };
        let text = nested.describe();
        assert_eq!(
            text,
            format!(
                "2 of 2:\n  Ed25519 {}\n  2 of 3:\n    Ed25519 {}\n    Ed25519 {}\n    Ed25519 {}",
                hex(9),
                hex(1),
                hex(2),
                hex(3)
            )
        );
        assert_eq!(ed(1).describe(), format!("Ed25519 {}", hex(1)));
        assert_eq!(ed(1).required_and_total(), (1, 1));
    }
}

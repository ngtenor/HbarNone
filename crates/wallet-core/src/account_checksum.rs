//! Hedera account ID checksums (HIP-15): `0.0.12345-abcde`. The five letters
//! depend on the network, so an ID checked for Testnet is rejected on Mainnet.

use crate::vault_container::Network;
use crate::wallet_service::validate_account_id;

const P3: u64 = 26 * 26 * 26;
const P5: u64 = P3 * 26 * 26;
const M: u64 = 1_000_003;
const W: u64 = 31;

fn ledger_id(network: Network) -> u8 {
    match network {
        Network::Mainnet => 0,
        Network::Testnet => 1,
    }
}

fn checksum_letters(network: Network, address: &str) -> String {
    let digits: Vec<u64> = address
        .bytes()
        .map(|byte| {
            if byte == b'.' {
                10
            } else {
                u64::from(byte - b'0')
            }
        })
        .collect();
    let (mut sd, mut sd0, mut sd1) = (0u64, 0u64, 0u64);
    for (index, digit) in digits.iter().enumerate() {
        sd = (sd * W + digit) % P3;
        if index % 2 == 0 {
            sd0 += digit;
        } else {
            sd1 += digit;
        }
    }
    let (sd0, sd1) = (sd0 % 11, sd1 % 11);

    let mut sh = 0u64;
    for byte in [ledger_id(network), 0, 0, 0, 0, 0, 0] {
        sh = (W * sh + u64::from(byte)) % P5;
    }

    let mut value = ((((digits.len() as u64 % 5) * 11 + sd0) * 11 + sd1) * P3 + sd + sh) * M % P5;
    let mut letters = [b'a'; 5];
    for slot in letters.iter_mut().rev() {
        *slot = b'a' + (value % 26) as u8;
        value /= 26;
    }
    String::from_utf8_lossy(&letters).into_owned()
}

/// The five-letter checksum of a normalised ID such as `0.0.12345`.
pub fn account_checksum(network: Network, account_id: &str) -> Result<String, String> {
    let id = validate_account_id(account_id)?;
    Ok(checksum_letters(network, &id))
}

/// The ID with its checksum appended, for example `0.0.12345-abcde`.
pub fn with_checksum(network: Network, account_id: &str) -> Result<String, String> {
    let id = validate_account_id(account_id)?;
    Ok(format!("{id}-{}", checksum_letters(network, &id)))
}

/// Parses typed input. A checksum, when present, must be correct for the
/// network; when `require_checksum` is set it must be present. Returns the
/// plain ID.
pub fn parse_account_input(
    input: &str,
    network: Network,
    require_checksum: bool,
) -> Result<String, String> {
    let trimmed = input.trim();
    match trimmed.split_once('-') {
        Some((id, typed)) => {
            let id = validate_account_id(id)?;
            if !typed.eq_ignore_ascii_case(&checksum_letters(network, &id)) {
                return Err(
                    "The account number's checksum does not match. Check every digit.".to_string(),
                );
            }
            Ok(id)
        }
        None if require_checksum => {
            Err("Enter the account number with its checksum, like 0.0.12345-abcde.".to_string())
        }
        None => validate_account_id(trimmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_published_hip15_vectors() {
        assert_eq!(
            account_checksum(Network::Mainnet, "0.0.123").unwrap(),
            "vfmkw"
        );
        assert_eq!(
            account_checksum(Network::Testnet, "0.0.123").unwrap(),
            "esxsf"
        );
    }

    #[test]
    fn parses_and_rejects_checksums() {
        let full = with_checksum(Network::Testnet, "0.0.12345").unwrap();
        assert!(full.starts_with("0.0.12345-") && full.len() == "0.0.12345-abcde".len());
        assert_eq!(
            parse_account_input(&full, Network::Testnet, true).unwrap(),
            "0.0.12345"
        );
        assert_eq!(
            parse_account_input(&full.to_uppercase(), Network::Testnet, true).unwrap(),
            "0.0.12345"
        );
        // Wrong network, a wrong digit and a missing checksum are all refused.
        assert!(parse_account_input(&full, Network::Mainnet, true).is_err());
        assert!(
            parse_account_input(&full.replace("12345", "12346"), Network::Testnet, true).is_err()
        );
        assert!(parse_account_input("0.0.12345", Network::Testnet, true).is_err());
        assert_eq!(
            parse_account_input("0.0.12345", Network::Testnet, false).unwrap(),
            "0.0.12345"
        );
    }
}

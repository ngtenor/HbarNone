use bip39::{Language, Mnemonic};
use ed25519_dalek::{Signer, SigningKey};
use hmac::{Hmac, Mac};
use rand::random;
use sha2::Sha512;
use zeroize::{Zeroize, Zeroizing};

pub struct Wallet {
    mnemonic: Zeroizing<String>,
    private_key: Zeroizing<[u8; 32]>,
    pub public_key: String,
}

impl Wallet {
    pub fn generate() -> Result<Self, String> {
        let entropy = Zeroizing::new(random::<[u8; 32]>());

        let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy[..])
            .map_err(|e| format!("generate mnemonic failed: {e}"))?;

        Self::from_bip39(mnemonic)
    }

    pub fn from_mnemonic(mnemonic_phrase: &str) -> Result<Self, String> {
        let mnemonic = Mnemonic::parse_in(Language::English, mnemonic_phrase)
            .map_err(|e| format!("invalid mnemonic: {e}"))?;

        Self::from_bip39(mnemonic)
    }

    fn from_bip39(mnemonic: Mnemonic) -> Result<Self, String> {
        let mnemonic_phrase = Zeroizing::new(mnemonic.to_string());
        let seed = Zeroizing::new(mnemonic.to_seed(""));
        let private_key = Zeroizing::new(derive_hedera_ed25519_key(&seed[..])?);
        let signing_key = SigningKey::from_bytes(&private_key);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());

        Ok(Self {
            mnemonic: mnemonic_phrase,
            private_key,
            public_key,
        })
    }

    pub fn display(&self) {
        println!("Public Key:");
        println!("{}", self.public_key);
    }

    pub fn mnemonic(&self) -> &str {
        &self.mnemonic
    }

    pub fn private_key_bytes(&self) -> &[u8; 32] {
        &self.private_key
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        SigningKey::from_bytes(&self.private_key)
            .sign(message)
            .to_bytes()
    }
}

fn derive_hedera_ed25519_key(seed: &[u8]) -> Result<[u8; 32], String> {
    let mut master = Hmac::<Sha512>::new_from_slice(b"ed25519 seed")
        .map_err(|e| format!("initialize master key derivation failed: {e}"))?;
    master.update(seed);
    let state = Zeroizing::new(master.finalize().into_bytes().to_vec());

    let mut private_key: [u8; 32] = state[..32]
        .try_into()
        .map_err(|_| "invalid master private key length".to_string())?;
    let mut chain_code: [u8; 32] = state[32..]
        .try_into()
        .map_err(|_| "invalid master chain code length".to_string())?;

    for index in [44u32, 3030, 0, 0, 0] {
        let mut mac = Hmac::<Sha512>::new_from_slice(&chain_code)
            .map_err(|e| format!("initialize child key derivation failed: {e}"))?;
        let mut data = Zeroizing::new([0u8; 37]);
        data[1..33].copy_from_slice(&private_key);
        data[33..].copy_from_slice(&(index | 0x8000_0000).to_be_bytes());
        mac.update(&data[..]);
        let child = Zeroizing::new(mac.finalize().into_bytes().to_vec());

        private_key.copy_from_slice(&child[..32]);
        chain_code.copy_from_slice(&child[32..]);
    }

    chain_code.zeroize();
    Ok(private_key)
}

#[cfg(test)]
mod tests {
    use super::Wallet;

    #[test]
    fn derives_standard_hedera_ed25519_key() {
        let mnemonic = "inmate flip alley wear offer often piece magnet surge toddler submit right radio absent pear floor belt raven price stove replace reduce plate home";

        let wallet = Wallet::from_mnemonic(mnemonic).unwrap();

        // Matches hiero-sdk-java's MnemonicTest.toStandardED25519PrivateKey vector.
        assert_eq!(
            hex::encode(&wallet.private_key[..]),
            "f8dcc99a1ced1cc59bc2fee161c26ca6d6af657da9aa654da724441343ecd16f"
        );
        assert_eq!(
            wallet.public_key,
            "2e42c9f5a5cdbde64afa65ce3dbaf013d5f9ff8d177f6ef4eb89fbe8c084ec0d"
        );
    }

    #[test]
    fn signs_with_the_derived_wallet_key() {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let mnemonic = "inmate flip alley wear offer often piece magnet surge toddler submit right radio absent pear floor belt raven price stove replace reduce plate home";
        let wallet = Wallet::from_mnemonic(mnemonic).unwrap();
        let message = b"HederaVault signing test";
        let signature = Signature::from_bytes(&wallet.sign(message));
        let public_key =
            VerifyingKey::from_bytes(&hex::decode(&wallet.public_key).unwrap().try_into().unwrap())
                .unwrap();

        assert!(public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn invalid_mnemonic_returns_error() {
        assert!(Wallet::from_mnemonic("not a valid phrase").is_err());
    }
}

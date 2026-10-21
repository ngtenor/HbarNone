use bip39::{Language, Mnemonic};
use ed25519_dalek::SigningKey;
use rand::random;
use sha2::{Digest, Sha256};

pub struct Wallet {
    pub mnemonic: String,
    pub seed: String,
    pub private_key: String,
    pub public_key: String,
}

impl Wallet {
    pub fn generate() -> Self {
        let entropy: [u8; 32] = random();

        let mnemonic = Mnemonic::from_entropy_in(
            Language::English,
            &entropy,
        )
        .expect("Failed to generate mnemonic");

        Self::from_bip39(mnemonic)
    }

    pub fn from_mnemonic(
        mnemonic_phrase: &str,
    ) -> Self {
        let mnemonic = Mnemonic::parse_in(
            Language::English,
            mnemonic_phrase,
        )
        .expect("Invalid mnemonic");

        Self::from_bip39(mnemonic)
    }

    fn from_bip39(
        mnemonic: Mnemonic,
    ) -> Self {
        let mnemonic_string = mnemonic.to_string();

        let seed = mnemonic.to_seed("");

        let seed_hex = hex::encode(seed);

        let hash = Sha256::digest(seed);

        let private_key_bytes: [u8; 32] = hash.into();

        let signing_key =
            SigningKey::from_bytes(&private_key_bytes);

        let verifying_key =
            signing_key.verifying_key();

        let private_key =
            hex::encode(private_key_bytes);

        let public_key =
            hex::encode(
                verifying_key.to_bytes(),
            );

        Self {
            mnemonic: mnemonic_string,
            seed: seed_hex,
            private_key,
            public_key,
        }
    }

    pub fn display(&self) {
        println!("Mnemonic:");
        println!("{}", self.mnemonic);

        println!("\nSeed:");
        println!("{}", self.seed);

        println!("\nPrivate Key:");
        println!("{}", self.private_key);

        println!("\nPublic Key:");
        println!("{}", self.public_key);
    }
}
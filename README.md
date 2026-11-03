# HederaVault

HederaVault is an early-stage, open-source self-custody wallet project for the
Hedera network. It begins as a desktop wallet and may later grow into an
offline-signing application and open hardware reference design.

## Two complementary wallet experiences

HederaVault is intended to support two complementary use cases:

- **Desktop wallet:** A convenient way to create and manage a Hedera wallet from a desktop app, for users who want to keep a modest amount of HBAR available for everyday network use and transaction fees. A small balance does not eliminate the risks of storing keys on a general-purpose computer; malware or a compromised device could still expose wallet material.
- **Offline signer:** The long-term security-focused goal is a Seedsigner-like device for Hedera, designed to generate and use keys in an air-gapped environment. The device is intended to keep private keys off network-connected computers.

These experiences have different threat models and should not be understood as offering the same level of protection. Wallets created on a desktop inherit the risks of that computer; transferring a desktop-created recovery phrase to an offline device does not undo its prior exposure. The offline device should be able to create its own wallet.

HederaVault is experimental and unaudited. The desktop wallet is not currently recommended for real funds.

## Security status

HederaVault is experimental software and has not received an independent
security audit. Do not use it to protect real funds or recovery phrases.
Treat development wallet material as compromised.

## Current implementation

- Rust workspace with a `wallet-core` library
- BIP39 mnemonic generation and recovery
- Validated BIP39 mnemonic import into an encrypted wallet record
- Raw private key import (hex or DER, Ed25519 or ECDSA secp256k1), with the key type detected from a DER key or, for bare hex, from the public keys' Testnet accounts; the wallet tab shows the public key and, for ECDSA, the EVM address
- Hedera ED25519 key derivation using the standard Hedera SDK derivation path
- A shared vault master key wrapped by an Argon2id-derived password key
- AES-256-GCM encryption for wallet metadata and wallet secrets under the master key
- Migration of existing per-wallet encrypted records into the shared-password vault
- Configurable inactivity auto-lock from 30 seconds to 30 minutes (5-minute default)
- A browser-style tabbed vault interface with a persistent HOME tab, wallet-specific tabs, and Settings for auto-lock, password changes, safety controls, backup, restore, and backup-and-erase; toolbar actions provide create/import, address book, Help, and lock
- Encrypted vault backup export and restore (separate Settings panels; restore also available from the first-run screen)
- Settings "Back up and erase all data from the app": saves and verifies an encrypted backup, then removes the vault so the app returns to its first-run state
- **Wallets and accounts:** A wallet is a key that can control multiple Hedera accounts. HOME displays wallets, named accounts, and Testnet balances. Account links use `Name - 0.0.12345-abcde` (`NONAME` for unnamed accounts). Imported mnemonic and private-key wallets discover linked accounts; wallet pages include account creation funded by another vault account. Mainnet is a data-model column only; Mainnet account access is not implemented.
- **Watch-only wallets:** Import an Ed25519 or ECDSA secp256k1 public key to discover and monitor matching Testnet accounts. These wallets are marked with a gold icon, allow balance and transaction viewing, and cannot reveal secrets or sign transactions. Unsigned-transaction export is not implemented.
- **Testnet account management:** Account pages show balances and recent transactions and support HBAR sends, renaming, moving an account to another wallet key (`CryptoUpdate`, signed by both keys), and account deletion (`CryptoDelete` with a recipient). Create, move, and delete actions include review, warnings, and acknowledgments where appropriate. These flows were manually exercised on Testnet; use disposable accounts only.
- **Address book and safety controls:** The address book lists wallet accounts and named outside accounts. External send and delete recipients are disabled by default and can be enabled in Settings. Deleting a wallet requires explicit confirmation.
- **Transaction status:** Sends, account creation, moves, and deletes show a progress popup and use the Testnet mirror node to report the result. Balances and recent transactions can be refreshed from the UI.
- **Help:** The Help tab content is bundled from `desktop-ui/src/help.html` at build time.
- Send HBAR on Testnet from a linked wallet (Ed25519 or ECDSA): a review screen shows network, sender, recipient, amount and maximum fee before the transfer is signed in the Rust backend and submitted to a Testnet node over gRPC (`hedera-tx` crate, hand-written protobuf messages)
- A two-button first-run screen (Create vault / Restore backup) and a centred unlock screen
- Wallet rename, with unique names (up to 40 characters), and blocking of duplicate recovery-phrase imports
- Wallet deletion and contextual private-key/recovery-phrase reveal from the selected wallet's tab
- Wallet-store persistence in the desktop app's operating-system data directory
- Tauri 2 and React desktop interface for creating, importing, listing, revealing, and deleting wallets

The vault password unlocks all wallets in the vault. The app creates a random
master key, wraps it with a key derived from the vault password, and uses the
master key to encrypt vault contents and wallet secrets. Normal
commands do not return mnemonic or private-key material to the UI; a user must
explicitly reveal a wallet's private key or recovery phrase while the vault is unlocked. The app locks
after the selected inactivity period and when it closes. The auto-lock setting
is encrypted as part of the vault contents. The app contacts the Hedera Testnet
mirror node for public account data when balances or transactions are requested.
HBAR sends and account create, move, and delete operations submit transactions
to Testnet only after the user reviews and confirms them. Mainnet access and
multisig are not implemented.

Existing records encrypted with separate wallet passwords are migrated by
asking for each wallet's current password once and a new vault password. The
old vault file is replaced only after all records are decrypted and validated
and the new encrypted vault is saved successfully.

Wallets can be removed from this device's local vault without re-entering the
vault password, after explicit confirmation while the vault is unlocked. This
does not delete a Hedera network account or securely erase older storage
copies or backups. If the vault password is forgotten, recovery phrases must
be backed up elsewhere to restore the wallets.

The vault setup form can generate a six-word passphrase locally using the
device's cryptographic random-number generator. Six uniformly selected words
from the 7,776-word EFF long list provide about 77.5 bits of entropy. Save the
passphrase safely: a forgotten vault password cannot be reset. This passphrase
is separate from, and must never be confused with, a wallet's recovery phrase.

The bundled EFF long word list is based on the
[EFF Dice-Generated Passphrases wordlist](https://www.eff.org/files/2016/07/18/eff_large_wordlist.txt),
published in [EFF's article on random passphrases](https://www.eff.org/deeplinks/2016/07/new-wordlists-random-passphrases).
The list is included under the
[Creative Commons Attribution 4.0 International License](https://creativecommons.org/licenses/by/4.0/);
the five-dice codes were removed for use by the generator.

Importing or revealing a phrase or private key exposes it on screen and in
application memory; malware or a compromised operating system may capture
wallet secrets, the vault password, or screen contents. Never share a secret;
only import or reveal disposable wallet material on a device you trust. A
forgotten vault password cannot currently be reset. Do not create a wallet
containing real funds or rely on this app for recovery. Use a long, unique
vault password and store it safely.

After revealing a wallet secret, the interface asks the user to acknowledge
that it has been written down and stored safely before completing the flow.
This is an acknowledgment only; the app does not check that the backup is
complete or correct.

## Development

Prerequisites: Rust, Node.js/npm, and the platform requirements for Tauri 2.

Install frontend dependencies and run the desktop application:

```powershell
npm install
Set-Location desktop-ui
npm install
Set-Location ..
npm run tauri:dev
```

Run the Rust tests and check the workspace:

```powershell
cargo test --workspace
cargo check --workspace
```

Build the frontend separately:

```powershell
Set-Location desktop-ui
npm run build
```

See [docs/TODO.md](docs/TODO.md) for current status and next steps.

## Longer-term roadmap

1. Finish Linux manual verification and remaining legacy-wallet migration checks
2. Add unsigned-transaction export for watch-only wallets
3. Offline signing and QR-based transaction transport
4. HTS tokens and NFTs, then a dedicated signer application and reproducible hardware reference design

## Security philosophy

The long-term signer design is intended to support air-gapped workflows with
ephemeral key handling. Encrypted persistent storage is intended for the
desktop wallet and is not a requirement for the future offline signer.

## License

Apache License 2.0. See [LICENSE](LICENSE).

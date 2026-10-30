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
- Hedera ED25519 key derivation using the standard Hedera SDK derivation path
- A shared vault master key wrapped by an Argon2id-derived password key
- AES-256-GCM encryption for wallet metadata and recovery phrases under the master key
- Migration of existing per-wallet encrypted records into the shared-password vault
- Configurable inactivity auto-lock from 30 seconds to 30 minutes (5-minute default)
- A browser-style tabbed vault interface with icon buttons at the top right (create/import wallet, settings, lock; a padlock shows when locked), a persistent HOME tab, wallet-specific tabs, and a Settings tab for auto-lock preferences and changing the vault password
- Encrypted vault backup export and restore (separate Settings panels; restore also available when creating a vault)
- Wallet rename, with unique names (up to 40 characters), and blocking of duplicate recovery-phrase imports
- Wallet deletion and recovery-phrase reveal from the selected wallet's tab
- Wallet-store persistence in the desktop app's operating-system data directory
- Tauri 2 and React desktop interface for creating, importing, listing, revealing, and deleting wallets

The vault password unlocks all wallets in the vault. The app creates a random
master key, wraps it with a key derived from the vault password, and uses the
master key to encrypt vault contents and wallet recovery phrases. Normal
commands do not return mnemonic or private-key material to the UI; a user must
explicitly reveal a wallet's phrase while the vault is unlocked. The app locks
after the selected inactivity period and when it closes. The auto-lock setting
is encrypted as part of the vault contents. Hedera network access,
account management, and transaction creation are not yet implemented.

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

The current interface is a development scaffold and does not yet provide a
complete recovery workflow. Importing or revealing a phrase exposes it on
screen and in application memory; malware may capture the phrase, vault
password, or screen contents. Never share a phrase; only import or reveal
disposable phrases on a device you trust. A forgotten vault password cannot
currently be reset. Do not create a wallet containing real funds or rely on
this app for recovery. Use a long, unique vault password and store it safely.

After revealing the phrase, the interface asks the user to acknowledge that it
has been written down and stored safely before completing the flow. This is an
acknowledgment only; the app does not check that the backup is complete or
correct.

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

1. Testnet support, account management, transfers, and transaction history
2. HTS tokens, NFTs, and smart-contract interactions
3. Offline signing and QR-based transaction transport
4. A dedicated signer application and reproducible hardware reference design

## Security philosophy

The long-term signer design is intended to support air-gapped workflows with
ephemeral key handling. Encrypted persistent storage is intended for the
desktop wallet and is not a requirement for the future offline signer.

## License

Apache License 2.0. See [LICENSE](LICENSE).

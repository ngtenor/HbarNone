# HBarNone

HBarNone is an early-stage, open-source self-custody wallet project for the
Hedera network. It begins as a desktop wallet and may later grow into an
offline-signing application and open hardware reference design.

## Two complementary wallet experiences

HBarNone is intended to support two complementary use cases:

- **Desktop wallet:** A convenient way to create and manage a Hedera wallet from a desktop app, for users who want to keep a modest amount of HBAR available for everyday network use and transaction fees. A small balance does not eliminate the risks of storing keys on a general-purpose computer; malware or a compromised device could still expose wallet material.
- **Offline signer:** The long-term security-focused goal is a Seedsigner-like device for Hedera, designed to generate and use keys in an air-gapped environment. The device is intended to keep private keys off network-connected computers.

These experiences have different threat models and should not be understood as offering the same level of protection. Wallets created on a desktop inherit the risks of that computer; transferring a desktop-created recovery phrase to an offline device does not undo its prior exposure. The offline device should be able to create its own wallet.

HBarNone is experimental and unaudited. The desktop wallet is not currently recommended for real funds.

## Security status

HBarNone is experimental software and has not received an independent
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
- A browser-style tabbed vault interface with a permanent icon tab for Create / import, Settings, Address book or Help, a permanent Wallets tab, and wallet-specific tabs. Toolbar actions switch the icon tab's section or lock the vault. Settings provides auto-lock, password changes, safety controls, backup, restore, and backup-and-erase.
- Encrypted vault backup export and restore (separate Settings panels; restore also available from the first-run screen)
- Settings "Back up and erase all data from the app": saves and verifies an encrypted backup, then removes the vault so the app returns to its first-run state
- **Wallets and accounts:** A wallet is a key that can control multiple Hedera accounts. The Wallets tab displays wallets, named accounts, and balances. Account links use `Name - 0.0.12345-abcde` (`NONAME` for unnamed accounts). Imported mnemonic and private-key wallets discover linked accounts; wallet pages include account creation funded by another vault account.
- **Network separation:** A topbar switch selects the Testnet or Mainnet view; each view shows only that network's wallets and accounts, and a wallet never moves between networks, so a key used on Testnet is not reused on Mainnet. Switching to Mainnet asks for confirmation. Mainnet wallets are watch-only and read-only: scan a signer-device QR to monitor accounts, balances and recent transactions through the public Mainnet mirror node; no secret is created, imported or used for Mainnet and nothing can be sent from the Mainnet view. A public key already in the vault on another network is refused.
- Wallet tabs show a colour-coded wallet icon and name. The wallet page lists its added date and key details; use **Wallet management** to rename the wallet, create an account, reveal available secrets, or delete the wallet. Refresh beside **Accounts** discovers Testnet accounts and refreshes their balances.
- **Watch-only wallets:** Create Testnet and Mainnet watch-only wallets by scanning a QR code displayed by the offline signer; public keys are not typed into the app. These wallets are marked with a gold icon, allow balance and transaction viewing, and cannot reveal secrets. A regular watch-only wallet signs eligible transactions through a 180-second QR request/reply flow; the reply can be scanned with the camera or pasted. The same flow supports account creation, deletion, and moving an account to another single-key wallet. Multisig wallets are made from an empty Ed25519 watch-only wallet using **Wallet management → Convert to multisig**. Their frozen 2-to-5-key threshold policy is reused for creating accounts, linking only accounts with an exact matching on-chain key structure, and signing supported Testnet operations. Multisig requests show one checkbox per participant and require the threshold of distinct signer replies; replies are scanned from signer-device QR codes. The app checks linked accounts against the policy and makes mismatched accounts read-only. Multisig key rotation is not currently supported.
- **Offline signer:** Run `hedera-offline-signer` (`cargo run -p offline-signer`). It can derive a public key from a disposable private key or phrase and display it as a QR code for watch-only wallet creation. For signing, scan or load a request, choose a recovery phrase or private key, check every displayed field (account IDs include their HIP-15 checksum), and type `yes`; the signer shows the reply as a QR code and text. Its parser supports HBAR transfer, account create/delete, key change, token transfer, NFT transfer and token association, and it never connects to a network. The signer displays the maximum fee, which it cannot independently verify against the network estimate. This is a test build: use disposable Testnet keys. Alternatively `cargo run -p hedera-tx --example sign_request -- <request> <ed25519|ecdsa> <private-key-hex>` stands in for the signer.
- **Automatic network fees:** Every Testnet transaction uses a state-aware network estimate, converted from USD tinycents to tinybars using the network exchange rate, plus a 25% buffer rounded up. There are no fixed 1/5/20 HBAR ceilings and no extra fee approval within the buffered maximum. If a node rejects a hot-wallet transaction at precheck with a higher fee quote, the app asks for approval to re-sign at that fee and retry once. Declining, ambiguous failures and final consensus failures are never automatically resubmitted. Offline signing requests carry the signed maximum in their payload without displaying fee details; increasing their fee requires a fresh request and new signatures, never modification of collected signatures. Only the actual network fee is charged. Estimation failures stop the transaction rather than substituting a default fee. Multisig quotes include every possible participant signature, so a subset of signers can cost less than the estimate. Rebuild the offline signer as well as the desktop app when updating.
- **Testnet account management:** Account pages show balances and recent transactions and support HBAR sends, renaming, moving an account to another wallet key (`CryptoUpdate`, signed by both keys), and account deletion (`CryptoDelete` with a recipient). Create, move, and delete actions include review, warnings, and acknowledgments where appropriate. Deleted tokens stay visible in Account assets marked "(deleted)"; they cannot be sent, but they can be dissociated whatever their remaining balance (the network discards it). These flows were manually exercised on Testnet; use disposable accounts only.
- **NFTs (Testnet):** Account assets list owned NFTs by token ID and serial number; the metadata text from the mirror node is treated as untrusted (shown only if printable, length-capped). NFT tokens can be associated and dissociated like fungible tokens (dissociation requires owning no serial numbers unless the token is deleted), and single NFTs are sent by serial number from the Send menu — from hot wallets directly and from watch-only wallets through the offline signing flow, where the signer displays token ID, serial number, sender and recipient. Before a send, the app checks on the mirror node that the token is a live NFT, that the sender owns that serial and that the receiver is associated. Transfers of NFTs whose token carries custom fees (for example artist royalties) are disabled by default behind a Settings toggle; the review step shows the token's fee schedule and warns that a royalty's fallback fee can charge the receiver. Hot wallets can also create an NFT collection (the account is the treasury; the wallet's key is the admin and supply key; no custom fees at creation) and mint serial numbers with metadata (1–100 bytes, usually a link) under Token management → Issuing; the offline signer keeps rejecting creation and minting by design. Marketplace features are out of scope.
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
to Testnet only after the user reviews and confirms them. Mainnet is limited to
watch-only monitoring; wallet-first multisig signing is Testnet-only and manual
Testnet verification remains pending.

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

Run the frontend tests and lint:

```powershell
Set-Location desktop-ui
npm test
npm run lint
```

Build the frontend separately:

```powershell
Set-Location desktop-ui
npm run build
```

See [docs/TODO.md](docs/TODO.md) for current status and next steps.

## Longer-term roadmap

1. NFT support on Testnet (implemented: display by token ID and serial number, associate/dissociate, hot-wallet and watch-only transfers via the offline signer, custom-fee gating; manual Testnet verification pending)
2. Wallet-first multisig: frozen Ed25519 M-of-N wallet policies, QR-based signer import and signature collection, exact account linking, and Testnet operation support; manual verification remains pending
3. A dedicated air-gapped signer device (QR transport, SeedQR key entry) and reproducible hardware reference design
4. Mainnet transactions through the signer only, after independent review and testing

## Security philosophy

The long-term signer design is intended to support air-gapped workflows with
ephemeral key handling. Encrypted persistent storage is intended for the
desktop wallet and is not a requirement for the future offline signer.

## License

Apache License 2.0. See [LICENSE](LICENSE).

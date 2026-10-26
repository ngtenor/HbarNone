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
- Hedera ED25519 key derivation using the standard Hedera SDK derivation path
- Argon2id key derivation and AES-256-GCM encrypted vault records
- Wallet-store persistence in the desktop app's operating-system data directory
- Initial Tauri 2 and React desktop interface for creating and listing wallets

Normal desktop commands do not return mnemonic or private-key material to the
UI. The user can explicitly reveal a wallet's recovery phrase after re-entering
the vault password. Wallet import, network access, account management, and
transaction creation are not yet implemented.

The current interface is a development scaffold. It creates and lists encrypted
wallet records and can display the recovery phrase on request, but does not yet
provide wallet import or a complete recovery workflow. Revealing the phrase
exposes it on screen and in application memory; malware may capture the phrase,
password, or screen contents. Write it down privately, never share it, and only
reveal it on a device you trust. A forgotten vault password cannot currently be
reset. Do not create a wallet containing real funds or rely on this app for
recovery. Use a long, unique vault password and store it safely.

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

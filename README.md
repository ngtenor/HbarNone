# HederaVault

HederaVault is an early-stage, open-source self-custody wallet project for the
Hedera network. It begins as a desktop wallet and may later grow into an
offline-signing application and open hardware reference design.

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

The desktop commands do not return mnemonic or private-key material to the UI.
Wallet recovery, network access, account management, and transaction creation
are not yet implemented.

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

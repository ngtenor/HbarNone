# HederaVault TODO

## Implemented
- [x] Encrypt wallet mnemonics with Argon2id and AES-256-GCM
- [x] Derive Hedera ED25519 keys using the standard BIP39/SLIP-0010 path and an SDK test vector
- [x] Store encrypted wallet records in the operating-system app-data directory
- [x] Add the initial Tauri 2 + React desktop shell
- [x] Add wallet create/list commands that do not return mnemonic or private-key material
- [x] Add password guidance and roll back wallet creation if saving fails
- [x] Test create/save/reload/unlock, wrong-password rejection, and save-failure rollback
- [x] Manually verify disposable wallet creation, listing, and persistence after app restart
- [x] Add password-gated recovery-phrase reveal with explicit screen and malware warnings; clear the view on close or focus loss
- [x] Require a write-down acknowledgment to complete the reveal flow, without implying that the backup was verified
- [x] Explain that a forgotten vault password cannot currently be reset

## Work before Linux testing
1. **Security and reliability review**
   - [x] Review vault format/versioning, KDF parameter validation, cryptographic input validation, secret lifetime/zeroization, and error handling; the source review found no substantiated vulnerabilities.
   - [ ] Verify app-data directory and wallet-file permissions on Windows and Linux.
   - Keep the development-only warning until the review and platform checks are complete.
2. **Desktop verification**
   - [x] Manually verify wrong-password rejection when revealing a disposable wallet's recovery phrase in the standalone Windows app; the phrase was not revealed.
   - [x] Manually verify acknowledgment gating and hiding the phrase with Escape and when switching away from the Windows app.
   - [x] Add a wallet-core persistence integration test for creating, listing, reloading, and unlocking a disposable wallet; it also checks the phrase is not stored in plaintext.
   - [x] Manually verify in the Windows app that the disposable wallet remains listed after closing and reopening the app.
   - [ ] Add an automated desktop-app end-to-end test that verifies persistence across an actual app restart.
   - [x] Build and launch the standalone Windows production app without the Vite development server; create and use an installer.
   - Use the release executable or installed app for standalone testing. A debug executable requires the Vite server and is expected to show a connection error if launched without it.
   - The browser smoke test uses mocked Tauri commands; it does not replace real-app verification.
3. **Wallet recovery and backup**
   - [x] Add English BIP39 mnemonic import with validation and encryption into a new wallet record.
   - [x] Keep imported mnemonic/private-key material out of normal command responses and logs.

On the weekend Linux machine, run the workspace tests and checks and verify the
desktop app builds and launches, and verify Linux app-data permissions. Do not
use real funds or recovery phrases.

## Later roadmap
- [ ] Integrate Hedera network access and Testnet account support
- [ ] Query account balances and transaction history
- [ ] Implement transaction creation and signing
- [ ] Add HTS token, NFT, and smart-contract support
- [ ] Build QR-based offline transaction transport
- [ ] Create an offline signer application
- [ ] Design a reproducible hardware prototype

## Validation status
The source security reviews found no substantiated vulnerabilities. The
standalone Windows x64 production app and NSIS/MSI installers built successfully
on the ARM Surface and the release executable was verified to launch with Vite
stopped. The manual Windows tests passed for wrong-password rejection,
persistence after app restart, acknowledgment gating, hiding the phrase with
Escape and on app focus-loss, and invalid mnemonic import.

Current code validation passed: `cargo fmt --all -- --check`, `cargo test
--workspace -j 1` (15 wallet-core unit tests and 1 persistence integration
test), frontend lint/build, and the standalone Tauri release build with NSIS/MSI
installers. Windows and Linux app-data permission checks and an automated
desktop-app restart test remain outstanding.

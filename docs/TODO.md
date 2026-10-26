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

## Next priorities
1. **Wallet recovery and backup**
   - Add mnemonic import with validation and encryption into a new wallet record.
   - Explain that a forgotten vault password cannot currently be reset.
   - Keep mnemonic/private-key material out of normal commands and logs.
2. **Security and reliability review**
   - Review vault format/versioning, KDF parameter validation, cryptographic input validation, and secret lifetime/zeroization.
   - Verify app-data directory and wallet-file permissions on supported platforms.
   - Review error handling and ensure failures never appear as successful wallet operations.
   - Keep the development-only warning until the review is complete.
3. **Automated desktop verification**
   - Add an end-to-end test for creating a disposable wallet, listing it, restarting the app, and confirming persistence.
   - Keep the existing manual restart check as a documented smoke test, not a substitute for automation.

## Later roadmap
- [ ] Integrate Hedera network access and Testnet account support
- [ ] Query account balances and transaction history
- [ ] Implement transaction creation and signing
- [ ] Add HTS token, NFT, and smart-contract support
- [ ] Build QR-based offline transaction transport
- [ ] Create an offline signer application
- [ ] Design a reproducible hardware prototype

## Latest validation
At commit `5a7524f`, `cargo fmt --all -- --check`, `cargo test --workspace -j 1` (13 tests), `cargo check --workspace`, frontend lint/build, and the Tauri debug build passed. A disposable wallet was manually confirmed to remain listed after restarting the app. For the recovery-phrase reveal changes, `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace --lib --tests -j 1` (14 library tests), and frontend lint/build passed. The initial Tauri debug build was blocked because Windows could not replace the running executable; after closing the app, the user rebuilt it successfully and confirmed the app works. The write-down acknowledgment UI passes frontend lint/build and the Tauri debug build. A browser smoke test with mocked Tauri commands and a fake phrase confirmed wrong-password rejection, successful reveal, acknowledgment gating, and hiding via confirmation, Hide now, Escape, and a simulated window-blur event. Actual wallet operations and platform focus changes in the Tauri desktop app still need manual confirmation. Automated desktop UI testing and the broader security review remain outstanding.

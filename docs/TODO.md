# HederaVault TODO

## Current status
- [x] Encrypt wallet mnemonics with Argon2id and AES-256-GCM
- [x] Derive Hedera ED25519 keys using the standard BIP39/SLIP-0010 path and an SDK test vector
- [x] Store encrypted vault records in the operating-system app-data directory
- [x] Add the initial Tauri 2 + React desktop shell
- [x] Add wallet create/list commands that do not return mnemonic or private-key material
- [x] Add basic password guidance and transactional rollback if wallet persistence fails
- [x] Manually verify disposable wallet creation, listing, and persistence after app restart
- [ ] Add recovery/import UX with safe mnemonic confirmation and password guidance
- [ ] Add an automated desktop UI test for wallet creation, listing, and persistence across restarts
- [ ] Review file permissions, secret-memory handling, and error reporting before any real-value use

## Next session checkpoint

The disposable-wallet create/list/restart flow was manually verified in the isolated test app. The Tauri create/list path also has wallet-service tests for create-save-reload-unlock, wrong-password rejection, and rollback after save failure. `cargo check --workspace`, `cargo test --workspace -j 1` (13 tests), the React production build, and frontend lint pass. Automated desktop UI testing is still outstanding.

Next:
1. Add a desktop UI test that creates a disposable wallet, lists it, restarts the app, and confirms persistence.
2. Design wallet recovery/import UX and safe mnemonic confirmation; backup/recovery are currently unavailable in the UI.
3. Review file permissions, secret-memory handling, and error reporting before any real-value use.

All previous user changes and commits should be preserved. `wallet_store.json` in the repository root is local test data and remains untracked; do not add it to Git.

## Future
- [ ] Integrate Hedera SDK
- [ ] Create Testnet account
- [ ] Query account balances
- [ ] Implement transaction signing
- [ ] Build QR transport layer
- [ ] Create offline signer application
- [ ] Design hardware prototype

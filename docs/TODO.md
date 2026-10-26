# HederaVault TODO

## Current status
- [x] Encrypt wallet mnemonics with Argon2id and AES-256-GCM
- [x] Derive Hedera ED25519 keys using the standard BIP39/SLIP-0010 path and an SDK test vector
- [x] Store encrypted vault records in the operating-system app-data directory
- [x] Add the initial Tauri 2 + React desktop shell
- [x] Add wallet create/list commands that do not return mnemonic or private-key material
- [ ] Add recovery/import UX with safe mnemonic confirmation and password guidance
- [ ] Add an end-to-end desktop smoke test and verify persistence across restarts
- [ ] Review file permissions, secret-memory handling, and error reporting before any real-value use

## Next session checkpoint

The initial Tauri desktop shell is implemented. `cargo check --workspace`, `cargo test --workspace -j 1` (10 tests), the React production build, frontend lint, and the Tauri debug executable build (`npm run tauri:build -- --debug --no-bundle`) have passed. The executable remained open during an 8-second startup smoke test.

Next:
1. Run the Tauri desktop app and verify create/list plus persistence after restart.
2. Add automated tests for wallet-service behavior and save-failure rollback.
3. Design wallet recovery/import UX, password guidance, and safe mnemonic confirmation.
4. Review file permissions, secret-memory handling, and error reporting before any real-value use.

All previous user changes and commits should be preserved. `wallet_store.json` in the repository root is local test data and remains untracked; do not add it to Git.

## Future
- [ ] Integrate Hedera SDK
- [ ] Create Testnet account
- [ ] Query account balances
- [ ] Implement transaction signing
- [ ] Build QR transport layer
- [ ] Create offline signer application
- [ ] Design hardware prototype

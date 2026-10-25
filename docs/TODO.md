# HederaVault TODO

## Next session checkpoint

Continue the project review and begin the Tauri desktop app work. The workspace tests passed on 2026-10-25 (`cargo test --workspace`: 3 tests passed).

Suggested order:
1. Confirm the intended Hedera ED25519 key derivation and add test vectors before relying on wallet keys.
2. Harden wallet secret handling and vault input validation; add tests for malformed vault fields.
3. Improve wallet-store persistence, application-data path selection, and wallet ID generation; add load/save tests.
4. Scaffold the Tauri app and minimal UI, then expose wallet-core operations through commands that never return mnemonic or private-key material.
5. Update examples and project documentation to reflect what is implemented; keep demo credentials and secret-printing examples clearly test-only.

At the last review, `src-tauri` contained only a placeholder Cargo manifest and `main.rs`, and `desktop-ui` was empty. Existing project changes are uncommitted; preserve them and inspect `git status` before editing.

## Current
- [ ] Implement encrypted wallet storage
- [ ] Add Argon2id password derivation
- [ ] Add AES-256-GCM encryption
- [ ] Save wallet.dat
- [ ] Load wallet.dat

## Future
- [ ] Integrate Hedera SDK
- [ ] Create Testnet account
- [ ] Query account balances
- [ ] Implement transaction signing
- [ ] Build QR transport layer
- [ ] Create offline signer application
- [ ] Design hardware prototype

# HederaVault TODO

## Implemented
- [x] Encrypt wallet mnemonics with Argon2id and AES-256-GCM
- [x] Derive Hedera ED25519 keys using the standard BIP39/SLIP-0010 path and an SDK test vector
- [x] Store encrypted wallet records in the operating-system app-data directory
- [x] Add the initial Tauri 2 + React desktop shell
- [x] Add wallet create/list commands that do not return mnemonic or private-key material
- [x] Add password guidance and roll back wallet creation if saving fails
- [x] Test create/save/reload/unlock, wrong-password rejection, and save-failure rollback
- [x] Add vault-unlock-gated recovery-phrase reveal with explicit screen and malware warnings; clear the view on close or focus loss
- [x] Require a write-down acknowledgment to complete the reveal flow, without implying that the backup was verified
- [x] Explain that a forgotten vault password cannot currently be reset
- [x] Generate six-word vault passphrases locally from the EFF long list using the device's cryptographic random-number generator
- [x] Delete a wallet from the local vault without requiring its password, with explicit irreversible-action confirmation
- [x] Protect all wallet records with a single vault password that wraps a random master key
- [x] Migrate legacy wallets by verifying each old wallet password before replacing the old store
- [x] Add an inactivity auto-lock slider from 30 seconds to 30 minutes, defaulting to 5 minutes
- [x] Lock the vault on app close and provide a manual lock action
- [x] Organize the unlocked vault into a persistent HOME tab, a create/import tab, and separate closable wallet tabs
- [x] Keep wallet actions, including recovery-phrase reveal and deletion, within each wallet tab
- [x] Add a Settings tab opened from HOME and move auto-lock controls into it
- [x] Build the latest x64 Windows standalone app and NSIS/MSI installers with the tabbed vault and Settings UI
- [x] Delay repeated failed unlock attempts (3 free, then 5 seconds doubling to 5 minutes)
- [x] Block selection, copy, cut, drag and the context menu on the revealed recovery phrase
- [x] Tighten the release CSP and move dev-server origins to `devCsp`
- [x] Restrict Linux app-data permissions (`0700` directory, `0600` vault file)
- [x] Add vault password change in Settings (re-wraps the master key; wallets are not re-encrypted)

## Work before Linux testing
1. **Security and reliability review**
   - [x] Review vault format/versioning, KDF parameter validation, cryptographic input validation, secret lifetime/zeroization, and error handling; the source review found no substantiated vulnerabilities.
   - [x] Verify app-data directory and wallet-file permissions on Windows: `%APPDATA%\org.hederavault.desktop` and `wallets.json` inherit ACLs granting access only to SYSTEM, Administrators and the owning user, so the defaults are restrictive.
   - [ ] Verify app-data directory and wallet-file permissions on Linux (expect `0700` directory and `0600` file).
   - [x] Explicitly set restrictive Linux permissions: the app sets the app-data directory to `0700` at startup and vault saves set the file to `0600` (unit-tested on Unix in CI). Not needed on Windows.
   - [x] Add brute-force friction to unlock: after 3 consecutive failed attempts, further attempts are rejected for 5 seconds, doubling per failure up to 5 minutes; a successful unlock resets it. The counter is in memory only, so restarting the app resets it; the Argon2id cost remains the main defence against offline guessing. Covered by wallet-core unit tests and manually verified in the release app.
   - [x] Confirm sensitive state is cleared on window minimise (manually verified) and that the clipboard is never used for secrets. The app has no clipboard API calls or Tauri clipboard permission, and the revealed phrase now blocks selection, copy, cut, drag and the context menu (friction only; screenshots and malware are unaffected). The copy blocking was manually verified in the rebuilt release app.
   - [x] Review the Tauri CSP and capabilities: the capability grants only `core:default` and no plugins are installed. The release CSP no longer allows the dev-server `http`/`ws` origins, `unsafe-inline` styles or `asset:` images (those moved to `devCsp`), and adds `object-src 'none'`, `base-uri`, `form-action` and `frame-ancestors` restrictions.
   - [x] Rebuild the release app and confirm the UI still renders and styles correctly under the stricter CSP (manually verified).
   - Keep the development-only warning until the review and platform checks are complete.
2. **Desktop verification**
   - [x] Manually verify vault creation with a generated six-word passphrase, masked-by-default entry, unlock, wrong-password rejection, and manual lock.
   - [ ] Manually verify migration of existing wallets with multiple old passwords; confirm a wrong password leaves the original wallets intact.
   - [x] Manually verify auto-lock slider bounds/default, timer reset after activity, lock after inactivity, and locked state after closing and reopening the app.
   - [x] Manually verify wallet deletion confirmation and persistence after an app restart.
   - [x] Manually verify HOME remains first, create/import opens in a closeable tab, wallets open in distinct switchable tabs, and closing or deleting a wallet tab returns to HOME as expected.
   - [x] Manually verify the HOME Settings button opens its tab and auto-lock settings can be changed and retained there.
   - [x] Add wallet-core tests for vault persistence, encrypted metadata and recovery phrases, wrong-password rejection, old-wallet migration, migration rollback, auto-lock bounds/expiry, and wallet mutation rollback.
   - [x] Add a wallet-core persistence integration test for creating, listing, locking, reloading, unlocking, and revealing a disposable wallet; it checks that the phrase is not stored in plaintext.
   - [ ] Add an automated desktop-app end-to-end test that verifies persistence across an actual app restart.
   - [x] Build the updated standalone Windows production app and NSIS/MSI installers without overwriting the running app; the latest build includes the HOME, wallet, create/import, and Settings tabs.
   - [x] Manually launch and test the updated Windows app without the Vite development server; use only disposable wallets.
   - Use the release executable or installed app for standalone testing. A debug executable requires the Vite server and is expected to show a connection error if launched without it.
   - The browser smoke test uses mocked Tauri commands; it does not replace real-app verification.
3. **Wallet recovery and backup**
   - [x] Add English BIP39 mnemonic import with validation and encryption into a new wallet record.
   - [x] Keep imported mnemonic/private-key material out of normal command responses and logs.
   - [x] Add vault password change in Settings: verifies the current password and re-wraps the master key under a new salt and password, saving atomically and leaving the vault unchanged if the save fails. Unit-tested; not yet manually verified in the app.
   - [ ] Add an encrypted, restorable backup export and restore, protected by the vault password, so a vault can be moved between machines. Never write seed words to a plaintext file.
   - [ ] Detect duplicate imports of the same recovery phrase and warn or block.
   - [ ] Add wallet rename.
4. **Automated testing and CI**
   - [ ] Add frontend component tests (Vitest and React Testing Library) with a mocked `invoke`, covering tab logic, auto-lock, and the reveal and delete flows.
   - [x] Add a GitHub Actions workflow (`.github/workflows/ci.yml`) running `cargo fmt --check`, `cargo test --workspace`, `cargo check --workspace`, frontend lint and frontend build on both Windows and Ubuntu. Passed on GitHub (Windows and Ubuntu) in the first run on PR #1.
5. **Linux readiness**
   - [x] Document the Ubuntu packages Tauri 2 needs: they are installed by `scripts/linux-test.sh` (build-essential, libssl-dev, libwebkit2gtk-4.1-dev, libgtk-3-dev, librsvg2-dev, libayatana-appindicator3-dev, libxdo-dev).
   - [x] Write a Linux test script, `scripts/linux-test.sh`: installs packages, checks toolchains and entropy, runs `cargo fmt`/`test`/`check`, builds and lints the frontend, runs `npm run tauri:build`, and checks app-data permissions with `stat`. It writes a timestamped `linux-test-*.log`. Options: `--skip-apt`, `--skip-bundle`. Untested on Linux; the CI Ubuntu job will exercise the same steps.
   - [ ] Run the script on Ubuntu, launch the built app, and complete the manual checks with disposable wallets.
   - [ ] Decide the Linux test environment: Ubuntu 24.04 LTS on an external NVMe/SSD (at least 256 GB, USB 3.2 Gen 2 or Thunderbolt/USB4 enclosure), or a partition on the Dell.
6. **Housekeeping**
   - [x] Open a pull request from `feature/wallet-generation` to `main` (merged as PR #1; first CI run passed on Windows and Ubuntu).
   - [x] Add a `.gitattributes` file to settle the LF/CRLF warnings.
   - [ ] Add a screenshot or short walkthrough to the README.

Suggested order: manually verify password change, run the Linux checks, then
encrypted backup and the remaining recovery items.

On the weekend Linux machine, run the workspace tests and checks and verify the
desktop app builds and launches, and verify Linux app-data permissions. Do not
use real funds or recovery phrases.

## Vault lifecycle and encrypted backups
Password change and encrypted backup export/restore are now tracked under
"Wallet recovery and backup" in the pre-Linux work above.

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
shared-vault architecture, legacy-wallet migration, inactivity auto-lock,
unlock brute-force delay, vault password change, HOME/wallet/create-import
tabs, wallet-specific reveal/delete actions, copy-blocked recovery-phrase
display, and Settings tab are implemented, with a stricter release CSP and
Linux `0700`/`0600` permission hardening. The latest standalone x64 Windows
app and NSIS/MSI installers include these changes. `cargo fmt --all -- --check`,
`cargo test --workspace` (19 wallet-core unit tests and 1 persistence
integration test), frontend lint/build, and `cargo check --workspace` passed
locally, and CI passed on Windows and Ubuntu for PR #1. Tests cover encryption,
legacy migration and failure preservation, locked persistence, auto-lock
bounds/expiry/activity reset, wallet mutation rollback, unlock delay, and
password change.

Manually verified on Windows: vault creation/unlock/lock, auto-lock, tabs,
Settings, wallet deletion and persistence, standalone launch, clearing on
minimise, copy blocking, CSP rendering, unlock delay, and app-data permissions.
Still outstanding: manual verification of vault password change, Linux
permission checks and app testing, and an automated desktop-app restart test.
Build note: this ARM64 Windows machine needs the x64 Rust toolchain
(`RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc`) and the x64 Visual Studio
environment to build the x64 app.

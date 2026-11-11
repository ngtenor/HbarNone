# HBarNone TODO

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
- [x] Unify account links across HOME, wallet, and account pages as `Name - 0.0.12345-abcde`; keep selected-account emphasis and HOME rollover shading
- [x] Add contextual wallet-secret reveal labels for private keys and recovery phrases
- [x] Unify button text/background styling and distinguish risky actions
- [x] Replace the account-page "Back to wallet" button with a wallet-name link
- [x] Show funding-account HBAR balances in the Create account dropdown
- [x] Add watch-only wallets with validated public-key import, Testnet account discovery, a gold wallet badge, read-only account controls, and backend signing/reveal guards

## Security, reliability, and desktop verification
1. **Security and reliability review**
   - [x] Review vault format/versioning, KDF parameter validation, cryptographic input validation, secret lifetime/zeroization, and error handling; the source review found no substantiated vulnerabilities.
   - [x] Verify app-data directory and wallet-file permissions on Windows: `%APPDATA%\org.hederavault.desktop` and `wallets.json` inherit ACLs granting access only to SYSTEM, comAdministrators and the owning user, so the defaults are restrictive.
   - [x] Verify app-data directory and wallet-file permissions on Linux (expect `0700` directory and `0600` file). Verified on Ubuntu 26.04: `~/.local/share/org.hederavault.desktop` is `drwx------`, and `wallets.json`, `wallets.json.pre-restore` and exported backups are `-rw-------`.
   - [x] Explicitly set restrictive Linux permissions: the app sets the app-data directory to `0700` at startup and vault saves set the file to `0600` (unit-tested on Unix in CI). Not needed on Windows.
   - [x] Add brute-force friction to unlock: after 3 consecutive failed attempts, further attempts are rejected for 5 seconds, doubling per failure up to 5 minutes; a successful unlock resets it. The counter is in memory only, so restarting the app resets it; the Argon2id cost remains the main defence against offline guessing. Covered by wallet-core unit tests and manually verified in the release app.
   - [x] Confirm sensitive state is cleared on window minimise (manually verified) and that the clipboard is never used for secrets. The app has no clipboard API calls or Tauri clipboard permission, and the revealed phrase now blocks selection, copy, cut, drag and the context menu (friction only; screenshots and malware are unaffected). The copy blocking was manually verified in the rebuilt release app.
   - [x] Review the Tauri CSP and capabilities: the capability grants only `core:default` and no plugins are installed. The release CSP no longer allows the dev-server `http`/`ws` origins, `unsafe-inline` styles or `asset:` images (those moved to `devCsp`), and adds `object-src 'none'`, `base-uri`, `form-action` and `frame-ancestors` restrictions.
   - [x] Rebuild the release app and confirm the UI still renders and styles correctly under the stricter CSP (manually verified).
   - Keep the development-only warning until the review and platform checks are complete.
2. **Desktop verification**
   - [x] Manually verify vault creation with a generated six-word passphrase, masked-by-default entry, unlock, wrong-password rejection, and manual lock.
   - [x] Manually verify migration of existing wallets with multiple old passwords; confirm a wrong password leaves the original wallets intact. Verified on Ubuntu with three legacy wallets using different passwords: a wrong password was refused and `wallets.json` was byte-identical afterwards; with the right passwords all three wallets migrated. Helper: `scripts/linux-migration-check.sh`.
   - [x] Manually verify auto-lock slider bounds/default, timer reset after activity, lock after inactivity, and locked state after closing and reopening the app.
   - [x] Manually verify wallet deletion confirmation and persistence after an app restart.
   - [x] Manually verify HOME remains first, create/import opens in a closeable tab, wallets open in distinct switchable tabs, and closing or deleting a wallet tab returns to HOME as expected.
   - [x] Manually verify the HOME Settings button opens its tab and auto-lock settings can be changed and retained there.
   - [x] Add wallet-core tests for vault persistence, encrypted metadata and recovery phrases, wrong-password rejection, old-wallet migration, migration rollback, auto-lock bounds/expiry, and wallet mutation rollback.
   - [x] Add a wallet-core persistence integration test for creating, listing, locking, reloading, unlocking, and revealing a disposable wallet; it checks that the phrase is not stored in plaintext.
   - [x] Verify persistence across restarts automatically with a Rust integration test (`crates/wallet-core/tests/wallet_persistence.rs`): it drops and reloads the service from disk across five sessions, checking auto-lock, rename, delete, re-import, password change (old password rejected, keys unchanged) and export followed by restore into a fresh store. A true GUI end-to-end test of the packaged app (e.g. `tauri-driver`/WebDriver) is deferred as impractical on Windows and Ubuntu CI for now.
   - [x] Build the updated standalone Windows production app and NSIS/MSI installers without overwriting the running app; the latest build includes the HOME, wallet, create/import, and Settings tabs.
   - [x] Manually launch and test the updated Windows app without the Vite development server; use only disposable wallets.
   - [x] Manually verify watch-only public-key import, Testnet account discovery, gold badge, balance viewing, and the absence of signing controls (verified on Windows with a public key found on HashScan).
   - Use the release executable or installed app for standalone testing. A debug executable requires the Vite server and is expected to show a connection error if launched without it.
   - The browser smoke test uses mocked Tauri commands; it does not replace real-app verification.
3. **Wallet recovery and backup**
   - [x] Add English BIP39 mnemonic import with validation and encryption into a new wallet record.
   - [x] Keep imported mnemonic/private-key material out of normal command responses and logs.
   - [x] Add vault password change in Settings: verifies the current password and re-wraps the master key under a new salt and password, saving atomically and leaving the vault unchanged if the save fails. Unit-tested and manually verified in the release app. The new-password field has a Show/Hide toggle so a generated passphrase can be read and written down before submitting.
   - [x] Add encrypted backup export and restore, as separate Settings panels. A backup is the encrypted vault file (no plaintext phrases), protected by the vault password in use at export time; restore replaces all wallets, keeps a `.pre-restore` copy, and also works from the create-vault screen so a vault can move between machines. Exported files are `0600` on Unix. Unit-tested (round trip, wrong password, bad/oversized files, unchanged vault on failure, `.pre-restore`, locked-vault rejection) and manually verified on Windows. Verified on Ubuntu 26.04: GTK file dialogs, `0600` permissions, restore of a Linux-made backup and restore of a Windows-made backup.
   - [x] Block duplicate imports of the same recovery phrase (checked against the decrypted phrases in memory; the error names the existing wallet, never the phrase). Unit-tested and manually verified.
   - [x] Add wallet rename, with names required to be non-empty, at most 40 characters, free of control characters, and unique within the vault ignoring capitalisation (enforced for create, import and rename). Rename is saved atomically and rolled back on failure. Unit-tested and manually verified.
4. **Automated testing and CI**

   - [x] Add Settings "Back up and erase this device": saves an encrypted backup via the file dialog, reads it back to verify, then deletes `wallets.json` and `wallets.json.pre-restore` and returns the app to the create-vault screen. Nothing is erased if the dialog is cancelled, the write or verification fails, the vault is locked, or the vault changed since the export. Requires a confirmation checkbox. Deletion is not a forensic wipe. Unit and Vitest tests added; manually verified on Linux.
   - [x] Add frontend component tests (Vitest and React Testing Library, jsdom) in `desktop-ui/src/App.test.tsx` with a stateful fake `invoke`. The suite covers unlock/lock gates, tab logic and keyboard navigation, auto-lock, reveal, delete, rename, backup/restore, password change, Testnet account flows, and account navigation. The latest run passed all 65 tests.
   - [x] Add a GitHub Actions workflow (`.github/workflows/ci.yml`) running `cargo fmt --check`, `cargo test --workspace`, `cargo check --workspace`, frontend lint, frontend tests and frontend build on both Windows and Ubuntu. Passed on GitHub (Windows and Ubuntu) in the first run on PR #1.
5. **Linux readiness**
   - [x] Document the Ubuntu packages Tauri 2 needs: they are installed by `scripts/linux-test.sh` (build-essential, libssl-dev, libwebkit2gtk-4.1-dev, libgtk-3-dev, librsvg2-dev, libayatana-appindicator3-dev, libxdo-dev).
   - [x] Write and run the Linux test script, `scripts/linux-test.sh`: installs packages, checks toolchains and entropy, runs `cargo fmt`/`test`/`check`, builds and lints the frontend, runs `npm run tauri:build`, and checks app-data permissions with `stat`. It writes a timestamped `linux-test-*.log`. Options: `--skip-apt`, `--skip-bundle`.
   - [x] Run the script on Ubuntu 26.04.1; `scripts/linux-test.sh` passed (tests, lint, build, and bundle). Backup, restore, and permissions were checked manually; one WebKitWebProcess crash occurred once during restore and was not reproduced.
   - [x] Decide the Linux test environment: Ubuntu 26.04 LTS dual-boot on a 54 GB partition of the Dell (i7-6700T).
6. **Housekeeping**
   - [x] Open a pull request from `feature/wallet-generation` to `main` (merged as PR #1; first CI run passed on Windows and Ubuntu).
   - [x] Add a `.gitattributes` file to settle the LF/CRLF warnings.

## Remaining manual verification
- [x] In Linux, verify legacy-wallet migration with multiple old passwords; confirm a wrong password preserves the original wallets. Verified on Ubuntu (see above).
- [x] In Linux, verify Backup and erase saves a usable backup, erases the vault, and permits restore. Verified on Ubuntu: the backup was `-rw-------`, the app returned to the create-vault screen, and restoring the backup brought back all wallets and accounts.
- [x] On the current Linux build, verify launch, app-data permissions, rename, duplicate-import rejection, and lock behavior. Verified on Ubuntu.
- [x] Manually test the current Testnet account flows on Windows with disposable accounts: create, send HBAR, move, delete, refresh balances/transactions, and review transaction status.
- [ ] Add a screenshot or short walkthrough to the README.
- [ ] A GUI-level restart test of the packaged app remains deferred; persistence across sessions is covered by Rust integration tests.

Use disposable wallets and Testnet accounts only. Never test with real funds or recovery phrases.

## Vault lifecycle and encrypted backups
Password change and encrypted backup export/restore are tracked under
"Wallet recovery and backup" above.

## Hedera Testnet integration
- [x] Account ID checksums (HIP-15, `0.0.12345-abcde`, verified against the published 0.0.123 vectors for Mainnet and Testnet): accounts are displayed with the checksum; send and delete recipients must carry a valid one; linking by ID verifies one if typed.
- [x] Account names (max 25 characters, shown as `Name - 0.0.12345-abcde`, unnamed = `NONAME`) and an address book: wallet accounts and named outside accounts are listed with checksummed IDs and balances. External send and delete recipients are disabled by default and can be enabled in Settings.
- [x] Multi-account wallets: each wallet (key) holds a list of `(network, account)` entries, unique per network across wallets, up to 50 per wallet; a legacy single account is migrated to Testnet on unlock. Import finds accounts for the key (single-key accounts only). Account creation (`CryptoCreate`), moving an account to another wallet's key (`CryptoUpdate`, signed by both keys), and account deletion (`CryptoDelete`) were implemented and manually verified on Testnet with disposable accounts. Mainnet is a data-model column only; multisig accounts are deferred to QR transport.
- [x] `hedera-mirror` crate: read-only client for the public Testnet mirror node (fixed URL, https only, no redirects, 15 s timeout, 512 KB cap); contacts the network only when the user clicks a button
- [x] Testnet panel on each wallet tab: link an account by ID, find an account by key (Ed25519), refresh balance and recent transactions. Verified manually against the real Testnet (1000 ℏ shown)
- [x] Implement Settings "Back up and erase this device"; manually verified on Linux.
- [x] First-run, unlock and Settings layout polish (two-button start screen, in-box eye toggle, centred fixed-size boxes, adaptive logo/padlock)

## Timetable: 7–15 November 2026

The target is a feature-complete Testnet desktop prototype for the agreed NFT
and multisig scope by 15 November, when the signer parts are expected. This is
an ambitious target, not a commitment to production readiness or a finished
hardware signer. Complete each stage's documentation, automated tests, manual
Testnet verification and CI before moving to the next stage.

### 7 November: baseline (completed 9 November)
- [x] Update stale roadmap and handover notes: token stages 2–6 are tested on
  Testnet and merged into main, offline key rotation is done, and the README
  and TODO roadmaps were refreshed. Outstanding verification: the next Linux
  check run (micro-SD procedure) covering the batch since the last run, the
  README screenshot/walkthrough, and the deferred GUI-level restart test.
- [x] Current release confirmed: the x64 Windows build of `main` at commit
  `3140ed9` ("Move screen polish: aligned signature rows and buttons"); no
  version tags exist yet. NFT and multisig scope agreed 9 Nov and recorded in
  the stage sections below.
- [x] Hardware bill of materials updated with purchased parts and the
  requirement that the signer never be connected to a computer; see
  "Hardware bill of materials (signer prototype)" below.

### 8–10 November: NFT support
- [x] Display owned NFTs by token ID and serial number; treat metadata as
  untrusted. (Implemented 10 Nov: `/accounts/{id}/nfts` on the mirror client,
  Account assets list and Send menu entries; metadata is base64-decoded,
  control-stripped and capped at 100 characters.)
- [x] Associate and dissociate NFT tokens and transfer NFTs by serial number.
  (Implemented 10 Nov: association checks now cover both token types; NFT
  dissociation requires zero owned serials unless the token is deleted;
  `CryptoTransfer` with `nftTransfers` built in `hedera-tx`.)
- [x] Support hot-wallet and watch-only transfers; show token ID, serial number,
  sender and recipient on the offline signer. (Implemented 10 Nov: `send_nft`,
  `prepare_unsigned_nft_transfer`, and a `SEND AN NFT` screen in the CLI
  signer; signer-core rejects NFT bodies with fungible amounts, decimals,
  several serials, or a sender other than the payer.)
- [x] Explicitly handle risky cases: reject invalid ownership or association.
  Transfers of NFTs with custom fees (for example artist royalties) are
  allowed but disabled by default behind an allow/disallow toggle in Settings
  (agreed 9 Nov); before sending, show the token's fee schedule from the
  mirror node and warn that fallback fees can charge the receiver.
  (Implemented 10 Nov: ownership checked against `/tokens/{id}/nfts/{serial}`,
  receiver association checked, Settings toggle stored in the vault payload
  and also enforced when a signed watch-only request is submitted.)
- [x] Scope extension (agreed 10 Nov): create an NFT collection (no custom
  fees at creation) and mint serial numbers with metadata, hot wallets only —
  the signer keeps rejecting token creation and minting by design. The app can
  now produce its own NFT test data. Marketplace features remain out of scope;
  Testnet only.
- [ ] Deferred NFT follow-ups, to be implemented later (after the current
  timetable permits):
  - Tier 2: custom fees at collection creation (royalty with an optional
    fallback fee), with its own fee-input form and review warnings. Until
    then, testing the custom-fee toggle needs an externally created royalty
    collection.
  - Tier 3: burn an owned NFT by serial number (`TokenBurn` with
    `serialNumbers`; supply key and treasury checks as for minting).
  - Both stay hot-wallet only and rejected by the offline signer by design.
- [ ] Test successful and failure paths with disposable Testnet accounts,
  document the workflow and pass CI. (Automated tests added for the mirror
  parsers, transaction bodies, signer whitelist and UI flows; the manual
  Testnet run with disposable accounts is still to be done.)

### 11–13 November: multisig
- [ ] Start with the planned 2-of-3 threshold account: display the key structure
  and collect, validate and combine signatures for the same transaction body.
  Implement the threshold logic generically (M-of-N, as Hedera's
  `ThresholdKey` already is) so shapes such as 3-of-5 need no structural
  change; 2-of-3 is the verified baseline and a 3-of-5 Testnet test is done
  only if time permits (agreed 9 Nov).
- [ ] Support the agreed account setup/key-change flow and a basic transfer
  through the desktop coordinator and software signer.
- [ ] Reject duplicate, unrelated, invalid or mismatched signatures and explain
  the transaction signing time limit.
- [ ] Test successful and insufficient-signature cases on Testnet, document
  recovery and limitations, and pass CI. Keep this milestone focused on
  ordinary transactions; schedule transactions are committed for the later
  roadmap below.
- [ ] Keep the key-tree representation and threshold evaluation generic and
  independent of transaction type. Keep request validation, signature
  verification and aggregation separable; bind each reply to the exact signing
  bytes and signer key, and do not assume every signature arrives within one
  180-second transaction window. Treat schedule signing as a distinct intent,
  not as an ordinary transaction with a different label.

### 14 November: hardware readiness and consolidation
- [ ] Prepare a wiring plan for the Pico 2, display, Arducam SPI camera and
  microSD reader; check pin, power and physical compatibility.
- [ ] Specify first-use provisioning and signed SD-card updates without
  connecting the signer to a computer.
- [ ] Specify signer export of QR images to microSD and a separate,
  network-disabled laptop app that converts imported QR images to GRBL G-code
  for Universal Gcode Sender (UGS). Test with a non-secret sample and document
  the risks of handling secret SeedQRs and toolpaths.
- [ ] Run regression checks and prepare a hardware test handover checklist.
- [ ] BIP39 passphrase support in the signer, to be implemented before the
  physical signer is developed (agreed 10 Nov). The device is stateless (the
  seed is wiped on unplug), so transporting the device together with the seed
  QR is only safe if a passphrase protects the seed. Minimal scope: derive the
  seed with `mnemonic.to_seed(passphrase)` in `wallet-core` (today it is
  hard-coded empty, `wallet.rs::from_bip39`) and add a hidden, double-entry
  passphrase prompt to the CLI signer's `read_key`; the QR request/reply
  protocol and the desktop watch-only flow are unchanged (they only see the
  public key). Design notes: any passphrase derives a valid but *different*
  key, so the signer's existing public-key display is the typo safeguard and
  must be compared with the paying account's key; BIP39's PBKDF2 (2048
  iterations) is weak stretching, so recommend a diceware-style passphrase
  (the EFF wordlist is already bundled for the vault-password generator); a
  forgotten passphrase is unrecoverable. Optional later: a passphrase field
  on the desktop mnemonic import for hot-wallet parity.

If time runs short, finish and verify NFTs first, then narrow multisig to the
smallest safe end-to-end flow. Hardware integration can begin after the parts
arrive. Mainnet, smart-contract support and production use remain out of scope
until separately reviewed and tested.

## Next steps
1. All Linux manual verification is complete.
2. (Done) Watch-only wallets and unsigned-transaction export.
3. QR-based offline transaction transport and a signer application: the
   desktop side is done (request QR, camera-scanned replies, multi-signature
   key-rotation replies) and the CLI signer works; still needed are the
   dedicated air-gapped device and animated/multi-part QR tuned to its camera
   and display.
4. (Done through stage 6; all stages merged into main and manually tested on
   Testnet with disposable accounts.) HTS tokens, in stages (hot wallets first):
   - Stage 1: token balances in Account assets and Send menu, and Token info lookup.
   - Stage 2: create and delete a fungible token. The wallet key is the treasury, admin and supply key; the fee cap for creation is 20 HBAR.
   - Stage 3: associate, dissociate and hide tokens.
   - Stage 4: send a token, with a receiver-association check.
   - Stage 5: mint and burn. Token management -> Issuing has Mint token and Burn token; the backend checks the supply key and treasury first.
   - Stage 6: signer support for watch-only wallets: token send, associate and disassociate go through the offline request and signer flow. Create, delete, mint and burn stay rejected by the signer by design.
   - Later: NFTs and multisig (scope agreed, see the timetable above) and
     hardware work. Key change is done (offline key rotation, 9 Nov).
5. Defer Mainnet until the implementation has been reviewed and tested.

## Cold signer (SeedSigner) design notes
Goal: a small, fully readable signer. It signs only what it can show in full and rejects everything else. Value is held cold; contracts, token management and experiments are done from small hot wallets.

**v1 whitelist (the device can display every field)**
1. HBAR transfer (`CryptoTransfer`, HBAR only).
2. Fungible HTS token transfer (same body). Show token ID and the raw amount; a token name or decimals from the QR is an unverified hint only.
3. NFT transfer (token ID and serial number).
   - **Done (10 Nov):** the CLI signer shows token ID, serial number, sender and
     recipient, and rejects NFT bodies that also move fungible amounts or HBAR,
     carry decimals, move several serials, or whose sender is not the payer.
4. Token association and dissociation.
5. Account create, update (key change) and delete. Show the full key (or key structure) being set.
   - **Done:** create (key shown in full, warning if it differs from the signing key), delete (recipient must be another account of the same watch-only wallet, one signature), and key change (two signatures via the multi-signature reply format, built and verified on Testnet 9 Nov).
   - **Still to do:** delete to a recipient with a different key (uses the same multi-signature reply format).
6. Later signer roadmap: `ScheduleCreate`/`ScheduleSign` for multisig collection
  over days. Schedule signing must not approve based only on a schedule ID and
  a coordinator-supplied description; define how the signer authenticates the
  scheduled contents before displaying and signing them. Allowance
  approve/delete remains optional later work.

**Rejected by default**
- Smart-contract calls and creation, and EVM transactions (opaque call data cannot be displayed).
- Token creation and management (create, mint, burn, freeze, wipe, update, fees, pause).
- Consensus service, file service, staking changes, node/system and batch transactions.
- Anything with custom fees or fields the parser does not recognise. Unknown means reject.
- Possible later: a clearly labelled "blind signing" mode, off by default.

**Parser rules**
- Show every line of a multi-party transfer, with a hard cap on the number of lines accepted.
- Check the whole body for fields beyond the whitelist (for example a key change or allowance hidden in a transfer) and reject if any exist.
- Fuzz the parser.

**Multisig**
- The device signs the same body bytes as for a single key, and returns one signature. The desktop app is the coordinator: it builds the body, collects signatures (QR transport), merges them and submits.
- The device must show the whole key structure (threshold and every public key) when an account is created or its key is changed.
- A transaction is valid for at most about 180 seconds from its start time, and the start time cannot be in the future. Signing across devices in different places therefore needs scheduled transactions, or all signers in one place within that window.
- Design the coordinator and signature records so a later schedule workflow can retain pending approvals across sessions, while keeping schedule-content authentication separate from ordinary transaction validation. Do not implement schedule transactions in this milestone.

**Planned workflow: three keys, two signatures, one device**
- Three independent keys are kept in separate places (three separate seeds, never derived from one mnemonic), with a 2-of-3 threshold account.
- The device is stateless. The desktop app shows the transaction as a QR; the device scans it and shows the summary. The user loads key A (scan its engraved SeedQR), the device signs and wipes the key from memory, then the same for key B. The device returns both signatures and the desktop app merges and submits.
- Load keys by SeedQR scan, not typed words, so both fit inside the 180-second window. Alternatively the desktop app sets the transaction start time only after the device reports ready.
- The desktop app sends the account's key structure with the transaction. The device derives the public key from each loaded seed, checks it is one of the listed keys, shows "key N of M matched", and refuses to sign with a key that is not part of the account.
- Optional per-key passphrase (a BIP39-style extra word), so a stolen plate alone is useless.
- Each key is backed up as a steel plate engraved with its SeedQR, generated on the device and shown only in the backup mode.
- Protects against the loss or compromise of one location. It does not protect against a compromised signer, because one device sees both keys in a session. That makes signed firmware, secure boot and a small readable codebase essential. A stricter mode for large amounts could have each key sign on a different device (2-3 devices are planned).

**Secrets never reach the screen or any export**
- The key type has no `Display`, `Debug`, `Serialize` or byte-access; only the signing function can use it.
- The UI crate must not depend on the key crate (checked in CI). Use TrustZone separation on the RP2350 where possible.
- The screen renders only whitelisted summary types. No generic text-from-buffer path, hex view or debug output in production.
- Seed/key backups are generated on the signer only, in a separate mode with its own PIN and warning, never reachable from the signing flow.
- Tests: a canary-key build that scans the framebuffer, QR output, SD writes and logs for key bytes; property tests; parser fuzzing; signed, reproducible firmware.

**Exports and engraving (later)**
- The signer exports QR images to microSD only; it does not generate toolpaths. A separate app on a network-disabled laptop imports those images and generates GRBL G-code for Universal Gcode Sender (UGS), which controls the CNC machine. Keep this tooling separate from the signer's small, auditable codebase.
- Secret backups are produced on the signer only; public engravings (addresses, watch-only keys) can come from the desktop app. Every export needs an explicit confirmation. Secret SeedQR images and any derived toolpaths, media, laptop storage, or CNC system handling them must be treated as secret.
- Print support comes last; writing a PBM/PNG/PDF to the SD card comes first.

**Code size and readability**
- Aim for a few thousand lines of our own code, few dependencies, `no_std`, no `unsafe` outside the hardware layer, and fewest concepts per function. Track the line count in CI.
- Each module gets a short walkthrough so it can be read line by line, starting with the signer core.

## Hardware bill of materials (signer prototype)

**The signer must never be connected to a computer** — no USB data connection
at any point in its life. Firmware updates and QR-image exports go through
microSD cards only, with separate cards for updates and exports. A
network-disabled laptop imports the QR images and runs the separate
QR-to-toolpath app and UGS; the signer never creates G-code. Secret SeedQR
images and any derived toolpaths, media, laptop storage, or CNC system handling
them must themselves be treated as secret.

| Part | Role | Status (9 Nov 2026) |
| --- | --- | --- |
| Raspberry Pi Pico 2 with headers (non-W, no Wi-Fi) | Signer MCU (RP2350) | Purchased |
| Pimoroni Pico Display Pack 2.8 | Display and buttons | Purchased |
| Adafruit 4682 microSD SPI breakout | Firmware updates and exports | Purchased |
| Arducam OV2640 camera | QR scanning | Purchased; confirm the exact model has an SPI interface before wiring |
| Miscellaneous (wiring, power, microSD cards) | Assembly | Believed on hand; verify against the wiring plan (14 Nov) |

No other parts have been purchased. Pin, power and physical compatibility
checks and the wiring plan are scheduled for 14 November; camera, display and
SD wiring, power, drivers and QR scanning have not yet been validated
together.

## Later roadmap
- [x] Integrate read-only Hedera Testnet access (balance and transaction history)
- [x] Implement and manually verify HBAR transfers and account create/move/delete on Testnet
- [x] Add watch-only wallets
- [x] Add unsigned-transaction export (HBAR sends, QR request, pasted signature, 180 s window). Manually verified end to end on Testnet with the example signer; reply can also be scanned by camera, added after this test
- [x] QR-based offline transaction transport, desktop side: request QR,
  camera-scanned replies and multi-signature key-rotation replies. Still to
  do: animated/multi-part QR tuned to the signer device's camera and display
  (tracked in the pre-Mainnet checklist).
- [ ] Create an offline signer application (first iteration done:
  `crates/offline-signer` CLI + `crates/signer-core`; it displays and signs
  HBAR transfers, account create/delete, key changes, token transfers, NFT
  transfers (token ID and serial number) and token associations, and can read
  a request from a screenshot or photo file; still needed: the dedicated
  air-gapped device with live camera scanning)
- [x] Add NFT support (implemented 10 Nov for the agreed scope: owned-NFT
  display, associate/dissociate, hot-wallet and watch-only transfers by
  serial number, the custom-fee Settings toggle and fee-schedule warnings,
  plus collection creation and minting for hot wallets (scope extension
  agreed 10 Nov); manual Testnet verification with disposable accounts is
  still to be done; deferred for later: custom fees at collection creation
  (tier 2) and burn by serial number (tier 3), see the NFT stage above;
  smart-contract calls stay rejected by the signer by design)
- [ ] Add scheduled-transaction support for multisig collection over days:
  `ScheduleCreate` and `ScheduleSign`, with strict signer-side validation of
  the scheduled action and a documented schedule-content authentication model.
- [ ] Design a reproducible hardware prototype

## Validation status
The source security reviews found no substantiated vulnerabilities. The
implemented desktop app includes the encrypted shared vault, wallet recovery
and backup, safety controls, account management, watch-only wallets, Testnet
HBAR and fungible-token transaction flows (token stages 1–6, including signer
support), NFT support (display, associate/dissociate, hot-wallet and
watch-only transfers, custom-fee gating), offline key rotation, and read-only
Mainnet watch-only monitoring.
Linux app-data permissions are hardened to `0700`/`0600`.

The current release is the x64 Windows build of `main` at commit `3140ed9`
(confirmed 9 Nov 2026); no version tags exist yet. A release app is rebuilt
after each change (`--no-bundle`); the last full installer build produced both
NSIS and MSI installers. The frontend suite stands at 118 passing UI tests, and
the Rust tests, formatting and workspace check pass on Windows and Ubuntu CI.
All manual Testnet token tests through stage 6 are complete with disposable
accounts, and the latest Linux check run passed on the Ubuntu desktop
partition. The NFT flows have automated tests but still need their manual
Testnet run. Remaining manual checks are listed above; do not use real funds or
recovery phrases.

Build note: this ARM64 Windows machine needs the x64 Rust toolchain
(`RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc`) and the x64 Visual Studio
environment to build the x64 app.

## Appendix: getting back to work after the chat break (4 Nov 2026)

This records what was found and done when a long-running AI evaluation stalled
(a session ran for over an hour and the app was closed) and work resumed on
HTS tokens. Use it to pick up cold.

### Where the work stood
- The previous chat's transcript could not be recovered (the session store had
  no history for it), but the workspace was intact. The state was rebuilt from
  the code, this TODO and the saved session notes under
  `C:\Users\ngt\.copilot\session-state\` (checkpoint files hold the build
  recipes below).
- Token stages (see Next steps, item 4): stage 1 done; stage 2 (create and
  delete) was built; stage 3 (associate, dissociate, hide) turned out to be
  already coded although this file listed it as not started; stage 4 (send) had
  no code.

### What was done
1. **Reviewed stage 3.** Protobuf fields are right (`token_association` 40,
   `token_dissociation` 41). The Tauri commands pre-check on the mirror node:
   associate needs an existing, non-deleted, fungible token not already held;
   dissociate needs a zero balance and refuses the treasury account. Hidden
   tokens are stored per wallet.
2. **Built stage 4, send a fungible token.**
   - `hedera-tx`: `TokenTransferList`, `UInt32Value` (`expected_decimals`, so a
     wrong scale is rejected by the network) and `token_transfer_body_bytes`,
     with a unit test. `CryptoTransferBody` gained `token_transfers` (field 2).
   - `submit.rs`: `send_testnet_token` (sender signs and pays, 1 HBAR fee cap,
     memo up to 100 bytes).
   - `src-tauri`: `send_token` command. It checks the token (exists, not
     deleted, fungible), the amount (above zero, within balance) and the
     recipient (exists on Testnet and has associated the token) before
     submitting, so a doomed transfer never costs a fee. It honours the
     address-book limit on external recipients.
   - UI: picking a token in the Send menu opens the send form as
     "Send <symbol>". Watch-only wallets get a notice (stage 6).
3. **Account assets now has a middle column** showing the token ID (left
   aligned, midway between asset and balance; balances are at the far right;
   the HBAR row leaves it empty).
4. **Menu names:** in Token management, "Associate token" (was "Add token") is association and
   "Disassociate token" (was "Remove token") is dissociation.
5. **Linux check script** `scripts/linux-token-check.sh`: fetches and checks
   out `feature/token-associate-hide`, sets aside local lockfile changes with
   `git stash` (npm rewrites them on that machine), then runs the Rust tests,
   `cargo check --workspace`, `cargo fmt --check` and the frontend lint and
   tests, and writes a log next to the script.

### Results
- Linux (Ubuntu): all checks passed (Rust tests, workspace check, formatting,
  frontend tests).
- Windows frontend: 85 of 85 UI tests pass.
- CI first failed on `cargo fmt --check` only (two spots in the new code),
  fixed in commit `303fca0`.
- A release app was built after each change. The tokens still need a manual
  Testnet test (see below).

### How to build and run on this machine (all discovered or confirmed)
- It is an ARM64 Windows machine with an x64 app target. Plain `cargo` fails
  with "linker `link.exe` not found" (default aarch64 toolchain), and the
  `arm64` or `arm64_x64` Visual Studio environments do not work either. Use:
  `$env:RUSTUP_TOOLCHAIN='stable-x86_64-pc-windows-msvc'`, then run through
  `cmd /c "call ...\BuildTools\VC\Auxiliary\Build\vcvars64.bat >nul && cd /d <repo> && <cmd>"`.
- Release app: `npm.cmd run tauri:build -- --target x86_64-pc-windows-msvc --no-bundle`
  from the repo root (about 90 seconds). The output is
  `target\x86_64-pc-windows-msvc\release\hbarnone.exe` (named
  `hedera-vault-app.exe` before the November 2026 rename). Close the app
  first or the exe is locked ("Access is denied").
- A stale exe is the usual reason a new feature seems to be missing: the check
  scripts only test, they do not rebuild the app.
- PowerShell blocks `npm.ps1` and `npx.ps1`; use `npm.cmd` and `npx.cmd`.
- `git` is not on PowerShell's PATH. Use
  `C:\Program Files\Git\cmd\git.exe` (or add that folder to PATH in the
  session).
- WSL and Python are not installed system-wide, but `py` launches
  Python 3.14 (with pillow and numpy installed). Node is available for
  helper scripts.

### Linux testing procedure
- Linux testing is done with a script that is copied to a USB stick, not typed
  in by hand. The stick appears at `/run/media/nick/AEAC-A758/`, the project is
  at `/home/nick/HederaVault` (folder name predates the rename; the scripts
  also look for `~/HbarNone`), and Rust and Node are installed.
- Scripts must have LF line endings (Windows CRLF breaks bash on line one).
- Run: `bash /run/media/nick/AEAC-A758/linux-token-check.sh`. The log
  `token-check-<date>.log` is written to the stick and copied back to
  `Documents` for reading. Always ask where things are before writing a script.
- The Linux machine had modified `package-lock.json` files and untracked
  backup and log folders; the script stashes only the two lockfiles.

### Git and CI workflow
- Commits carry a `Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>`
  trailer. This is a solo project: commits are made and pushed directly to
  `main` (confirmed 8 Nov 2026); feature branches and pull requests are no
  longer used.
- CI (`.github/workflows/ci.yml`) runs on Windows and Ubuntu. Failed job logs
  are one long line; the useful part is the `Diff in ...` blocks from
  `cargo fmt`.

### Cost discipline for future sessions
- Start a fresh session per substantial feature or related batch; use this TODO
  and Git history for handoff. Batch related small requests and keep general
  questions separate from implementation sessions.
- Search and read only relevant, bounded context; avoid repeated full-file
  reads, unnecessary tool chatter, and delegation without a concrete need.
- Use targeted tests while iterating, then run the appropriate final checks
  and build once; repeat only when failures or further changes require it.
- Close the app before rebuilding. Use `--no-bundle` for developer builds
  unless installers are requested; documentation-only changes need no rebuild.
- These practices reduce avoidable work, not guarantee a particular saving;
  local build time alone does not establish AI usage cost.

### Recently completed (8 Nov 2026)
- [x] Move-screen layout and per-signature camera scan (9 Nov): the watch-only
  move confirmation and signing boxes now span the full panel width, the
  request details sit beside the QR code, and each signature line pairs a scan
  button ("Scan current key's signature" / "Scan new key's signature") with
  its text box, so both replies can be read by camera. A pressed button reads
  "Stop scanning" and the camera opens as a layer over the signing box; a
  click anywhere outside it closes it. Other signing flows keep the single
  "Scan reply with camera" button.
- [x] Offline key rotation (built 9 Nov, manually verified on Testnet with QR
  codes 9 Nov):
  "Move account" now also works for watch-only wallets through a signing
  request (`CryptoUpdate`). The network needs the current key and the new key
  to sign: the app shows one request QR, the signer runs once per key, and
  the app accepts both replies (or co-signs itself when the target wallet's
  key is in the vault). The CLI signer displays and signs key changes with
  loud warnings. Fee preview was dropped from the pre-Mainnet checklist:
  Hedera fees are fixed in USD, so there is no fee-spike risk.
- [x] Mainnet watch-only verified by the user: a genuine Mainnet public key
  (DER form) imports, discovers its account and shows balances and history;
  the Mainnet view offers no send, token or account actions. The QR + offline
  signer flow was re-verified on Testnet with a watch-only wallet, and the
  deleted-token display/dissociation works.
- [x] Watch-only import accepts DER-encoded public keys (Ed25519 SPKI and
  secp256k1 forms) as exported by most Hedera tools; keys are stored
  normalized, so the duplicate-key guard works across formats.
- [x] Deleted tokens are shown in Account assets marked "(deleted)", excluded
  from Send/Delete/Mint/Burn, and can be dissociated whatever their balance.
- [x] Rename completed: productName, window title, installer names, Rust
  binary (`hbarnone.exe`), npm package, user agent, backup dialog labels and
  signer messages all say HBarNone. The bundle identifier stays
  `org.hederavault.desktop` so existing app-data is not relocated; changing it
  needs a migration step.
- [x] Merged remote feature branches deleted (hedera-testnet-readonly,
  signer-image-scan, tokens-menus).
- [x] Network separation: a topbar Testnet/Mainnet switch shows one network at
  a time. Wallets carry a `network` tag (existing wallets default to Testnet);
  accounts cannot be linked across networks, and every spending or signing
  command resolves its payer through a Testnet-only check, so Mainnet stays
  read-only. Mainnet wallets are watch-only (public key only; balances and
  history via the public Mainnet mirror node), and a key already in the vault
  on another network is refused at import, so Testnet keys never graduate to
  Mainnet. The home list now has a single Balance column.
- [x] `scripts/build-signers.ps1` rebuilds both offline signer executables
  (the app build does not rebuild them, and they are easy to lose since
  `target\` is not in git).
- [x] All manual Testnet token tests are complete through stage 6, using
  disposable accounts: stages 2–4 checklist (create and delete token with
  watch-only greyed out; associate with duplicate and invalid ID errors;
  dissociate zero and non-zero balance; hide and show persisting after
  restart; send to an associated account, to an unassociated account with the
  readable error and no fee, more than balance, decimals, watch-only notice),
  stage 5 mint and burn (Mint and Burn stay greyed out unless the account is
  the treasury of a token whose supply key is this wallet's key), and the
  stage 6 end-to-end signer test (watch-only
  wallet with the offline signer: associate, send, dissociate). Regression
  checks (HBAR send and Close button, Account assets layout) passed.
- [x] Stage 6 (token signer support) is merged into main: signer-core,
  hedera-tx offline builders, Tauri commands (prepare_unsigned_token_transfer,
  prepare_unsigned_token_association), UI and CLI signer display. Create,
  delete, mint and burn stay rejected by the signer by design.
- [x] UI polish: send error spacing, a Close button after a send (error or
  success), menu renamed to Associate token. Send section changes also apply
  to HBAR sends.
- [x] Linux test completed on the Ubuntu partition of the desktop machine.
  The test script was written on the Windows Surface machine and transported
  via micro SD card.

### Still to do
- Run the next Linux test once the next batch of features has been added
  (same procedure: script via micro SD to the Ubuntu desktop partition).
- Slow start after a quick relaunch (measured 8 Nov 2026): closing the app
  leaves its `msedgewebview2.exe` child processes draining for ~10–60 seconds
  while they hold the shared browser profile
  (`%LOCALAPPDATA%\org.hederavault.desktop\EBWebView`). A relaunch during that
  window shows its window immediately but the WebView cannot initialise until
  the old processes release the profile, so the user sees a white screen for
  7–10 seconds. A cold start is ~200 ms to the window and ~700 ms to content.
  Research the current Tauri 2 mitigation (explicitly closing the webview on
  exit, or a separate profile per instance) and fix.
- Branded startup splash: show the HBarNone logo (hexagon lockup on the app
  background) instead of a white screen while the WebView initialises and the
  vault state loads. Also softens the relaunch delay above until it is fixed.
- Offline signers (Windows, x64 build), both in `target\x86_64-pc-windows-msvc\release\`:
  `hedera-offline-signer.exe` is the proper test signer: shows every field, hides the key as typed, refuses to read it if the window cannot hide input. Use this for testing.
  `examples\sign_request.exe` is the older bare example (source `crates/hedera-tx/examples/sign_request.rs`); it echoes the private key as you type.
  Rebuild BOTH after any signer or hedera-tx change (the app build does not rebuild them):
  `cargo build --release -p offline-signer --target x86_64-pc-windows-msvc` and `cargo build --release -p hedera-tx --example sign_request --target x86_64-pc-windows-msvc`.
  Close any running copy first or the link fails (LNK1104).
- Air-gapped signer prototype hardware: see "Hardware bill of materials
  (signer prototype)" above. The Arducam OV2640 camera is now purchased, but
  its SPI interface still needs confirming before wiring. Initial
  provisioning and a trusted, verified SD-update mechanism still need
  designing.
- Tool approvals: the chat asks for permission often. This is an app setting
  (the approvals picker or `chat.tools.global.autoApprove`), not something the
  assistant can change.

## Pre-Mainnet launch checklist

Everything needed before the app is launched / used to transact on Mainnet.
Watch-only Mainnet monitoring is already live; this list is about trusting
the app near real value. Items marked (hardware) depend on the signer device.

### Core functionality
- [x] Offline key rotation: `CryptoUpdate` via signing request, so a
      watch-only wallet can move an account to a new key without the old or
      new key touching the desktop. Built and manually verified on Testnet
      with QR codes, 9 Nov 2026.
- [ ] QR transaction transport: animated/multi-part QR display of unsigned
      requests and camera scan of signed replies, tuned to the signer
      device's camera and display. (hardware)
- [ ] Air-gapped signer device: wiring, firmware, QR scan/display, SeedQR
      key entry, initial provisioning, verified SD-update mechanism. (hardware)
- [ ] Mainnet transactions through the signer only: Mainnet gRPC submit
      endpoints and node IDs, network-aware checksums in every parse path,
      network-aware signer validation and display, no stored Mainnet secrets.
- [ ] Mainnet outside entries in the address book (the book shows one
      network at a time, but outside entries are Testnet-only today).

### Security hardening
- [ ] Independent security audit (vault crypto, signing-request format,
      transaction encoding, signer parser); fix findings.
- [ ] Threat-model pass on the signing-request format: replay protection,
      expiry, node pinning, cross-network confusion.
- [ ] Secret-handling review: zeroization coverage, clipboard and screenshot
      hygiene, reveal-flow warnings.
- [ ] Supply-chain checks: cargo audit / npm audit in CI, dependency pinning
      review, signed release binaries (code-signing certificate), and a
      defined update/verification story for users.
- [ ] Vault password policy review (minimum strength at creation).

### Reliability and UX
- [ ] WebView relaunch delay fixed and branded startup splash (above).
- [ ] Graceful handling of mirror-node outages and partial failures
      (submitted-but-unconfirmed transactions, retry guidance).
- [ ] Backup/restore round-trip tests across app versions (vault format
      migration policy).
- [ ] Final branding/UX pass incl. Help text rewritten for Mainnet use.

### Testing and release
- [ ] Full manual test matrix per release on Windows and Linux (scripted
      checklists; Linux via the micro-SD procedure).
- [ ] Mainnet dress rehearsal: tiny real amounts end to end (fund, send,
      token associate/transfer, key rotation, delete) via the signer.
- [ ] CI enforcing fmt, lint, tests and audits on both platforms.
- [ ] Versioned release with changelog, checksums and signed installers.

## Appendix: Branding guidelines

### Colours
The three primary brand colours:

| Name       | Hex       | Usage                                                        |
| ---------- | --------- | ------------------------------------------------------------ |
| Dark       | `#2d3742` | All text (words not included in graphics)                    |
| Medium     | `#7087a5` | Borders of text boxes and buttons                            |
| Light      | `#a9b8ca` | Outer edges (borders) of large boxes that contain information (panels, dialogs); the boxes themselves are not shaded |
| Very light | `#e8f0fa` | Soft shading and accents (e.g., focus glow around text boxes) |

Exception: 'Danger' buttons and messages keep their own (red/warning)
colours and are not restyled with the brand palette.

### Typography
- The font is **Inter** (fallbacks: Segoe UI, Arial, sans-serif).
- Section titles are not bold (regular weight).

### Name
The project name is written exactly as **HBarNone** (capital H, capital B,
lowercase "ar", capital N).

## Appendix: Building on Windows

Prerequisites: Node.js, Rust (rustup) and the Visual Studio 2022 Build
Tools with the "Desktop development with C++" workload and a Windows SDK.

On an ARM64 Windows machine the default `aarch64-pc-windows-msvc`
toolchain cannot link (the Build Tools ship no arm64-target `link.exe`),
so everything is built with the x86_64 toolchain and target, which run
fine under emulation:

```
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup target add x86_64-pc-windows-msvc
```

If PowerShell blocks `npm`/`npx` with an execution-policy error, run the
Node commands from `cmd` instead.

Install dependencies (first time, or after a dependency change):

```
npm install
npm install --prefix desktop-ui
```

Run the tests:

```
cargo +stable-x86_64-pc-windows-msvc test --workspace --target x86_64-pc-windows-msvc
npm test --prefix desktop-ui
```

Build the release app and installers:

```
set RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-msvc
npm run tauri:build -- --target x86_64-pc-windows-msvc
```

To rebuild only the app itself (skipping the MSI/NSIS installers, which
saves time when just testing a change), append `--no-bundle`:

```
npm run tauri:build -- --target x86_64-pc-windows-msvc --no-bundle
```

The build writes:

- the app itself to
  `target\x86_64-pc-windows-msvc\release\hbarnone.exe` (run directly to
  test),
- an MSI installer to
  `target\x86_64-pc-windows-msvc\release\bundle\msi\`, and
- an NSIS installer to
  `target\x86_64-pc-windows-msvc\release\bundle\nsis\`.

For day-to-day development, `npm run tauri:dev` (with the same
`RUSTUP_TOOLCHAIN` set) starts the app against the Vite dev server with
hot reload.

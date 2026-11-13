# Wallet-first multisig manual testing

These are manual Testnet checks for the wallet-first multisig flow in
[MULTISIG-PLAN.md](./MULTISIG-PLAN.md). Use disposable keys and accounts only;
manual Testnet verification is still pending. Record outcomes and transaction
IDs without recording private keys or recovery phrases.

## Safety and setup

- Never use a real recovery phrase, private key, or account with real funds.
  Treat every test key as compromised and discard it after testing.
- Use the current desktop app, an unlocked disposable vault, the offline
  signer, a Testnet explorer, and at least one funded hot-wallet account in
  the vault.
- Have the signer and desktop ready before creating a request. Signing
  requests expire after 180 seconds; every required signer must reply within
  the same window.
- A multisig policy is local and immutable in HBarNone, not on-chain. A
  participant can still change an account's Hedera key elsewhere; the app
  must then flag the account and block its transaction controls.

## 1. Derive and scan public keys

1. Start `offline-signer`:
   `cargo +stable-x86_64-pc-windows-msvc run -p offline-signer --target x86_64-pc-windows-msvc --release`
2. Choose **2) derive a public key**, enter a disposable private key or phrase
   using the signer's hidden input, and verify that the displayed public key
   matches its `HVTX1-PUB:` QR.
3. In the vault, start creating a watch-only Testnet wallet, enter a name, and
   confirm **Scan to create wallet** is disabled while the name is blank.
4. Click **Scan to create wallet**, scan the signer's public-key QR, and verify
   the wallet appears with that key. The import screen must not offer a typed
   public-key field.
5. Repeat public-key derivation for two distinct participant keys. Do not
   import participant private keys into the vault.

## 2. Convert an empty wallet to a multisig policy

1. Open the empty watch-only wallet and choose **Wallet management → Convert
   to multisig**.
2. Choose three keys and a threshold of two. Confirm the threshold selector is
   disabled before choosing a key count and never offers a value greater than
   the key count.
3. Import the other two public keys by scanning their signer QR codes. The
   wallet's original key must be **Key A**; the scanned keys must appear as
   **Key B** and **Key C**. The remaining-key instruction counts down and
   changes to **Scan completed**.
4. Attempt to scan a duplicate. Expect **Duplicate key detected. Scan a
   different key**; the duplicate must not be added.
5. Create the multisig wallet. Verify the wallet view shows Ed25519,
   watch-only multisig, a 2-of-3 threshold, and all three public keys.
6. Confirm the structure cannot be edited or converted again. A watch-only
   wallet that already has an account must not offer conversion; if conversion
   is attempted, verify the specified existing-accounts warning.

## 3. Create the first account with a hot payer

1. From the multisig wallet choose **Wallet management → Create account**.
   Select a linked account of a hot wallet as the funder and enter a small
   opening balance.
2. Review the new account. Its key summary must say **3 keys with a threshold
   of 2**.
3. Click **Create signing request**. Verify the QR, three abbreviated key rows
   with unchecked boxes, a live expiry countdown, and the 2-of-3 signature
   requirement.
4. Scan the request with signer A, check its details, sign, then scan its
   reply with **Scan signatures** in the desktop app. Repeat with signer B.
   The corresponding checkboxes must tick; after one signature, creation
   stays disabled; after two, **Scan signatures** is disabled and **Create
   account** is enabled.
5. Create the account. The app reads the new account ID from the transaction
   record and links it automatically. Confirm it appears under the multisig
   wallet and that the hot payer was charged the opening balance and network
   fee.
6. If the account has enough balance, create a second account funded by the
   multisig wallet itself. Verify the same threshold signatures are needed
   for that payer.

## 4. Link only an exact policy match

1. From the multisig wallet choose **Wallet management → Link account by ID**.
   Enter a Testnet account ID and fetch its on-chain key structure.
2. Link an account controlled by the exact same 2-of-3 tree. Verify the
   complete structure is displayed before linking and the account appears in
   the wallet afterward.
3. Try a single-key account, a threshold account with different keys, a
   different threshold, and a different key order. Each mismatch must be
   visible and **Link this account** must remain disabled.

## 5. Sign an account operation

1. Open a linked multisig account and prepare a small HBAR transfer to a
   Testnet account permitted by the vault's external-send setting.
2. Confirm the signing screen uses the create-account layout: request QR on
   the left, abbreviated participant keys and checkboxes on the right, expiry
   and threshold progress below, then **Scan signatures**, **Send HBAR**,
   **Back**, and **Cancel**.
3. Scan replies from two distinct participants. A duplicate reply must not
   advance progress; a valid reply from an unrelated key must be rejected.
   The send button must remain disabled until two policy keys have signed.
4. Submit and verify the transaction and balance on Testnet. Repeat with the
   supported watch-only operations available in the account UI (token/NFT
   transfer, token association/dissociation, and account deletion) using
   disposable assets/accounts. Each must collect the same threshold before
   submission and show operation-specific details and button text.
5. **Move account** is currently unavailable for multisig wallets because the
   key-rotation flow still supports single-key wallets only.

## 6. Expiry and key mismatch

1. Create a signing request and let the 180-second window expire. Verify the
   timer reports expiry, scanning and submission are disabled, and only
   **Back** and **Cancel** remain available.
2. With a disposable linked account, rotate its on-chain key using separate
   Testnet tooling. Refresh the multisig wallet. It must show **Key mismatch**,
   display the read-only warning when the account is opened, and expose no
   transaction    controls. Restore the exact on-chain policy before using the account
   again; otherwise leave it flagged and do not transact.

## 7. Cleanup

Sweep remaining test funds where possible, delete disposable accounts only
when safe, and delete the test wallets. Deleting a multisig wallet removes
only the local policy; it does not delete network accounts or change their
keys. Preserve account IDs and policy keys until all accounts are handled.

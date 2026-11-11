# Multisig implementation plan (11–13 November milestone)

Scope agreed 9 Nov and recorded in [TODO.md](./TODO.md) ("11–13 November:
multisig" and the "Multisig" section of the cold-signer design notes). This
plan turns that scope into concrete stages grounded in the current code.

**Goal:** a 2-of-3 threshold Testnet account as the verified baseline, with
the threshold logic generic (M-of-N, matching Hedera's `ThresholdKey`) so
shapes such as 3-of-5 need no structural change. The desktop app is the
coordinator; the signer signs the same body bytes as for a single key and
returns one signature. Ordinary transactions only; schedule transactions stay
on the later roadmap and are treated as a distinct intent, not implemented
here.

## What already exists (do not rebuild)

- `hedera-tx::offline::collect_signatures` → `PartiallySigned`: verifies each
  `HVTX1-SIG:` reply against the exact request body bytes and the embedded
  signer public key, rejects duplicates and invalid signatures, merges
  `SignaturePair`s and submits. Used today by `submit_signed_move` for the
  two-signature key rotation.
- `PartiallySigned::sign_locally` co-signs with a vault key, and
  `seconds_remaining` reports the 180-second window.
- The `HVTX1-REQ:`/`HVTX1-SIG:` QR transport, camera-scanned replies and the
  multi-reply UI path (key rotation) on the desktop.
- The CLI signer (`signer-core` + `offline-signer`) signs whitelisted bodies
  and already shows the full (single) key for account create and key change.

## What is missing (the actual work)

1. `hedera-tx::Key` has only `ed25519` (tag 2) and `ecdsa_secp256k1` (tag 7);
   no `ThresholdKey` (tag 5) or `KeyList` (tag 6).
2. No key-tree type, no canonical validation, no generic M-of-N evaluation of
   collected signer keys against a key structure.
3. `hedera-mirror` parses only single keys (`single_key_hex`); accounts with
   `"_type": "ProtobufEncoded"` keys cannot be displayed or linked usefully.
4. No way to build a `CryptoCreate`/`CryptoUpdate` carrying a threshold key.
5. `signer-core` rejects a key change whose new key is not a single key and
   cannot display a key structure.
6. No UI to assemble a threshold key from wallet keys, show an account's key
   structure, or track signature progress against a threshold.
7. No data-model home for a multisig account (accounts currently belong to
   exactly one wallet key).

## Stage 1: key tree in `hedera-tx` (foundation, no UI)

**Status: done 11 Nov** — `hedera_tx::key_tree::KeyTree`, the protobuf
`ThresholdKey`/`KeyList` variants on `Key`, and
`PartiallySigned::satisfies`/`missing_keys`, all unit-tested.

- Extend the protobuf `Key` message with `threshold_key` (tag 5:
  `ThresholdKey { threshold: u32 (tag 1), keys: KeyList (tag 2) }`) and
  `key_list` (tag 6: `KeyList { keys: repeated Key (tag 1) }`). Prost handles
  the recursion with `Box`/`Option` as needed.
- Add a domain type, independent of transaction type:

  ```text
  enum KeyTree {
      Single { key_type, public_key_hex },
      Threshold { required: u32, keys: Vec<KeyTree> },  // KeyList = N-of-N
  }
  ```

  with:
  - encode to / decode from the protobuf `Key`, round-trip tested;
  - canonical validation: threshold in `1..=n`, at least one child, caps on
    total keys and nesting depth (suggest ≤ 10 keys, depth ≤ 2 — enough for
    2-of-3 and 3-of-5; anything deeper is rejected as "unknown means reject");
    duplicate public keys inside one list rejected;
  - `required_and_total()` and a plain-language description ("2 of 3: key A…,
    key B…, key C…") for UI and signer reuse;
  - `is_satisfied_by(&[signer_key_hex]) -> bool` — generic recursive threshold
    evaluation over lowercase hex public keys;
  - `contains_key(hex) -> bool` to reject unrelated signatures early.
- Keep validation, signature verification and aggregation separable: the key
  tree never touches `SignatureMap` internals; `PartiallySigned` gains only
  `fn satisfies(&self, tree: &KeyTree) -> bool` (delegating to
  `is_satisfied_by(self.signer_keys)`) and `fn missing_against(&self, tree)`
  for progress messages.
- Unit tests: round-trips, 2-of-3 and 3-of-5 evaluation, nested list inside a
  threshold, rejection of malformed trees (threshold 0, threshold > n, too
  deep, duplicate keys).

## Stage 2: mirror-node key structures

**Status: done 11 Nov** — `hedera-mirror` fetches the raw account key
(`parse_account_key`, `MirrorClient::account_key`), `KeyTree::from_mirror`
reads it, and the desktop exposes `get_account_key_structure` for linking.
Checked against the live Testnet mirror: account `0.0.50`'s large
`ProtobufEncoded` structure decodes and is cleanly rejected by the caps
(`parse_mirror_key` example). Lifting the single-key import restriction
awaits the stage-5 Testnet run.

- Parse an account's `"_type": "ProtobufEncoded"` key (hex of the protobuf
  `Key`) into the stage-1 tree; expose it on the account lookup used when
  linking by ID. Single keys keep the existing path.
- Reject (with a clear message) trees that fail canonical validation rather
  than displaying something partial.
- Note: the mirror's `account.publickey` filter matches keys inside threshold
  structures, so "find accounts for a key" can list multisig accounts a
  wallet key participates in; verify this on Testnet and lift the
  "single-key accounts only" restriction on import if it holds.
- Tests against captured mirror JSON fixtures, as the crate already does.

## Stage 3: account setup and key change (coordinator + signer)

- `hedera-tx`: let `CryptoCreateBody.key` and `CryptoUpdateBody.key` carry a
  full key tree (they already take a `Key`; the new variants make trees
  expressible). New builders or parameters: `build_create_request` /
  `build_key_update_request` accepting a `KeyTree`.
- Desktop flow, 2-of-3 baseline:
  - Build the threshold key from chosen participant public keys — any mix of
    vault wallet keys and pasted external public keys (validated hex, the
    design assumes three independent seeds, never one mnemonic).
  - Create: the funder pays and signs; Hedera does not require the new
    account's key to sign `CryptoCreate` (receiver-sig-required stays off).
    Hot-wallet create can submit directly; watch-only create goes through the
    request QR as today.
  - Key change to/from a threshold key: the old key must sign, and the new
    key must be satisfied (threshold of the new tree). Reuse the
    `submit_signed_move` multi-reply pattern: collect replies, check the old
    key present and `partial.satisfies(new_tree)`, co-sign locally with any
    vault-held participant keys, then submit.
- `signer-core`: extend its private `Key` mirror struct with tags 5/6 and the
  create/update screens to show the whole structure — the threshold and every
  public key in full, e.g. `2 OF 3`, then each key hex — as the design notes
  require. Cap accepted structure size to the stage-1 limits; anything beyond
  stays rejected. `MAX_REQUEST_HEX_CHARS` (2000) comfortably fits a 2-of-3 or
  3-of-5 body; confirm with a test and raise only if a real body demands it.
- The signer also warns, as it does today for create, when the signing key is
  not among the listed keys ("key N of M matched" / refuse to sign with an
  unrelated key — per the planned workflow).

## Stage 4: multisig account representation and transfer flow

- Data model: a multisig account cannot belong to one wallet key. Store it as
  a watch-only-style entry that records the account ID, network and its key
  tree (fetched from the mirror at link time and shown to the user for
  confirmation). Vault payload changes follow the existing versioned-payload
  pattern; legacy vaults are unaffected.
- Transfer (HBAR first; the body builders for tokens/NFTs are unchanged and
  can be enabled after the baseline is verified):
  - Prepare the same `HVTX1-REQ:` request as for a single-key watch-only
    account — the signer code path for the transfer itself is untouched.
  - Collect replies by paste or camera scan, reusing the multi-reply
    key-rotation UI. After each reply show progress against the key tree:
    matched keys, remaining requirement, and reject with specific messages:
    - duplicate signature (already rejected in `collect_signatures`);
    - unrelated signature — verified but from a key not in the tree
      (`contains_key` fails);
    - invalid signature — fails verification against the body bytes;
    - mismatched signature — a reply for a different request (same check:
      verification is bound to the exact body bytes and embedded key).
  - Enable submit only when `satisfies(tree)` holds, so a doomed transaction
    never costs a fee; show the countdown (`seconds_remaining`) and explain
    the 180-second limit: the start time cannot be in the future, so all
    signatures must be gathered within one window — signing across distant
    devices needs the later scheduled-transaction workflow.
- Keep the collection records designed so a later schedule workflow can
  persist pending approvals across sessions (replies are self-contained:
  request payload + reply strings), but do not implement persistence or
  schedules now.

## Stage 5: Testnet verification, docs, CI

- Automated tests:
  - stage-1 tree tests in `hedera-tx`;
  - `collect_signatures` + `satisfies` threshold paths, including duplicate /
    unrelated / mismatched replies;
  - mirror fixture tests for `ProtobufEncoded` keys;
  - signer tests: threshold-key create/update display and the reject paths;
  - frontend UI tests for the new flows (suite currently at 118).
- Manual Testnet run with disposable accounts, documented step by step:
  1. Create three disposable wallets (three independent seeds).
  2. Create the 2-of-3 account funded by a disposable hot wallet; verify the
     key structure on the mirror node.
  3. Transfer out with signatures A+B → success; repeat with A+C and B+C if
     time permits.
  4. Insufficient-signature case: one signature only — confirm the app blocks
     submission locally with a clear message (and optionally document one
     deliberate network `INVALID_SIGNATURE` for the record).
  5. Duplicate / unrelated / expired-window cases.
  6. Key change: rotate the 2-of-3 account to a new single key and back,
     verifying the old-key + new-tree signing rule.
  7. 3-of-5 end-to-end only if time permits (agreed 9 Nov).
- Document recovery and limitations (loss of one key is survivable; loss of
  two is not; the 180-second constraint; one device seeing two keys in a
  session) and update TODO.md checkboxes and the README walkthrough.
- `cargo fmt`, tests and the workspace check pass on Windows and Ubuntu CI.

## Order of work and fallback

Stages 1 → 2 → 3 → 4 → 5; stages 1–2 are pure library work and merge safely
on their own. If time runs short (per TODO), the smallest safe end-to-end
flow is: stage 1, mirror key display (stage 2), linking an existing
externally created 2-of-3 account, and the stage-4 transfer with threshold
checking — account *setup* through the app (stage 3) degrades gracefully to
"create the account with external tooling, link and verify it here".

## Out of scope (explicit)

- `ScheduleCreate`/`ScheduleSign` and any persistence of pending approvals
  (later roadmap; schedule-content authentication is a separate design).
- Mainnet, smart contracts, and signer hardware work.
- Nested key shapes beyond the stage-1 caps; contract-ID and RSA key
  variants remain rejected ("unknown means reject").

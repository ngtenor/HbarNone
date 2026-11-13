# Multisig manual testing instructions (stage 5)

The step-by-step Testnet verification of the multisig implementation
([MULTISIG-PLAN.md](./MULTISIG-PLAN.md), stages 1–4). Follow the steps in
order; later tests reuse what earlier ones create. Record the outcome of each
numbered test (pass/fail, transaction IDs, surprises) as you go.

**Safety rules**
- Testnet only. Use disposable wallets and accounts created for this run.
  Never enter a real recovery phrase or key, here or in the signer.
- The CLI signer shows secrets briefly in your hands (pasted keys). Treat
  every key in this run as burned afterwards.
- Expect each transfer to cost a tiny Testnet fee; fund generously (a portal
  Testnet account starts with 1000 ℏ).

**You will need**
- The desktop app (current `main`), unlocked with a disposable vault.
- The CLI signer:
  `cargo +stable-x86_64-pc-windows-msvc run -p offline-signer --target x86_64-pc-windows-msvc --release`
  (two terminal windows ready saves time inside the 180-second window).
- A funded disposable Testnet account ("funder") in a hot wallet of the
  vault (create one at the Hedera portal and link it, as in earlier runs).
- A mirror-node explorer (hashscan.io on Testnet) for cross-checks.

## Setup: three independent keys

1. Create three hot wallets in the vault from the **+** tab: `A`, `B`, `C`.
   Each gets its own recovery phrase — three independent keys, never derived
   from one mnemonic.
2. For `A` and `B`, reveal and copy each wallet's private key (wallet menu →
   reveal private key) into a scratch buffer for the signer runs. (Faster to
   paste than a 24-word phrase inside the time window; these are disposable.)
3. In **Settings**, enable external sends if the transfer recipient will not
   be in the address book.

## Test 1: create the 2-of-3 account

1. Home tab → **Shared accounts** → **Create a shared account**.
2. Funding account: the funder. Opening balance: `20` HBAR.
3. Threshold: `2`. Add the three participant keys with the
   "Add a vault wallet's key..." dropdown (`A`, `B`, `C`), one per line.
4. Name it (for example `Ours`) and click **Create account**.
5. Expected: "Submitted. Waiting for Testnet to confirm..." then
   "Created shared account 0.0.X-xxxxx." and a row showing **2 of 3 keys**.
6. Click **Show keys**: the structure must list exactly the three keys with
   threshold 2. Cross-check the account's key on hashscan (it shows a
   threshold key with the same three public keys).
7. Click **Refresh balance**: 20 ℏ.

All transaction flows now estimate the full fee using the Testnet mirror's
state-aware fee endpoint, convert USD tinycents to tinybars using the network
exchange rate, and round up a 25% buffer. Fixed 1/5/20 HBAR ceilings are removed.
Hot-wallet operations sign and submit automatically within that maximum,
without a separate fee dialog. Only an `INSUFFICIENT_TX_FEE` node precheck
with a quote above the buffered maximum asks for approval to re-sign at the
quoted fee and retry once. Declining must prevent that retry. Missing quotes,
quotes at or below the maximum, other errors, timeouts and a second rejection
must not trigger a retry or further fee dialogs.
Offline and shared signing requests carry the signed maximum in their payload
but display no fee information; neither does the offline signer.
An estimation or exchange-rate error must prevent signing/submission.
The funding account pays the actual fee in addition to the opening balance.
Only the explicitly approved precheck fee increase above is retried.
A consensus failure is reported immediately with the transaction ID; it is
not treated as pending confirmation or automatically resubmitted. Failed
consensus transactions may still incur fees.

The UI shows a submitting status while waiting for the node (connection
timeout 10 seconds, RPC timeout 20 seconds), followed by mirror
confirmation only after submission succeeds. Names are local labels, not
part of the creation transaction; repeating a name is allowed and creates
a separate account. If a submission times out, check the funder's transactions
on the mirror before creating again to avoid accidentally funding two accounts.

Reported during manual testing: the first creation appeared to succeed;
a later creation waited about 30 seconds and returned `INSUFFICIENT_TX_FEE`.
The first patch retried only precheck fee rejections, still initially offering
1 HBAR. Retesting returned **"The account creation failed: INSUFFICIENT_TX_FEE"**,
which is a final consensus result, not a precheck rejection. That path never
used the permitted 5 HBAR cap and unnecessarily retried confirmation polling.
An intermediate correction offered 5 HBAR from the outset. The current policy
replaces that fixed ceiling with the network estimate plus 25% and still stops
polling final failures. Signature-count thresholds are unchanged.
Repeat this test with the updated app and record transaction IDs and actual
fees; live Testnet verification is pending.

## Test 2: link and unlink by ID

1. Click **Forget** on the new account, then **Link a shared account**, type
   its ID (no checksum) and click **Fetch key structure**.
2. Expected: "This account is controlled by 2 of 3 keys" and the full
   structure, shown *before* anything is stored. Verify every key, then
   **Link this account**.
3. Also try linking the funder's account ID the same way. Expected error:
   it uses a single key and must be linked to a wallet instead.

## Test 3: transfer with 2 of 3 signatures (the baseline)

1. On the shared account, click **Send HBAR**. Recipient: the funder's
   account **with checksum** (`0.0.X-abcde`). Amount: `1`. Click
   **Create signing request**.
2. The request QR appears with a countdown. Within the window:
   - Signer window 1: paste the request text (expand "Show request as
     text"), check the summary (From = shared account, To, Amount), load
     key `A` (option 2, private key hex), type `yes`, copy the
     `HVTX1-SIG:` reply.
   - Desktop: paste the reply, **Add signature**. Expected: "1 reply
     collected; 1 of the required 2 keys have signed." **Submit stays
     disabled.**
   - Signer window 2: same request, key `B`, copy the reply.
   - Desktop: **Add signature**. Expected: "The threshold is satisfied."
     and **Submit** becomes enabled.
3. Click **Submit**. Expected: "Transfer submitted: <id>", and after a
   refresh the balance dropped by 1 ℏ plus a small fee. Verify the
   transaction on hashscan (two signatures).
4. If time permits, repeat once with `A`+`C` and once with `B`+`C`.

## Test 4: insufficient signatures

1. Start a new transfer, add only key `A`'s reply.
2. Expected: Submit is disabled — the app blocks a doomed transaction
   locally, before any fee. Cancel the request.
3. (Optional, for the record) To see the network's own answer once, modify
   nothing in the app; instead note that the app never submits below the
   threshold by design, so the network `INVALID_SIGNATURE` path cannot be
   reached from the UI.

## Test 5: duplicate signature

1. Start a new transfer, add key `A`'s reply twice (paste the same reply
   again and click **Add signature**).
2. Expected: "That key has already signed." — the count stays at
   "1 of the required 2", and Submit stays disabled.

## Test 6: unrelated signature

1. In the same request, sign with the **funder's** key (not a participant)
   in the signer and add that reply.
2. Expected error naming the key: "A signature was made with a key that is
   not part of this account: ...". The reply is not kept.

## Test 7: invalid and mismatched signatures

1. Paste a tampered reply (change the last character of a valid reply).
   Expected: "This is not a valid signature from a signer."
2. Create a request, sign it, **Cancel**, create a *new* request for the
   same account, and add the old reply. Expected: the same rejection — a
   reply is bound to the exact signing bytes of its own request.

## Test 8: the 180-second limit

1. Create a request and wait out the countdown without signing.
2. Expected: "The request has expired. Start again." — Add signature and
   Submit are disabled. This is the documented limit: the start time cannot
   be in the future and the window is at most 180 seconds, so all signers
   must sign within one window (distant signers need the future scheduled-
   transaction workflow).

## Test 9: convert an existing account to 2-of-3

1. Create a fresh disposable account owned by wallet `C` (wallet tab →
   Account management → Create account, funded by the funder).
2. **Shared accounts** → **Convert an account**. Choose `C`'s new account,
   threshold `2`, add keys `A`, `B`, `C`. Click **Review key change**.
3. Expected: the full new structure is shown. Since the current key (`C`)
   and all three participant keys live in this vault, the vault co-signs
   everything: click **Submit** with no replies.
4. Expected: "Waiting for Testnet to confirm the key change..." then the
   account moves out of wallet `C` into the shared list. Verify the key on
   hashscan and send 1 ℏ out of it with two signer replies (repeat the
   Test 3 steps) to prove control.
5. (Variant, if time permits) Convert an account whose participants are
   *not* all vault wallets: paste one external public key. Then the convert
   submit must refuse until enough signer replies are added.

## Test 10: participant key search on the mirror

1. Open wallet `A`'s tab and run "find accounts by key".
2. Expected: the mirror may list the shared account (its `account.publickey`
   filter matches keys inside threshold structures). The app must **skip**
   it — it must not appear as one of wallet `A`'s own accounts — while any
   single-key accounts of `A` still import. (Guarded in
   `add_wallet_accounts_and_save`; record what the mirror returned.)

## Test 11: signer display checks

While running the earlier tests, confirm on the CLI signer:
1. A transfer from the shared account looks like any transfer: the signer
   signs the same body bytes and returns one signature; the payer is the
   shared account (compare checksums with the app).
2. The convert request (Test 9 variant) shows `THRESHOLD: any 2 of these 3
   keys must sign` and **every key in full**, warns that the current key
   plus 2 new keys must sign, and when signing with a participant key says
   "key N of 3". With a non-participant key it says the key is not in the
   new structure (the current key's signature).
3. A request whose structure is nested or lists more than 10 keys is
   rejected (needs a hand-built request; optional, covered by unit tests).

## Recovery and limitations (document in the TODO after the run)

- Losing one of the three keys is survivable: any two remaining keys can
  rotate the account to a fresh structure (via the key-change request) or
  sweep the funds. Losing two keys is unrecoverable.
- All signatures must land within one 180-second window; collection across
  days needs `ScheduleCreate`/`ScheduleSign` (later roadmap).
- The UI converts an account *to* a structure but does not yet rotate a
  shared account back to a single key, delete a shared account, or send
  tokens/NFTs from one (HBAR transfers only in this milestone).
- One machine coordinated and one signer saw two keys in each session; the
  independence of the three keys is organisational here, not physical.
  Separate devices per key remain the hardware goal.

## Cleanup

1. Sweep remaining HBAR from both shared accounts back to the funder
   (2 signatures each).
2. **Forget** the shared accounts; delete the disposable wallets and the
   test accounts you can delete; burn the scratch buffer with the keys.

## After the run

- Tick the multisig checkboxes in [TODO.md](./TODO.md) that the run proved,
  and note any deviations in this file next to the test concerned.
- Update the stage-5 status in [MULTISIG-PLAN.md](./MULTISIG-PLAN.md).

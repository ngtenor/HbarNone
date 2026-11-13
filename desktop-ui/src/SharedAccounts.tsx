import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { QrScanner } from './QrScanner'
import type { WalletSummary } from './types'

// Shared (multisig) accounts: linking, creation, conversion and transfers.
// The desktop app is the coordinator: it builds one request, collects signer
// replies, checks them against the account's key structure and submits only
// once the threshold is satisfied.

const TINYBARS_PER_HBAR = 100_000_000
const POLL_ATTEMPTS = 20

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms))

// The mirror node trails consensus by a few seconds, so bookkeeping retries.
async function retry<T>(
  task: () => Promise<T>,
  attempts: number,
  retryable?: (failure: unknown) => boolean,
): Promise<T> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await task()
    } catch (failure) {
      if (attempt >= attempts || (retryable && !retryable(failure))) throw failure
      await sleep(3000)
    }
  }
}

function formatHbar(tinybars: number) {
  return (tinybars / TINYBARS_PER_HBAR).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 8,
  })
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

export type SharedAccountView = {
  id: string
  displayId: string
  name: string
  named: boolean
  required: number
  total: number
  keys: string[]
  description: string
}

type AccountKeyStructure = {
  description: string
  required: number
  total: number
  keys: string[]
}

type SigningRequest = {
  payload: string
  qrSize: number
  qrModules: boolean[]
  action: string
  sender: string
  receiver: string
  newKey: string
  keyStructure: string[]
  tinybars: number
  memo: string
  expiresAtSeconds: number
}

type SignatureProgress = {
  signedKeys: string[]
  required: number
  total: number
  matched: number
  satisfied: boolean
}

const QUIET_ZONE = 4

function QrCode({ request }: { request: SigningRequest }) {
  const span = request.qrSize + QUIET_ZONE * 2
  return (
    <svg
      className="qr-code"
      role="img"
      aria-label="Signing request QR code"
      viewBox={`0 0 ${span} ${span}`}
      shapeRendering="crispEdges"
    >
      <rect width={span} height={span} fill="#fff" />
      {request.qrModules.map((dark, index) =>
        dark ? (
          <rect
            key={index}
            x={(index % request.qrSize) + QUIET_ZONE}
            y={Math.floor(index / request.qrSize) + QUIET_ZONE}
            width="1"
            height="1"
            fill="#000"
          />
        ) : null,
      )}
    </svg>
  )
}

function StructureLines({ description }: { description: string }) {
  return (
    <pre className="key-text shared-structure" aria-label="Key structure">
      {description}
    </pre>
  )
}

type Props = {
  wallets: WalletSummary[]
  onWalletsRefresh: () => void
}

type Flow =
  | { kind: 'link' }
  | { kind: 'create' }
  | { kind: 'convert' }
  | { kind: 'send'; account: SharedAccountView }

export function SharedAccounts({ wallets, onWalletsRefresh }: Props) {
  const [accounts, setAccounts] = useState<SharedAccountView[]>([])
  const [balances, setBalances] = useState<Record<string, number>>({})
  const [flow, setFlow] = useState<Flow | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [isBusy, setIsBusy] = useState(false)
  const [detailsFor, setDetailsFor] = useState('')

  // Link form
  const [linkId, setLinkId] = useState('')
  const [linkName, setLinkName] = useState('')
  const [preview, setPreview] = useState<AccountKeyStructure | null>(null)

  // Create and convert forms
  const [funder, setFunder] = useState('')
  const [openingBalance, setOpeningBalance] = useState('')
  const [convertSource, setConvertSource] = useState('')
  const [requiredInput, setRequiredInput] = useState('2')
  const [keysInput, setKeysInput] = useState('')
  const [keyWallet, setKeyWallet] = useState('')

  // Send form and signature collection
  const [recipient, setRecipient] = useState('')
  const [amount, setAmount] = useState('')
  const [memo, setMemo] = useState('')
  const [signing, setSigning] = useState<SigningRequest | null>(null)
  const [signatures, setSignatures] = useState<string[]>([])
  const [progress, setProgress] = useState<SignatureProgress | null>(null)
  const [reply, setReply] = useState('')
  const [scanning, setScanning] = useState(false)
  const [secondsLeft, setSecondsLeft] = useState(0)
  // What the collected signatures belong to: a transfer, or a conversion.
  const [signingFor, setSigningFor] = useState<'send' | { walletId: string; accountId: string } | null>(null)

  useEffect(() => {
    invoke<SharedAccountView[]>('list_shared_accounts')
      .then(setAccounts)
      .catch((failure) => setError(errorMessage(failure)))
  }, [])

  useEffect(() => {
    if (!signing) {
      setScanning(false)
      return
    }
    const tick = () =>
      setSecondsLeft(Math.max(0, signing.expiresAtSeconds - Math.floor(Date.now() / 1000)))
    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [signing])

  const run = async (task: () => Promise<void>) => {
    setError('')
    setNotice('')
    setIsBusy(true)
    try {
      await task()
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setIsBusy(false)
    }
  }

  const resetForms = () => {
    setLinkId('')
    setLinkName('')
    setPreview(null)
    setFunder('')
    setOpeningBalance('')
    setConvertSource('')
    setRequiredInput('2')
    setKeysInput('')
    setKeyWallet('')
    setRecipient('')
    setAmount('')
    setMemo('')
    setSigning(null)
    setSignatures([])
    setProgress(null)
    setReply('')
    setSigningFor(null)
  }

  const openFlow = (next: Flow | null) => {
    resetForms()
    setError('')
    setNotice('')
    setFlow(next)
  }

  const testnetWallets = wallets.filter((wallet) => wallet.network === 'testnet')
  const hotAccounts = testnetWallets
    .filter((wallet) => wallet.secret_kind !== 'watchonly')
    .flatMap((wallet) => wallet.accounts.map((account) => ({ wallet, account })))
  const allAccounts = testnetWallets.flatMap((wallet) =>
    wallet.accounts.map((account) => ({ wallet, account })),
  )

  const keyList = () =>
    keysInput
      .split('\n')
      .map((line) => line.trim())
      .filter(Boolean)

  // Quiet refresh: used after a submit, so it must not clear the notice.
  const refreshBalance = async (accountId: string) => {
    try {
      const overview = await invoke<{ balance_tinybars: number } | null>('get_shared_overview', {
        accountId,
      })
      if (overview) {
        setBalances((existing) => ({ ...existing, [accountId]: overview.balance_tinybars }))
      }
    } catch (failure) {
      setError(errorMessage(failure))
    }
  }

  const handlePreview = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    return run(async () => {
      const structure = await invoke<AccountKeyStructure | null>('get_account_key_structure', {
        accountId: linkId.trim(),
      })
      if (!structure) {
        setError('The account is unknown on Testnet.')
        return
      }
      if (structure.total < 2) {
        setError('This account uses a single key. Link it to a wallet above instead.')
        return
      }
      setPreview(structure)
    })
  }

  const handleLink = () =>
    run(async () => {
      const added = await invoke<SharedAccountView>('add_shared_account', {
        accountId: linkId.trim(),
        name: linkName.trim() || null,
      })
      setAccounts((existing) => [...existing, added])
      openFlow(null)
      setNotice(`Linked shared account ${added.displayId}.`)
    })

  const addWalletKey = () =>
    run(async () => {
      if (!keyWallet) return
      const keys = await invoke<{ publicKey: string }>('get_wallet_keys', { walletId: keyWallet })
      setKeysInput((existing) => (existing ? `${existing}\n${keys.publicKey}` : keys.publicKey))
    })

  const handleCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    return run(async () => {
      setNotice('Submitting account creation to Testnet...')
      const result = await invoke<{ transactionId: string }>('create_shared_account', {
        fromAccount: funder,
        initialHbar: openingBalance.trim(),
        required: Number(requiredInput),
        keys: keyList(),
      }).catch((failure: unknown) => {
        setNotice('')
        throw failure
      })
      setNotice('Submitted. Waiting for Testnet to confirm the new account...')
      const added = await retry(
        () =>
          invoke<SharedAccountView>('finish_shared_create', {
            transactionId: result.transactionId,
            name: linkName.trim() || null,
          }),
        POLL_ATTEMPTS,
        (failure) => [
          'Testnet has not confirmed the creation yet.',
          'The account is unknown on Testnet.',
        ].includes(errorMessage(failure)),
      ).catch((failure: unknown) => {
        setNotice('')
        throw failure
      })
      setAccounts((existing) => [...existing, added])
      openFlow(null)
      setNotice(`Created shared account ${added.displayId}.`)
    })
  }

  const handleConvert = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const source = allAccounts.find(
      ({ wallet, account }) => `${wallet.id}/${account.id}` === convertSource,
    )
    if (!source) {
      setError('Choose the account to convert.')
      return
    }
    return run(async () => {
      const request = await invoke<SigningRequest>('prepare_shared_key_change', {
        walletId: source.wallet.id,
        accountId: source.account.id,
        required: Number(requiredInput),
        keys: keyList(),
      })
      setSigning(request)
      setSigningFor({ walletId: source.wallet.id, accountId: source.account.id })
      setSignatures([])
      setProgress(null)
    })
  }

  const handleSend = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (flow?.kind !== 'send') return
    const account = flow.account
    return run(async () => {
      const request = await invoke<SigningRequest>('prepare_shared_transfer', {
        accountId: account.id,
        recipient: recipient.trim(),
        amountHbar: amount.trim(),
        memo,
      })
      setSigning(request)
      setSigningFor('send')
      setSignatures([])
      setProgress(null)
    })
  }

  // Each reply is verified the moment it is added: invalid, mismatched or
  // unrelated signatures are refused with the backend's message, and a
  // repeated signature counts only once.
  const addReply = () =>
    run(async () => {
      if (!signing) return
      const next = [...signatures, reply.trim()]
      if (signingFor === 'send') {
        const checked = await invoke<SignatureProgress>('check_shared_signatures', {
          request: signing.payload,
          responses: next,
        })
        if (progress && checked.signedKeys.length === progress.signedKeys.length) {
          setNotice('That key has already signed.')
        }
        setProgress(checked)
      }
      setSignatures(next)
      setReply('')
    })

  const handleScanned = (text: string) => {
    setReply(text)
    setScanning(false)
  }

  const handleSubmitCollected = () =>
    run(async () => {
      if (!signing || !signingFor) return
      if (signingFor === 'send') {
        const result = await invoke<{ transactionId: string }>('submit_shared_transfer', {
          request: signing.payload,
          responses: signatures,
        })
        const sender = flow?.kind === 'send' ? flow.account.id : ''
        openFlow(null)
        setNotice(`Transfer submitted: ${result.transactionId}`)
        if (sender) void refreshBalance(sender)
        return
      }
      const { walletId, accountId } = signingFor
      await invoke<{ transactionId: string }>('submit_shared_key_change', {
        walletId,
        request: signing.payload,
        responses: signatures,
      })
      setNotice('Submitted. Waiting for Testnet to confirm the key change...')
      const added = await retry(
        () =>
          invoke<SharedAccountView>('finish_shared_convert', {
            walletId,
            accountId,
            name: linkName.trim() || null,
          }),
        POLL_ATTEMPTS,
      )
      setAccounts((existing) => [...existing, added])
      onWalletsRefresh()
      openFlow(null)
      setNotice(`Account ${added.displayId} is now a shared account.`)
    })

  const handleForget = (account: SharedAccountView) =>
    run(async () => {
      await invoke('remove_shared_account', { accountId: account.id })
      setAccounts((existing) => existing.filter((item) => item.id !== account.id))
      setNotice(`Stopped watching ${account.displayId}. Nothing changed on the network.`)
    })

  const thresholdFields = (
    <>
      <label htmlFor="shared-required">Signatures needed (threshold)</label>
      <input
        id="shared-required"
        type="number"
        min={1}
        max={10}
        value={requiredInput}
        onChange={(event) => setRequiredInput(event.target.value)}
      />
      <label htmlFor="shared-keys">Participant public keys (one per line)</label>
      <textarea
        id="shared-keys"
        rows={4}
        autoComplete="off"
        spellCheck={false}
        placeholder="64 (Ed25519) or 66 (ECDSA) hexadecimal characters per line"
        value={keysInput}
        onChange={(event) => setKeysInput(event.target.value)}
      />
      <div className="signature-row">
        <select
          aria-label="Wallet whose key to add"
          value={keyWallet}
          onChange={(event) => setKeyWallet(event.target.value)}
        >
          <option value="">Add a vault wallet&apos;s key...</option>
          {testnetWallets.map((wallet) => (
            <option key={wallet.id} value={wallet.id}>
              {wallet.name}
            </option>
          ))}
        </select>
        <button
          className="secondary-button"
          type="button"
          disabled={isBusy || !keyWallet}
          onClick={addWalletKey}
        >
          Add this wallet&apos;s key
        </button>
      </div>
    </>
  )

  // The request QR plus the signature collection list, used by transfers and
  // conversions. Submit stays disabled until the threshold is satisfied (for
  // a transfer) so a doomed transaction never costs a fee.
  const collector = signing && (
    <div className="testnet-confirm" role="group" aria-label="Shared signing request">
      <p>
        Let each participant scan this QR code with their signer and add every
        reply below. A transaction is valid for at most 180 seconds, so all
        signatures must be collected within that window.
      </p>
      <QrCode request={signing} />
      <p aria-live="polite">
        {secondsLeft > 0 ? `Time left: ${secondsLeft} seconds` : 'The request has expired. Start again.'}
      </p>
      <dl className="confirm-grid">
        {signingFor === 'send' ? (
          <>
            <dt>From</dt>
            <dd>{signing.sender}</dd>
            <dt>To</dt>
            <dd>{signing.receiver}</dd>
            <dt>Amount</dt>
            <dd>{formatHbar(signing.tinybars)} ℏ</dd>
            <dt>Memo</dt>
            <dd>{signing.memo || '(none)'}</dd>
          </>
        ) : (
          <>
            <dt>Account</dt>
            <dd>{signing.sender}</dd>
            <dt>New key structure</dt>
            <dd>
              <StructureLines description={signing.keyStructure.join('\n')} />
            </dd>
          </>
        )}
      </dl>
      <details>
        <summary>Show request as text</summary>
        <textarea readOnly rows={4} aria-label="Signing request text" value={signing.payload} />
      </details>
      {signatures.length > 0 && (
        <p aria-live="polite">
          {signatures.length} {signatures.length === 1 ? 'reply' : 'replies'} collected
          {progress
            ? `; ${progress.matched} of the required ${progress.required} keys have signed.`
            : '.'}
        </p>
      )}
      {progress?.satisfied && (
        <p role="status">The threshold is satisfied. The transfer can be submitted.</p>
      )}
      <label htmlFor="shared-reply">Signer reply</label>
      {scanning ? (
        <QrScanner prefix="HVTX1-SIG:" onScan={handleScanned} onClose={() => setScanning(false)} />
      ) : (
        <button
          className="secondary-button"
          type="button"
          disabled={isBusy || secondsLeft === 0}
          onClick={() => setScanning(true)}
        >
          Scan reply with camera
        </button>
      )}
      <textarea
        id="shared-reply"
        rows={3}
        autoComplete="off"
        spellCheck={false}
        placeholder="HVTX1-SIG:..."
        value={reply}
        onChange={(event) => setReply(event.target.value)}
      />
      <div className="button-pair">
        <button
          className="secondary-button"
          type="button"
          disabled={isBusy || secondsLeft === 0 || !reply.trim()}
          onClick={addReply}
        >
          Add signature
        </button>
        <button
          className="primary-button"
          type="button"
          disabled={
            isBusy
            || secondsLeft === 0
            || (signingFor === 'send' && !progress?.satisfied)
          }
          onClick={handleSubmitCollected}
        >
          {isBusy ? 'Submitting...' : 'Submit'}
        </button>
        <button
          className="secondary-button"
          type="button"
          disabled={isBusy}
          onClick={() => openFlow(null)}
        >
          Cancel
        </button>
      </div>
    </div>
  )

  return (
    <section className="panel settings-panel shared-accounts" aria-labelledby="shared-accounts-heading">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">MULTISIG</p>
          <h2 id="shared-accounts-heading">Shared accounts</h2>
        </div>
      </div>
      <p className="dialog-copy">
        A shared account is controlled by several keys with a threshold (for
        example 2 of 3). The app builds one request, collects the signers&apos;
        replies and submits once enough keys have signed.
      </p>
      {error && <p className="error-message" role="alert">{error}</p>}
      {notice && <p className="field-hint" role="status">{notice}</p>}
      {accounts.length === 0 && flow === null && (
        <p className="empty-state">No shared accounts yet.</p>
      )}
      {accounts.length > 0 && (
        <ul className="wallet-list shared-account-list">
          {accounts.map((account) => (
            <li className="wallet-item" key={account.id}>
              <div className="account-row">
                <span className="account-id">
                  <strong>{account.named ? `${account.name} - ` : ''}{account.displayId}</strong>
                  <span> {account.required} of {account.total} keys</span>
                </span>
                <span className="account-balance">
                  {balances[account.id] !== undefined ? `${formatHbar(balances[account.id])} ℏ` : ''}
                </span>
              </div>
              <div className="button-pair">
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={() => openFlow({ kind: 'send', account })}
                >
                  Send HBAR
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={() => run(() => refreshBalance(account.id))}
                >
                  Refresh balance
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  onClick={() => setDetailsFor(detailsFor === account.id ? '' : account.id)}
                >
                  {detailsFor === account.id ? 'Hide keys' : 'Show keys'}
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={() => handleForget(account)}
                >
                  Forget
                </button>
              </div>
              {detailsFor === account.id && <StructureLines description={account.description} />}
            </li>
          ))}
        </ul>
      )}

      {flow === null && (
        <div className="button-pair">
          <button className="secondary-button" type="button" onClick={() => openFlow({ kind: 'link' })}>
            Link a shared account
          </button>
          <button className="secondary-button" type="button" onClick={() => openFlow({ kind: 'create' })}>
            Create a shared account
          </button>
          <button className="secondary-button" type="button" onClick={() => openFlow({ kind: 'convert' })}>
            Convert an account
          </button>
        </div>
      )}

      {flow?.kind === 'link' && (
        <form onSubmit={handlePreview} aria-label="Link a shared account">
          <label htmlFor="shared-link-id">Account ID</label>
          <input
            id="shared-link-id"
            autoComplete="off"
            spellCheck={false}
            placeholder="0.0.12345 or 0.0.12345-abcde"
            value={linkId}
            onChange={(event) => {
              setLinkId(event.target.value)
              setPreview(null)
            }}
          />
          <label htmlFor="shared-link-name">Name (optional)</label>
          <input
            id="shared-link-name"
            autoComplete="off"
            maxLength={25}
            value={linkName}
            onChange={(event) => setLinkName(event.target.value)}
          />
          {preview && (
            <>
              <p>
                This account is controlled by {preview.required} of {preview.total} keys. Check
                every key before linking:
              </p>
              <StructureLines description={preview.description} />
            </>
          )}
          <div className="button-pair">
            {preview ? (
              <button className="primary-button" type="button" disabled={isBusy} onClick={handleLink}>
                Link this account
              </button>
            ) : (
              <button className="primary-button" type="submit" disabled={isBusy || !linkId.trim()}>
                Fetch key structure
              </button>
            )}
            <button className="secondary-button" type="button" onClick={() => openFlow(null)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {flow?.kind === 'create' && !signing && (
        <form onSubmit={handleCreate} aria-label="Create a shared account">
          <label htmlFor="shared-funder">Funding account (signs and pays)</label>
          <select
            id="shared-funder"
            value={funder}
            onChange={(event) => setFunder(event.target.value)}
          >
            <option value="">Choose a hot-wallet account...</option>
            {hotAccounts.map(({ wallet, account }) => (
              <option key={`${wallet.id}/${account.id}`} value={account.id}>
                {wallet.name} - {account.display_id}
              </option>
            ))}
          </select>
          <label htmlFor="shared-opening">Opening balance (HBAR)</label>
          <p className="field-hint">
            The funding account pays the network fee in addition to the opening balance.
          </p>
          <input
            id="shared-opening"
            autoComplete="off"
            spellCheck={false}
            inputMode="decimal"
            value={openingBalance}
            onChange={(event) => setOpeningBalance(event.target.value)}
          />
          {thresholdFields}
          <label htmlFor="shared-create-name">Name (optional)</label>
          <input
            id="shared-create-name"
            autoComplete="off"
            maxLength={25}
            value={linkName}
            onChange={(event) => setLinkName(event.target.value)}
          />
          <div className="button-pair">
            <button
              className="primary-button"
              type="submit"
              disabled={isBusy || !funder || !openingBalance.trim() || keyList().length < 2}
            >
              {isBusy ? 'Working...' : 'Create account'}
            </button>
            <button className="secondary-button" type="button" disabled={isBusy} onClick={() => openFlow(null)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {flow?.kind === 'convert' && !signing && (
        <form onSubmit={handleConvert} aria-label="Convert an account to shared">
          <p className="dialog-copy">
            Replaces the account&apos;s key with a threshold structure. The current
            key signs (the vault signs for its own wallets), and enough of the
            new keys must sign too.
          </p>
          <label htmlFor="shared-convert-source">Account to convert</label>
          <select
            id="shared-convert-source"
            value={convertSource}
            onChange={(event) => setConvertSource(event.target.value)}
          >
            <option value="">Choose an account...</option>
            {allAccounts.map(({ wallet, account }) => (
              <option key={`${wallet.id}/${account.id}`} value={`${wallet.id}/${account.id}`}>
                {wallet.name} - {account.display_id}
              </option>
            ))}
          </select>
          {thresholdFields}
          <label htmlFor="shared-convert-name">Name (optional)</label>
          <input
            id="shared-convert-name"
            autoComplete="off"
            maxLength={25}
            value={linkName}
            onChange={(event) => setLinkName(event.target.value)}
          />
          <div className="button-pair">
            <button
              className="primary-button"
              type="submit"
              disabled={isBusy || !convertSource || keyList().length < 2}
            >
              {isBusy ? 'Working...' : 'Review key change'}
            </button>
            <button className="secondary-button" type="button" disabled={isBusy} onClick={() => openFlow(null)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {flow?.kind === 'send' && !signing && (
        <form onSubmit={handleSend} aria-label="Send HBAR from a shared account">
          <p>
            Sending from <strong>{flow.account.displayId}</strong> ({flow.account.required} of{' '}
            {flow.account.total} keys must sign).
          </p>
          <label htmlFor="shared-recipient">Recipient account ID</label>
          <input
            id="shared-recipient"
            autoComplete="off"
            spellCheck={false}
            placeholder="0.0.12345-abcde"
            value={recipient}
            onChange={(event) => setRecipient(event.target.value)}
          />
          <label htmlFor="shared-amount">Amount (HBAR)</label>
          <input
            id="shared-amount"
            autoComplete="off"
            spellCheck={false}
            inputMode="decimal"
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
          />
          <label htmlFor="shared-memo">Memo (optional)</label>
          <input
            id="shared-memo"
            autoComplete="off"
            maxLength={100}
            value={memo}
            onChange={(event) => setMemo(event.target.value)}
          />
          <div className="button-pair">
            <button
              className="primary-button"
              type="submit"
              disabled={isBusy || !recipient.trim() || !amount.trim()}
            >
              {isBusy ? 'Working...' : 'Create signing request'}
            </button>
            <button className="secondary-button" type="button" disabled={isBusy} onClick={() => openFlow(null)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      {collector}
    </section>
  )
}

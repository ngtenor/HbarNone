import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { DropdownMenu } from './DropdownMenu'
import { QrScanner } from './QrScanner'
import type { TxProgress } from './TransactionProgress'
import type { AddressBookView, WalletSummary } from './types'

const POLL_INTERVAL_MS = 2000
const POLL_ATTEMPTS = 45

const sleep = (ms: number) => new Promise<void>((resolve) => window.setTimeout(resolve, ms))

// The mirror node trails consensus by a few seconds, so bookkeeping steps retry.
async function retry<T>(task: () => Promise<T>, attempts: number): Promise<T> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await task()
    } catch (failure) {
      if (attempt >= attempts) throw failure
      await sleep(3000)
    }
  }
}

type TransactionSummary = {
  transaction_id: string
  kind: string
  result: string
  consensus_time: string
  net_tinybars: number
  counterparty?: string
}

type AccountToken = {
  token_id: string
  symbol: string
  name: string
  balance: string
  decimals: number
}

type TokenInfo = {
  token_id: string
  name: string
  symbol: string
  decimals: number
  token_type: string
  total_supply: string
  treasury: string
  deleted: boolean
  has_admin_key: boolean
  admin_key: string | null
  memo: string
}

type AccountOverview = {
  account_id: string
  balance_tinybars: number
  transactions: TransactionSummary[]
  tokens?: AccountToken[]
}

type TokenSection =
  | 'token-info'
  | 'token-add'
  | 'token-remove'
  | 'token-hide'
  | 'token-create'
  | 'token-delete'

const TINYBARS_PER_HBAR = 100_000_000

function formatHbar(tinybars: number, maxDecimals = 8) {
  return (tinybars / TINYBARS_PER_HBAR).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: maxDecimals,
  })
}

// Token amounts are integers in the smallest unit; BigInt keeps large supplies exact.
function formatTokenAmount(units: string, decimals: number) {
  let value: bigint
  try {
    value = BigInt(units)
  } catch {
    return units
  }
  const scale = 10n ** BigInt(decimals)
  const whole = (value / scale).toLocaleString()
  if (decimals === 0) return whole
  const point = (1.1).toLocaleString().charAt(1)
  return `${whole}${point}${(value % scale).toString().padStart(decimals, '0')}`
}

function tokenLabel(token: AccountToken) {
  return token.symbol || token.token_id
}

function formatNet(tinybars: number) {
  const sign = tinybars > 0 ? '+' : ''
  return `${sign}${formatHbar(tinybars, 2)} ℏ`
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

type SigningRequest = {
  payload: string
  qrSize: number
  qrModules: boolean[]
  action: 'transfer' | 'delete' | 'create'
  sender: string
  receiver: string
  deleteAccount: string
  newKey: string
  tinybars: number
  memo: string
  expiresAtSeconds: number
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

type Props = {
  wallet: WalletSummary
  account: string | null
  otherWallets: WalletSummary[]
  addressBook: AddressBookView
  onWalletUpdated: (wallet: WalletSummary) => void
  onAccountMoved: (target: WalletSummary, sourceWalletId: string, accountId: string) => void
  onAccountDeleted: (updated: WalletSummary, accountId: string) => void
  onBalance?: (walletId: string, accountId: string, tinybars: number) => void
  onProgress: (progress: TxProgress | null) => void
}

export function TestnetPanel({
  wallet,
  account,
  otherWallets,
  addressBook,
  onWalletUpdated,
  onAccountMoved,
  onAccountDeleted,
  onBalance,
  onProgress,
}: Props) {
  const [overview, setOverview] = useState<AccountOverview | null>(null)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [isBusy, setIsBusy] = useState(false)
  const [recipient, setRecipient] = useState('')
  const [useOtherRecipient, setUseOtherRecipient] = useState(false)
  const [nameInput, setNameInput] = useState('')
  const [amount, setAmount] = useState('')
  const [memo, setMemo] = useState('')
  const [confirming, setConfirming] = useState(false)
  const [openingBalance, setOpeningBalance] = useState('')
  const [newAccountName, setNewAccountName] = useState('')
  const [funder, setFunder] = useState('')
  const [funderBalances, setFunderBalances] = useState<Record<string, number>>({})
  const [confirmingCreate, setConfirmingCreate] = useState(false)
  const [moveTarget, setMoveTarget] = useState('')
  const [confirmingMove, setConfirmingMove] = useState(false)
  const [pendingMove, setPendingMove] = useState('')
  const [moveAcknowledged, setMoveAcknowledged] = useState(false)
  const [deleteRecipient, setDeleteRecipient] = useState('')
  const [useOtherDeleteRecipient, setUseOtherDeleteRecipient] = useState(false)
  const [confirmingDelete, setConfirmingDelete] = useState(false)
  const [deleteAcknowledged, setDeleteAcknowledged] = useState(false)
  const [pendingDelete, setPendingDelete] = useState(false)
  const [showAssets, setShowAssets] = useState(false)
  const [hiddenTokens, setHiddenTokens] = useState<string[]>([])
  const [showHidden, setShowHidden] = useState(false)
  const [tokenToRemove, setTokenToRemove] = useState('')
  const [section, setSection] = useState<TokenSection | 'send' | 'rename' | 'move' | 'delete' | null>(null)
  const [tokenQuery, setTokenQuery] = useState('')
  const [tokenDetails, setTokenDetails] = useState<TokenInfo | null>(null)
  const [newTokenName, setNewTokenName] = useState('')
  const [newTokenSymbol, setNewTokenSymbol] = useState('')
  const [newTokenDecimals, setNewTokenDecimals] = useState('0')
  const [newTokenSupply, setNewTokenSupply] = useState('')
  const [confirmingToken, setConfirmingToken] = useState(false)
  const [tokenToDelete, setTokenToDelete] = useState('')
  const [tokenDeleteAcknowledged, setTokenDeleteAcknowledged] = useState(false)
  const [sendTokenId, setSendTokenId] = useState('')
  const isWatchOnly = wallet.secret_kind === 'watchonly'
  const [signing, setSigning] = useState<SigningRequest | null>(null)
  const [signatureText, setSignatureText] = useState('')
  const [scanning, setScanning] = useState(false)
  const handleScanned = (text: string) => {
    setSignatureText(text)
    setScanning(false)
  }
  const [secondsLeft, setSecondsLeft] = useState(0)
  useEffect(() => {
    if (!signing) setScanning(false)
  }, [signing])
  useEffect(() => {
    if (!signing) return
    const tick = () =>
      setSecondsLeft(Math.max(0, signing.expiresAtSeconds - Math.floor(Date.now() / 1000)))
    tick()
    const timer = window.setInterval(tick, 1000)
    return () => window.clearInterval(timer)
  }, [signing])
  const toggleSection = (next: TokenSection | 'send' | 'rename' | 'move' | 'delete') => {
    setSection((current) => (current === next ? null : next))
    // Leaving a section discards anything typed into it and any pending review.
    setRecipient('')
    setUseOtherRecipient(false)
    setAmount('')
    setMemo('')
    setConfirming(false)
    setSigning(null)
    setSignatureText('')
    setMoveTarget('')
    setConfirmingMove(false)
    setMoveAcknowledged(false)
    setDeleteRecipient('')
    setUseOtherDeleteRecipient(false)
    setConfirmingDelete(false)
    setDeleteAcknowledged(false)
    setTokenQuery('')
    setTokenDetails(null)
    setNewTokenName('')
    setNewTokenSymbol('')
    setNewTokenDecimals('0')
    setNewTokenSupply('')
    setConfirmingToken(false)
    setTokenToDelete('')
    setTokenToRemove('')
    setTokenDeleteAcknowledged(false)
    setSendTokenId('')
    setError('')
    setNotice('')
  }
  const allTokens = [...(overview?.tokens ?? [])].sort((a, b) =>
    tokenLabel(a).localeCompare(tokenLabel(b)),
  )
  const sendableTokens = allTokens.filter((token) => !hiddenTokens.includes(token.token_id))
  const hiddenHere = allTokens.length - sendableTokens.length
  const assetTokens = showHidden ? allTokens : sendableTokens
  const handleAddToken = () =>
    run(async () => {
      if (!tokenDetails) return
      const result = await invoke<{ transactionId: string }>('associate_token', {
        walletId: wallet.id,
        accountId: account,
        tokenId: tokenDetails.token_id,
      })
      toggleSection('token-add')
      void watchTransaction(result.transactionId, 'Adding token', async () => {
        await retry(loadOverview, 2)
      })
    })
  const handleRemoveToken = () =>
    run(async () => {
      const result = await invoke<{ transactionId: string }>('dissociate_token', {
        walletId: wallet.id,
        accountId: account,
        tokenId: tokenToRemove,
      })
      toggleSection('token-remove')
      void watchTransaction(result.transactionId, 'Removing token', async () => {
        await retry(loadOverview, 2)
      })
    })
  const handleSetHidden = (tokenId: string, hidden: boolean) =>
    run(async () => {
      setHiddenTokens(
        await invoke<string[]>('set_token_hidden', { walletId: wallet.id, tokenId, hidden }),
      )
    })
  useEffect(() => {
    let current = true
    invoke<string[]>('get_hidden_tokens', { walletId: wallet.id })
      .then((ids) => {
        if (current) setHiddenTokens(ids ?? [])
      })
      .catch(() => {})
    return () => {
      current = false
    }
  }, [wallet.id])
  const decimalsValid = /^\d{1,2}$/.test(newTokenDecimals) && Number(newTokenDecimals) <= 18
  const handleReviewToken = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError('')
    if (!newTokenName.trim() || !newTokenSymbol.trim()) {
      setError('Enter a token name and symbol.')
    } else if (!decimalsValid) {
      setError('Decimals must be a whole number from 0 to 18.')
    } else if (!/^(\d+\.?\d*|\.\d+)$/.test(newTokenSupply.trim())) {
      setError('Enter the initial supply as a number, such as 1000 (0 is allowed).')
    } else {
      setConfirmingToken(true)
    }
  }
  const handleCreateToken = () =>
    run(async () => {
      const result = await invoke<{ transactionId: string }>('create_token', {
        walletId: wallet.id,
        fromAccount: account,
        name: newTokenName.trim(),
        symbol: newTokenSymbol.trim(),
        decimals: Number(newTokenDecimals),
        initialSupply: newTokenSupply.trim(),
      })
      toggleSection('token-create')
      void watchTransaction(result.transactionId, 'Creating token', async () => {
        await retry(loadOverview, 2)
      })
    })
  const handleDeleteToken = () =>
    run(async () => {
      const result = await invoke<{ transactionId: string }>('delete_token', {
        walletId: wallet.id,
        accountId: account,
        tokenId: tokenToDelete,
      })
      toggleSection('token-delete')
      void watchTransaction(result.transactionId, 'Deleting token', async () => {
        await retry(loadOverview, 2)
      })
    })
  const handleLookupToken = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setTokenDetails(null)
    void run(async () => {
      const found = await invoke<TokenInfo | null>('get_token_info', { tokenId: tokenQuery.trim() })
      if (!found) throw new Error('Testnet does not know this token.')
      setTokenDetails(found)
    })
  }
  const alive = useRef(true)
  async function run(task: () => Promise<void>) {
    if (isBusy) return
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

  async function loadOverview() {
    if (!account) return
    const result = await invoke<AccountOverview | null>('get_account_overview', {
      walletId: wallet.id,
      accountId: account,
    })
    setOverview(result)
    if (result) onBalance?.(wallet.id, account, result.balance_tinybars)
    if (!result) setNotice('This account was not found on Hedera Testnet.')
  }

  // Every refresh control reloads everything shown for the account.
  const handleRefresh = () => run(loadOverview)

  const refreshButton = (label: string) => (
    <button
      className={`icon-button${isBusy ? ' refreshing' : ''}`}
      type="button"
      aria-label={label}
      title={isBusy ? 'Contacting Testnet...' : label}
      disabled={isBusy}
      onClick={handleRefresh}
    >
      <span aria-hidden="true">↻</span>
    </button>
  )

  // Selecting an account refreshes its balance; the panel remounts per account.
  useEffect(() => {
    if (account) void handleRefresh()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallet.id, account])

  useEffect(() => {
    alive.current = true
    return () => {
      alive.current = false
    }
  }, [])

  // Shows the progress popup until Testnet reports a consensus result, then
  // runs `onConfirmed` (which may return a detail line for the popup).
  async function watchTransaction(
    transactionId: string,
    title: string,
    onConfirmed: () => Promise<string | void>,
  ) {
    onProgress({ phase: 'waiting', title })
    for (let attempt = 0; attempt < POLL_ATTEMPTS && alive.current; attempt += 1) {
      if (attempt > 0) await sleep(POLL_INTERVAL_MS)
      if (!alive.current) return
      let result: string | null = null
      try {
        result = await invoke<string | null>('get_transaction_result', { transactionId })
      } catch {
        result = null
      }
      if (!result) continue
      if (result !== 'SUCCESS') {
        onProgress({ phase: 'failed', title, detail: `Testnet result: ${result}` })
        return
      }
      const detail = await onConfirmed().catch(() => undefined)
      onProgress({ phase: 'success', title, detail: detail || undefined })
      return
    }
    if (alive.current) onProgress({ phase: 'timeout', title, detail: transactionId })
  }

  const targetName = otherWallets.find((item) => item.id === moveTarget)?.name ?? ''
  const accountSummary = wallet.accounts.find((item) => item.id === account)
  const accountLabel = accountSummary
    ? `${accountSummary.name} - ${accountSummary.display_id}`
    : account
  const sameAccount = (typed: string) => typed.trim().split('-')[0] === account
  const funderRows = addressBook.rows.filter(
    (row) => row.network === 'testnet' && row.wallet_id && (!isWatchOnly || row.wallet_id === wallet.id),
  )
  const funderKey = funderRows.map((row) => row.display_id).join(',')

  useEffect(() => {
    if (account || !funderKey) return
    let cancelled = false
    for (const displayId of funderKey.split(',')) {
      invoke<number | null>('get_address_balance', { accountId: displayId.split('-')[0] })
        .then((tinybars) => {
          if (!cancelled && tinybars !== null) {
            setFunderBalances((existing) => ({ ...existing, [displayId]: tinybars }))
          }
        })
        .catch(() => undefined)
    }
    return () => {
      cancelled = true
    }
  }, [account, funderKey])
  const funderRow = funderRows.find((row) => row.display_id === funder)
  const funderLabel = funderRow ? `${funderRow.name} ${funderRow.display_id}` : funder

  const handleRename = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    return run(async () => {
      if (!account) return
      const updated = await invoke<WalletSummary>('rename_account', {
        walletId: wallet.id,
        accountId: account,
        name: nameInput,
      })
      onWalletUpdated(updated)
      setSection(null)
    })
  }

  // The mirror node trails consensus, so the wallet bookkeeping retries a few times.
  async function completeMove(target: string) {
    if (!account) return
    try {
      const updated = await retry(() => invoke<WalletSummary>('finish_account_move', {
        walletId: wallet.id,
        accountId: account,
        targetWalletId: target,
      }), 4)
      onAccountMoved(updated, wallet.id, account)
    } catch (failure) {
      setError(`${errorMessage(failure)} Use “Check again” in a moment.`)
      setPendingMove(target)
      return 'Confirmed on Testnet, but the wallet could not be updated yet. Use “Check again”.'
    }
  }

  async function completeDelete() {
    if (!account) return
    try {
      const updated = await retry(() => invoke<WalletSummary>('finish_account_delete', {
        walletId: wallet.id,
        accountId: account,
      }), 4)
      onAccountDeleted(updated, account)
    } catch (failure) {
      setError(`${errorMessage(failure)} Use “Check again” in a moment.`)
      setPendingDelete(true)
      return 'Confirmed on Testnet, but the wallet could not be updated yet. Use “Check again”.'
    }
  }

  const handleDelete = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('delete_account', {
      walletId: wallet.id,
      accountId: account,
      recipient: deleteRecipient.trim(),
    })
    setConfirmingDelete(false)
    setDeleteAcknowledged(false)
    void watchTransaction(result.transactionId, 'Deleting account', completeDelete)
  })

  const handleCheckDelete = () => run(async () => {
    if (!account) return
    const updated = await invoke<WalletSummary>('finish_account_delete', {
      walletId: wallet.id,
      accountId: account,
    })
    onAccountDeleted(updated, account)
  })

  const handleMove = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('move_account', {
      walletId: wallet.id,
      accountId: account,
      targetWalletId: moveTarget,
    })
    const target = moveTarget
    setConfirmingMove(false)
    setMoveAcknowledged(false)
    void watchTransaction(result.transactionId, 'Moving account', () => completeMove(target))
  })

  const handleCheckMove = () => run(async () => {
    if (!account || !pendingMove) return
    const updated = await invoke<WalletSummary>('finish_account_move', {
      walletId: wallet.id,
      accountId: account,
      targetWalletId: pendingMove,
    })
    onAccountMoved(updated, wallet.id, account)
  })

  const resetSend = () => {
    setRecipient('')
    setUseOtherRecipient(false)
    setAmount('')
    setMemo('')
    setConfirming(false)
    setSigning(null)
    setSignatureText('')
    setSendTokenId('')
  }

  const handlePrepare = () => run(async () => {
    const request = await invoke<SigningRequest>('prepare_unsigned_transfer', {
      walletId: wallet.id,
      fromAccount: account,
      recipient: recipient.trim(),
      amountHbar: amount.trim(),
      memo,
    })
    setSignatureText('')
    setSigning(request)
  })

  const handleSubmitSigned = () => run(async () => {
    if (!signing) return
    const action = signing.action
    const result = await invoke<{ transactionId: string }>('submit_signed_request', {
      request: signing.payload,
      response: signatureText.trim(),
    })
    if (action === 'delete') {
      setSigning(null)
      setSignatureText('')
      setDeleteRecipient('')
      void watchTransaction(result.transactionId, 'Deleting account', completeDelete)
    } else if (action === 'create') {
      setSigning(null)
      setSignatureText('')
      setOpeningBalance('')
      void watchNewAccount(result.transactionId)
    } else {
      resetSend()
      void watchTransaction(result.transactionId, 'Sending HBAR', async () => {
        await retry(loadOverview, 2)
      })
    }
  })

  const handlePrepareDelete = () => run(async () => {
    const request = await invoke<SigningRequest>('prepare_unsigned_delete', {
      walletId: wallet.id,
      accountId: account,
      recipient: deleteRecipient.trim(),
    })
    setSignatureText('')
    setConfirmingDelete(false)
    setDeleteAcknowledged(false)
    setSigning(request)
  })

  const handlePrepareCreate = () => run(async () => {
    const request = await invoke<SigningRequest>('prepare_unsigned_create', {
      walletId: wallet.id,
      fromAccount: funder.split('-')[0],
      initialHbar: openingBalance.trim(),
    })
    setSignatureText('')
    setConfirmingCreate(false)
    setSigning(request)
  })

  const renderSigning = (submitLabel: string, onCancel: () => void) => signing && (
    <div className="testnet-confirm" role="group" aria-label="Signing request">
      <p>
        Scan this code with your offline signer, check the details it shows, then paste its
        reply below.
      </p>
      <QrCode request={signing} />
      <p className="field-hint" role="timer">
        {secondsLeft > 0
          ? `Valid for ${secondsLeft} more seconds.`
          : 'This request has expired. Go back and create a new one.'}
      </p>
      <dl>
        {signing.action === 'transfer' && (
          <>
            <dt>From</dt>
            <dd>{signing.sender}</dd>
            <dt>To</dt>
            <dd>{signing.receiver}</dd>
            <dt>Amount</dt>
            <dd>{formatHbar(signing.tinybars)} ℏ</dd>
            {signing.memo && (
              <>
                <dt>Memo</dt>
                <dd>{signing.memo}</dd>
              </>
            )}
          </>
        )}
        {signing.action === 'delete' && (
          <>
            <dt>Delete account</dt>
            <dd>{signing.deleteAccount}</dd>
            <dt>Remaining ℏ go to</dt>
            <dd>{signing.receiver}</dd>
            <dt>Fee paid by</dt>
            <dd>{signing.sender}</dd>
          </>
        )}
        {signing.action === 'create' && (
          <>
            <dt>Funded by</dt>
            <dd>{signing.sender}</dd>
            <dt>Opening balance</dt>
            <dd>{formatHbar(signing.tinybars)} ℏ</dd>
            <dt>New account key</dt>
            <dd className="key-text">{signing.newKey} (this wallet&apos;s own key)</dd>
          </>
        )}
      </dl>
      <details>
        <summary>Show request as text</summary>
        <textarea readOnly rows={4} aria-label="Signing request text" value={signing.payload} />
      </details>
      <label htmlFor={`signature-${wallet.id}`}>Signature from the signer</label>
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
        id={`signature-${wallet.id}`}
        rows={3}
        autoComplete="off"
        spellCheck={false}
        placeholder="HVTX1-SIG:..."
        value={signatureText}
        onChange={(event) => setSignatureText(event.target.value)}
      />
      <div className="button-pair">
        <button
          className="primary-button"
          type="button"
          disabled={isBusy || secondsLeft === 0 || !signatureText.trim()}
          onClick={handleSubmitSigned}
        >
          {isBusy ? 'Submitting...' : submitLabel}
        </button>
        <button className="secondary-button" type="button" disabled={isBusy} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  )

  const handleReview = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError('')
    setNotice('')
    if (!recipient.trim()) {
      setError(useOtherRecipient ? 'Enter a recipient account.' : 'Choose a recipient from the address book.')
      return
    }
    if (sameAccount(recipient)) {
      setError('The recipient must be a different account.')
      return
    }
    return run(async () => {
      await invoke<string>('check_recipient', { recipient, forSend: true })
      setConfirming(true)
    })
  }

  const sendToken = allTokens.find((token) => token.token_id === sendTokenId)

  const handleSend = () => run(async () => {
    if (sendToken) {
      const result = await invoke<{ transactionId: string }>('send_token', {
        walletId: wallet.id,
        fromAccount: account,
        recipient: recipient.trim(),
        tokenId: sendToken.token_id,
        amount: amount.trim(),
        memo,
      })
      resetSend()
      void watchTransaction(result.transactionId, `Sending ${tokenLabel(sendToken)}`, async () => {
        await retry(loadOverview, 2)
      })
      return
    }
    const result = await invoke<{ transactionId: string }>('send_hbar', {
      walletId: wallet.id,
      fromAccount: account,
      recipient: recipient.trim(),
      amountHbar: amount.trim(),
      memo,
    })
    resetSend()
    void watchTransaction(result.transactionId, 'Sending HBAR', async () => {
      await retry(loadOverview, 2)
    })
  })

  const parseHbarToTinybars = (text: string) => {
    const match = /^(\d*)(?:\.(\d{0,8}))?$/.exec(text.trim())
    if (!match || (!match[1] && !match[2])) return null
    const whole = Number(match[1] || '0')
    const tinybars = whole * TINYBARS_PER_HBAR + Number((match[2] ?? '').padEnd(8, '0'))
    return Number.isSafeInteger(tinybars) && tinybars > 0 ? tinybars : null
  }

  const handleReviewCreate = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError('')
    setNotice('')
    const tinybars = parseHbarToTinybars(openingBalance)
    if (tinybars === null) {
      setError('Enter a valid opening balance greater than zero, like 1.5 (at most 8 decimal places).')
      return
    }
    return run(async () => {
      const balance = await invoke<number | null>('get_address_balance', {
        accountId: funder.split('-')[0],
      })
      if (balance === null || balance < tinybars) {
        throw new Error(
          balance === null
            ? 'The funding account was not found on Testnet.'
            : `The funding account has only ${formatHbar(balance)} ℏ, which is less than the opening balance.`,
        )
      }
      setConfirmingCreate(true)
    })
  }

  // Waits for the new account to show up on the mirror node, then names it.
  const watchNewAccount = (transactionId: string) => {
    const accountsBefore = wallet.accounts.length
    const idsBefore = new Set(wallet.accounts.map((item) => item.id))
    const chosenName = newAccountName.trim()
    setNewAccountName('')
    return watchTransaction(transactionId, 'Creating account', async () => {
      let updated = await retry(async () => {
        const found = await invoke<WalletSummary>('find_wallet_accounts', { walletId: wallet.id })
        if (found.accounts.length <= accountsBefore) throw new Error('not listed yet')
        return found
      }, 4).catch(() => null)
      if (!updated) return 'The new account is not listed yet. Use Find accounts in a moment.'
      const created = updated.accounts.find((item) => !idsBefore.has(item.id))
      if (chosenName && created) {
        try {
          updated = await invoke<WalletSummary>('rename_account', {
            walletId: wallet.id,
            accountId: created.id,
            name: chosenName,
          })
        } catch (renameError) {
          onWalletUpdated(updated)
          return `The account was created but could not be named: ${errorMessage(renameError)}`
        }
      }
      onWalletUpdated(updated)
    })
  }

  const handleCreate = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('create_account', {
      walletId: wallet.id,
      fromAccount: funder,
      initialHbar: openingBalance.trim(),
    })
    setOpeningBalance('')
    setConfirmingCreate(false)
    void watchNewAccount(result.transactionId)
  })

  return (
    <section className={`panel testnet-panel${account ? '' : ' create-panel'}`} aria-label={account ? 'Account' : 'Create account'}>
      {account ? (
        <>
          <div className="account-heading">
            <h2 className="section-title">{accountLabel}</h2>
            <div className="account-heading-balance-group">
              {refreshButton('Refresh balance')}
              <strong className="account-heading-balance">
                {overview ? `${formatHbar(overview.balance_tinybars, 2)} ℏ` : ''}
              </strong>
            </div>
          </div>
          <div className="wallet-tab-actions account-actions">
            <DropdownMenu
              label="Send"
              primary
              entries={[
                { id: 'hbar', label: 'HBAR' },
                ...sendableTokens.map((token) => ({
                  id: `token:${token.token_id}`,
                  label: tokenLabel(token),
                  detail: formatTokenAmount(token.balance, token.decimals),
                  disabled: token.balance === '0',
                })),
              ]}
              onSelect={(id) => {
                if (id === 'hbar') {
                  toggleSection('send')
                } else if (isWatchOnly) {
                  setSection(null)
                  setNotice('Sending tokens from a watch-only wallet is not available yet.')
                } else {
                  toggleSection('send')
                  setSection('send')
                  setSendTokenId(id.slice('token:'.length))
                }
              }}
            />
            <button
              className="secondary-button"
              type="button"
              onClick={() => setShowAssets((current) => !current)}
            >
              {showAssets ? 'Recent transactions' : 'Account assets'}
            </button>
            <DropdownMenu
              label="Token management"
              entries={[
                { kind: 'heading', label: 'Holding' },
                { id: 'token-info', label: 'Token info' },
                {
                  id: 'token-add',
                  label: 'Associate token',
                  detail: isWatchOnly ? 'needs a signer' : undefined,
                  disabled: isWatchOnly,
                },
                {
                  id: 'token-remove',
                  label: 'Remove token',
                  detail: isWatchOnly ? 'needs a signer' : undefined,
                  disabled: isWatchOnly,
                },
                { id: 'token-hide', label: 'Hide or show tokens' },
                { kind: 'heading', label: 'Issuing' },
                {
                  id: 'token-create',
                  label: 'Create token',
                  detail: isWatchOnly ? 'needs a signer' : undefined,
                  disabled: isWatchOnly,
                },
                {
                  id: 'token-delete',
                  label: 'Delete token',
                  detail: isWatchOnly ? 'needs a signer' : undefined,
                  disabled: isWatchOnly,
                  danger: true,
                },
              ]}
              onSelect={(id) => toggleSection(id as TokenSection)}
            />
            <DropdownMenu
              label="Account management"
              entries={[
                { id: 'rename', label: accountSummary?.named ? 'Rename account' : 'Name this account' },
                ...(!isWatchOnly && otherWallets.length > 0
                  ? [{ id: 'move', label: 'Move account' }]
                  : []),
                { id: 'delete', label: 'Delete account', danger: true },
              ]}
              onSelect={(id) => {
                if (id === 'rename') setNameInput(accountSummary?.named ? accountSummary.name : '')
                toggleSection(id as 'rename' | 'move' | 'delete')
              }}
            />
          </div>
        </>
      ) : null}

      {error && <p className="error-message" role="alert">{error}</p>}
      {notice && <p className="field-hint" role="status">{notice}</p>}

      {account && section === 'rename' && (
        <div className="testnet-send">
          <h3 className="section-title">{accountSummary?.named ? 'Rename account' : 'Name this account'}</h3>
          <form className="testnet-link-form" onSubmit={handleRename} noValidate>
            <label htmlFor={`account-name-${wallet.id}`}>Account name (max 25 characters)</label>
            <input
              id={`account-name-${wallet.id}`}
              autoComplete="off"
              maxLength={25}
              value={nameInput}
              onChange={(event) => setNameInput(event.target.value)}
            />
            <div className="button-pair">
              <button className="primary-button" type="submit" disabled={isBusy}>
                Save name
              </button>
              <button className="secondary-button" type="button" onClick={() => setSection(null)}>
                Cancel
              </button>
            </div>
          </form>
        </div>
      )}

      {account && section === 'token-info' && (
        <div className="testnet-send">
          <h3 className="section-title">Token info</h3>
          <form className="testnet-link-form" onSubmit={handleLookupToken} noValidate>
            <label htmlFor={`token-id-${wallet.id}`}>Token ID</label>
            <input
              id={`token-id-${wallet.id}`}
              autoComplete="off"
              placeholder="0.0.12345"
              maxLength={32}
              value={tokenQuery}
              onChange={(event) => setTokenQuery(event.target.value)}
            />
            <div className="button-pair">
              <button className="secondary-button" type="submit" disabled={isBusy}>
                Look up token
              </button>
              <button className="secondary-button" type="button" onClick={() => setSection(null)}>
                Cancel
              </button>
            </div>
          </form>
          {tokenDetails && (
            <dl className="testnet-confirm token-details" aria-label="Token details">
              <dt>Name</dt>
              <dd>{tokenDetails.name || '(none)'}</dd>
              <dt>Symbol</dt>
              <dd>{tokenDetails.symbol || '(none)'}</dd>
              <dt>Token ID</dt>
              <dd>{tokenDetails.token_id}</dd>
              <dt>Type</dt>
              <dd>{tokenDetails.token_type}</dd>
              <dt>Decimals</dt>
              <dd>{tokenDetails.decimals}</dd>
              <dt>Total supply</dt>
              <dd>{formatTokenAmount(tokenDetails.total_supply, tokenDetails.decimals)}</dd>
              <dt>Treasury</dt>
              <dd>{tokenDetails.treasury}</dd>
              <dt>Admin key</dt>
              <dd>{tokenDetails.has_admin_key ? 'Yes (the token can be changed or deleted)' : 'None (fixed)'}</dd>
              {tokenDetails.deleted && (
                <>
                  <dt>Status</dt>
                  <dd>Deleted</dd>
                </>
              )}
              {tokenDetails.memo && (
                <>
                  <dt>Memo</dt>
                  <dd>{tokenDetails.memo}</dd>
                </>
              )}
            </dl>
          )}
        </div>
      )}

      {account && !isWatchOnly && section === 'token-add' && (
        <div className="testnet-send">
          <h3 className="section-title">Associate token</h3>
          <form className="testnet-link-form" onSubmit={handleLookupToken} noValidate>
            <label htmlFor={`token-add-id-${wallet.id}`}>Token ID</label>
            <input
              id={`token-add-id-${wallet.id}`}
              autoComplete="off"
              placeholder="0.0.12345"
              maxLength={32}
              value={tokenQuery}
              onChange={(event) => setTokenQuery(event.target.value)}
            />
            <div className="button-pair">
              <button className="secondary-button" type="submit" disabled={isBusy}>
                Look up token
              </button>
              <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('token-add')}>
                Cancel
              </button>
            </div>
          </form>
          {tokenDetails && (
            <div className="testnet-confirm" role="group" aria-label="Confirm adding token">
              <p>Please check that this is the token you expect. Anyone can create a token with any name.</p>
              <dl>
                <dt>Name</dt>
                <dd>{tokenDetails.name || '(none)'}</dd>
                <dt>Symbol</dt>
                <dd>{tokenDetails.symbol || '(none)'}</dd>
                <dt>Token ID</dt>
                <dd>{tokenDetails.token_id}</dd>
                <dt>Decimals</dt>
                <dd>{tokenDetails.decimals}</dd>
                <dt>Total supply</dt>
                <dd>{formatTokenAmount(tokenDetails.total_supply, tokenDetails.decimals)}</dd>
                <dt>Treasury</dt>
                <dd>{tokenDetails.treasury}</dd>
                <dt>Account</dt>
                <dd>{accountLabel}</dd>
                <dt>Fee</dt>
                <dd>A small network fee, about 0.05 USD in HBAR</dd>
              </dl>
              {tokenDetails.deleted && <p className="error-message">This token has been deleted.</p>}
              <button
                className="primary-button"
                type="button"
                disabled={isBusy || tokenDetails.deleted}
                onClick={handleAddToken}
              >
                Sign and add token
              </button>
            </div>
          )}
        </div>
      )}

      {account && !isWatchOnly && section === 'token-remove' && (
        <div className="testnet-send">
          <h3 className="section-title">Remove token</h3>
          <div className="testnet-link-form">
            <label htmlFor={`token-remove-${wallet.id}`}>Token to remove</label>
            <select
              id={`token-remove-${wallet.id}`}
              value={tokenToRemove}
              onChange={(event) => setTokenToRemove(event.target.value)}
            >
              <option value="">Choose a token</option>
              {allTokens.map((token) => (
                <option key={token.token_id} value={token.token_id} disabled={token.balance !== '0'}>
                  {tokenLabel(token)} ({token.token_id})
                  {token.balance !== '0'
                    ? ` - balance ${formatTokenAmount(token.balance, token.decimals)}, must be 0`
                    : ''}
                </option>
              ))}
            </select>
            <p className="field-hint">
              Removing a token stops this account from holding it. The balance must be zero, and the token's
              treasury account cannot remove its own token. A small network fee is paid in HBAR.
            </p>
            <div className="button-pair">
              <button
                className="primary-button"
                type="button"
                disabled={isBusy || !tokenToRemove}
                onClick={handleRemoveToken}
              >
                Sign and remove token
              </button>
              <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('token-remove')}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {account && section === 'token-hide' && (
        <div className="testnet-send">
          <h3 className="section-title">Hide or show tokens</h3>
          <p className="field-hint">
            Hidden tokens stay in the account but are left out of Account assets and the Send menu. This applies to
            every account of this wallet and changes nothing on the network.
          </p>
          {allTokens.length === 0 ? (
            <p className="field-hint">This account holds no tokens.</p>
          ) : (
            <ul className="testnet-transactions testnet-assets" aria-label="Tokens">
              {allTokens.map((token) => {
                const hidden = hiddenTokens.includes(token.token_id)
                return (
                  <li key={token.token_id}>
                    <span className="testnet-kind">
                      {tokenLabel(token)}
                      {token.name && token.symbol ? (
                        <span className="testnet-token-name"> {token.name}</span>
                      ) : null}
                    </span>
                    <button
                      className="secondary-button"
                      type="button"
                      disabled={isBusy}
                      aria-label={`${hidden ? 'Show' : 'Hide'} ${tokenLabel(token)}`}
                      onClick={() => void handleSetHidden(token.token_id, !hidden)}
                    >
                      {hidden ? 'Show' : 'Hide'}
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
          <div className="button-pair">
            <button className="secondary-button" type="button" onClick={() => toggleSection('token-hide')}>
              Close
            </button>
          </div>
        </div>
      )}

      {account && !isWatchOnly && section === 'token-create' && (
        <div className="testnet-send">
          <h3 className="section-title">Create token</h3>
          {confirmingToken ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm token creation">
              <p>Please check the details. A token's name, symbol and decimals cannot be changed later.</p>
              <dl>
                <dt>Network</dt>
                <dd>Hedera Testnet (no real value)</dd>
                <dt>Name</dt>
                <dd>{newTokenName.trim()}</dd>
                <dt>Symbol</dt>
                <dd>{newTokenSymbol.trim()}</dd>
                <dt>Decimals</dt>
                <dd>{newTokenDecimals}</dd>
                <dt>Initial supply</dt>
                <dd>{newTokenSupply.trim()}</dd>
                <dt>Treasury</dt>
                <dd>{accountLabel}</dd>
                <dt>Keys</dt>
                <dd>This wallet's key is the admin and supply key, so it can later delete the token.</dd>
                <dt>Fee</dt>
                <dd>About 1 USD in HBAR (at most 20 ℏ is allowed)</dd>
              </dl>
              <div className="button-pair">
                <button className="primary-button" type="button" disabled={isBusy} onClick={handleCreateToken}>
                  Sign and create token
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => setConfirmingToken(false)}>
                  Back
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('token-create')}>
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <form className="testnet-link-form" onSubmit={handleReviewToken} noValidate>
              <label htmlFor={`token-name-${wallet.id}`}>Token name</label>
              <input
                id={`token-name-${wallet.id}`}
                autoComplete="off"
                maxLength={100}
                value={newTokenName}
                onChange={(event) => setNewTokenName(event.target.value)}
              />
              <label htmlFor={`token-symbol-${wallet.id}`}>Symbol</label>
              <input
                id={`token-symbol-${wallet.id}`}
                autoComplete="off"
                maxLength={100}
                value={newTokenSymbol}
                onChange={(event) => setNewTokenSymbol(event.target.value)}
              />
              <label htmlFor={`token-decimals-${wallet.id}`}>Decimals (0 to 18)</label>
              <input
                id={`token-decimals-${wallet.id}`}
                inputMode="numeric"
                autoComplete="off"
                maxLength={2}
                value={newTokenDecimals}
                onChange={(event) => setNewTokenDecimals(event.target.value)}
              />
              <label htmlFor={`token-supply-${wallet.id}`}>Initial supply (whole tokens)</label>
              <input
                id={`token-supply-${wallet.id}`}
                inputMode="decimal"
                autoComplete="off"
                value={newTokenSupply}
                onChange={(event) => setNewTokenSupply(event.target.value)}
              />
              <div className="button-pair">
                <button className="primary-button" type="submit" disabled={isBusy}>
                  Review token
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('token-create')}>
                  Cancel
                </button>
              </div>
            </form>
          )}
        </div>
      )}

      {account && !isWatchOnly && section === 'token-delete' && (
        <div className="testnet-send">
          <h3 className="section-title">Delete token</h3>
          <div className="testnet-link-form">
            <label htmlFor={`token-delete-${wallet.id}`}>Token to delete</label>
            <select
              id={`token-delete-${wallet.id}`}
              value={tokenToDelete}
              onChange={(event) => {
                setTokenToDelete(event.target.value)
                setTokenDeleteAcknowledged(false)
              }}
            >
              <option value="">Choose a token</option>
              {sendableTokens.map((token) => (
                <option key={token.token_id} value={token.token_id}>
                  {tokenLabel(token)} ({token.token_id})
                </option>
              ))}
            </select>
            {tokenToDelete && (
              <>
                <p className="field-hint">
                  Deleting a token is permanent: nobody can send or receive it afterwards. Only the token's admin
                  key can delete it, and the app checks this before it signs. The fee is paid in HBAR.
                </p>
                <label className="testnet-acknowledge">
                  <input
                    type="checkbox"
                    checked={tokenDeleteAcknowledged}
                    onChange={(event) => setTokenDeleteAcknowledged(event.target.checked)}
                  />
                  I understand that this cannot be undone
                </label>
              </>
            )}
            <div className="button-pair">
              <button
                className="primary-button"
                type="button"
                disabled={isBusy || !tokenToDelete || !tokenDeleteAcknowledged}
                onClick={handleDeleteToken}
              >
                Sign and delete token
              </button>
              <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('token-delete')}>
                Cancel
              </button>
            </div>
          </div>
        </div>
      )}

      {account && section === 'send' && (
        <div className="testnet-send">
          <h3 className="section-title">{sendToken ? `Send ${tokenLabel(sendToken)}` : 'Send HBAR'}</h3>
          {signing ? renderSigning('Submit signed transfer', resetSend) : confirming ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm transfer">
              <p>Please check the details. A transfer cannot be undone.</p>
              <dl>
                <dt>Network</dt>
                <dd>Hedera Testnet (no real value)</dd>
                <dt>From</dt>
                <dd>{accountLabel}</dd>
                <dt>To</dt>
                <dd>{recipient.trim()}</dd>
                <dt>Amount</dt>
                <dd>{amount.trim()} {sendToken ? tokenLabel(sendToken) : 'ℏ'}</dd>
                {memo && (
                  <>
                    <dt>Memo</dt>
                    <dd>{memo}</dd>
                  </>
                )}
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <div className="button-pair">
                <button
                  className="primary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={isWatchOnly ? handlePrepare : handleSend}
                >
                  {isWatchOnly
                    ? isBusy ? 'Preparing...' : 'Create signing request'
                    : isBusy ? 'Sending...' : 'Sign and send'}
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => setConfirming(false)}>
                  Back
                </button>
                {error && (
                  <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('send')}>
                    Close
                  </button>
                )}
              </div>
            </div>
          ) : (
            <form className="testnet-send-form send-form" onSubmit={handleReview}>
              <select
                id={`send-book-${wallet.id}`}
                aria-label="Recipient from the address book"
                className={(useOtherRecipient ? '' : recipient) ? '' : 'placeholder-shown'}
                disabled={useOtherRecipient}
                value={useOtherRecipient ? '' : recipient}
                onChange={(event) => setRecipient(event.target.value)}
              >
                <option value="">Select recipient from the address book</option>
                {addressBook.rows
                  .filter((row) => row.id !== account)
                  .map((row) => (
                    <option key={`${row.network}/${row.id}`} value={row.display_id}>
                      {row.name} {row.display_id}
                    </option>
                  ))}
              </select>
              <div className="send-side">
                <label
                  className={`testnet-acknowledge${addressBook.allowExternalSends ? '' : ' disabled-text'}`}
                  htmlFor={`send-other-${wallet.id}`}
                >
                  <input
                    id={`send-other-${wallet.id}`}
                    type="checkbox"
                    disabled={!addressBook.allowExternalSends}
                    checked={useOtherRecipient}
                    onChange={(event) => {
                      setUseOtherRecipient(event.target.checked)
                      setRecipient('')
                    }}
                  />
                  <span>Check to send to a different account</span>
                </label>
                {!addressBook.allowExternalSends && (
                  <p className="field-hint disabled-text">
                    Send limited to accounts in the address book. Remove this limit in the Settings menu.
                  </p>
                )}
              </div>
              {useOtherRecipient && (
                <>
                  <label htmlFor={`send-to-${wallet.id}`}>Recipient account ID</label>
                  <input
                    id={`send-to-${wallet.id}`}
                    autoComplete="off"
                    placeholder="0.0.12345-abcde"
                    maxLength={32}
                    value={recipient}
                    onChange={(event) => setRecipient(event.target.value)}
                  />
                </>
              )}
              <input
                id={`send-amount-${wallet.id}`}
                aria-label={sendToken ? 'Amount' : 'Amount (ℏ)'}
                autoComplete="off"
                inputMode="decimal"
                placeholder={sendToken ? 'Enter send amount here' : 'Enter send amount here (ℏ)'}
                maxLength={20}
                required
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
              />
              <input
                id={`send-memo-${wallet.id}`}
                aria-label="Memo (optional)"
                autoComplete="off"
                maxLength={100}
                placeholder="Memo (optional)"
                value={memo}
                onChange={(event) => setMemo(event.target.value)}
              />
              <div className="button-pair">
                <button className="secondary-button" type="submit" disabled={isBusy}>
                  Review transfer
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('send')}>
                  Close
                </button>
              </div>
            </form>
          )}
        </div>
      )}

      {!account && isWatchOnly && (
        <p className="field-hint">
          Watch-only wallet: this app does not hold its private key. Anything that moves funds
          or changes accounts is signed on your offline signer.
        </p>
      )}

      {!account && (
        <div className="testnet-send create-account">
          <h3 className="section-title">Create account</h3>
          <p className="field-hint">
            {isWatchOnly
              ? 'Creates a new Testnet account that uses this wallet\u2019s public key. Your offline signer signs the request. It is added to this wallet automatically. Choose one of this wallet\u2019s accounts to fund it:'
              : 'Creates a new Testnet account controlled by this wallet\u2019s key. It is added to this wallet automatically. Choose an account from the dropdown box below to fund the account:'}
          </p>
          {signing ? (
            renderSigning('Submit signed account creation', () => {
              setSigning(null)
              setSignatureText('')
            })
          ) : confirmingCreate ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm new account">
              <dl className="create-confirm">
                <dt>Network</dt>
                <dd>Hedera Testnet (no real value)</dd>
                <dt>Funded from</dt>
                <dd>{funderLabel}</dd>
                {newAccountName.trim() && (
                  <>
                    <dt>Account name</dt>
                    <dd>{newAccountName.trim()}</dd>
                  </>
                )}
                <dt>Opening balance</dt>
                <dd>{openingBalance.trim()} ℏ</dd>
                <dt>Key</dt>
                <dd>This wallet&apos;s key</dd>
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <div className="button-pair">
                <button
                  className="primary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={isWatchOnly ? handlePrepareCreate : handleCreate}
                >
                  {isWatchOnly
                    ? isBusy ? 'Preparing...' : 'Create signing request'
                    : isBusy ? 'Creating...' : 'Sign and create'}
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => setConfirmingCreate(false)}>
                  Back
                </button>
              </div>
            </div>
          ) : (
            <form
              className="testnet-send-form"
              onSubmit={handleReviewCreate}
            >
              <label htmlFor={`create-name-${wallet.id}`}>Account name</label>
              <input
                id={`create-name-${wallet.id}`}
                autoComplete="off"
                maxLength={25}
                placeholder="Optional, up to 25 characters"
                value={newAccountName}
                onChange={(event) => setNewAccountName(event.target.value)}
              />
              <label htmlFor={`create-funder-${wallet.id}`}>Fund from</label>
              <select
                id={`create-funder-${wallet.id}`}
                required
                value={funder}
                onChange={(event) => setFunder(event.target.value)}
              >
                <option value="">Select an account</option>
                {funderRows.map((row) => (
                  <option key={`${row.network}/${row.id}`} value={row.display_id}>
                    {row.name} - {row.display_id}
                    {funderBalances[row.display_id] !== undefined
                      ? ` (${formatHbar(funderBalances[row.display_id], 2)} ℏ)`
                      : ''}
                  </option>
                ))}
              </select>
              <label htmlFor={`opening-balance-${wallet.id}`}>Opening balance (ℏ)</label>
              <input
                id={`opening-balance-${wallet.id}`}
                autoComplete="off"
                inputMode="decimal"
                placeholder="1.2345"
                maxLength={20}
                required
                value={openingBalance}
                onChange={(event) => setOpeningBalance(event.target.value)}
              />
              <button className="primary-button review-create-button" type="submit" disabled={isBusy}>
                Review new account
              </button>
            </form>
          )}
        </div>
      )}

      {account && !isWatchOnly && section === 'move' && otherWallets.length > 0 && (
        <div className="testnet-send">
          <h3 className="section-title">Move account to another wallet</h3>
          <p className="field-hint">
            Changes this account&apos;s key to another wallet&apos;s key, for example if this
            wallet&apos;s key may have been exposed. Both keys sign, so both wallets must be in
            this vault. After it is confirmed, the account belongs to the other wallet.
          </p>
          {pendingMove ? (
            <div className="wallet-tab-actions">
              <button className="secondary-button" type="button" disabled={isBusy} onClick={handleCheckMove}>
                Check again
              </button>
            </div>
          ) : confirmingMove ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm account move">
              <p className="testnet-warning" role="alert">
                <span aria-hidden="true">⚠ </span>
                Moving an account hands control of its funds to the other wallet&apos;s key. If
                that key is not backed up and secured, the funds can be lost permanently.
              </p>
              <dl>
                <dt>Account</dt>
                <dd>{accountLabel}</dd>
                <dt>New owner</dt>
                <dd>{targetName}</dd>
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <label className="testnet-acknowledge">
                <input
                  type="checkbox"
                  checked={moveAcknowledged}
                  onChange={(event) => setMoveAcknowledged(event.target.checked)}
                />
                I understand that the other wallet&apos;s key will control this account, and that
                it is backed up and secured.
              </label>
              <div className="button-pair">
                <button className="primary-button risky-button" type="button" disabled={isBusy || !moveAcknowledged} onClick={handleMove}>
                  {isBusy ? 'Moving...' : 'Sign and move'}
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={() => {
                    setConfirmingMove(false)
                    setMoveAcknowledged(false)
                  }}
                >
                  Back
                </button>
              </div>
            </div>
          ) : (
            <form
              className="testnet-send-form"
              onSubmit={(event) => {
                event.preventDefault()
                setError('')
                setNotice('')
                setConfirmingMove(true)
              }}
            >
              <label htmlFor={`move-target-${wallet.id}`}>New wallet</label>
              <select
                id={`move-target-${wallet.id}`}
                required
                value={moveTarget}
                onChange={(event) => setMoveTarget(event.target.value)}
              >
                <option value="">Choose a wallet</option>
                {otherWallets.map((item) => (
                  <option key={item.id} value={item.id}>{item.name}</option>
                ))}
              </select>
              <div className="button-pair">
                <button className="secondary-button" type="submit" disabled={isBusy}>
                  Review move
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('move')}>
                  Cancel
                </button>
              </div>
            </form>
          )}
        </div>
      )}

      {account && section === 'delete' && (
        <div className="testnet-send">
          <h3 className="section-title">Delete account</h3>
          <p className="field-hint">
            {isWatchOnly
              ? 'Permanently deletes this account and sends its remaining ℏ to another account of this wallet. Your offline signer signs the request.'
              : 'Permanently deletes this account and sends its remaining ℏ to another account. The recipient may be controlled by the same key. If the recipient is in this vault its key signs too.'}
          </p>
          {pendingDelete ? (
            <div className="wallet-tab-actions">
              <button className="secondary-button" type="button" disabled={isBusy} onClick={handleCheckDelete}>
                Check again
              </button>
            </div>
          ) : signing ? (
            renderSigning('Submit signed deletion', () => {
              setSigning(null)
              setSignatureText('')
            })
          ) : confirmingDelete ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm account deletion">
              <p className="testnet-warning" role="alert">
                <span aria-hidden="true">⚠ </span>
                Deleting an account cannot be undone. Any ℏ left in it goes to the recipient below;
                check the recipient carefully, because a wrong account means the funds are gone.
              </p>
              <dl>
                <dt>Account to delete</dt>
                <dd>{accountLabel}</dd>
                <dt>Remaining ℏ go to</dt>
                <dd>{deleteRecipient.trim()}</dd>
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <label className="testnet-acknowledge">
                <input
                  type="checkbox"
                  checked={deleteAcknowledged}
                  onChange={(event) => setDeleteAcknowledged(event.target.checked)}
                />
                I understand that deleting this account is permanent and that its remaining
                balance will be sent to the recipient.
              </label>
              <div className="button-pair">
                <button
                  className="primary-button risky-button"
                  type="button"
                  disabled={isBusy || !deleteAcknowledged}
                  onClick={isWatchOnly ? handlePrepareDelete : handleDelete}
                >
                  {isWatchOnly
                    ? isBusy ? 'Preparing...' : 'Create signing request'
                    : isBusy ? 'Deleting...' : 'Sign and delete'}
                </button>
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBusy}
                  onClick={() => {
                    setConfirmingDelete(false)
                    setDeleteAcknowledged(false)
                  }}
                >
                  Back
                </button>
              </div>
            </div>
          ) : (
            <form
              className="testnet-send-form"
              onSubmit={(event) => {
                event.preventDefault()
                setError('')
                setNotice('')
                if (!deleteRecipient.trim()) {
                  setError(useOtherDeleteRecipient ? 'Enter a recipient account.' : 'Choose a recipient from the address book.')
                  return
                }
                if (sameAccount(deleteRecipient)) {
                  setError('The recipient must be a different account.')
                  return
                }
                void run(async () => {
                  if (!isWatchOnly) {
                    await invoke<string>('check_recipient', {
                      recipient: deleteRecipient,
                      forSend: false,
                      forDelete: true,
                    })
                  }
                  setConfirmingDelete(true)
                })
              }}
            >
              <label htmlFor={`delete-book-${wallet.id}`}>
                {isWatchOnly ? 'Recipient (another account of this wallet)' : 'Recipient from the address book'}
              </label>
              <select
                id={`delete-book-${wallet.id}`}
                disabled={useOtherDeleteRecipient}
                value={useOtherDeleteRecipient ? '' : deleteRecipient}
                onChange={(event) => setDeleteRecipient(event.target.value)}
              >
                <option value="">Select an account</option>
                {addressBook.rows
                  .filter((row) => row.id !== account && (!isWatchOnly || row.wallet_id === wallet.id))
                  .map((row) => (
                    <option key={`${row.network}/${row.id}`} value={row.display_id}>
                      {row.name} {row.display_id}
                    </option>
                  ))}
              </select>
              {!isWatchOnly && (
                <label
                  className={`testnet-acknowledge${addressBook.allowExternalDeletes ? '' : ' disabled-text'}`}
                  htmlFor={`delete-other-${wallet.id}`}
                >
                  <input
                    id={`delete-other-${wallet.id}`}
                    type="checkbox"
                    disabled={!addressBook.allowExternalDeletes}
                    checked={useOtherDeleteRecipient}
                    onChange={(event) => {
                      setUseOtherDeleteRecipient(event.target.checked)
                      setDeleteRecipient('')
                    }}
                  />
                  <span>Check to send to a different account</span>
                </label>
              )}
              {!isWatchOnly && !addressBook.allowExternalDeletes && (
                <p className="field-hint disabled-text">
                  Funds can only go to accounts in the address book. Remove this limit in the Settings menu.
                </p>
              )}
              {useOtherDeleteRecipient && (
                <>
                  <label htmlFor={`delete-recipient-${wallet.id}`}>Recipient account ID</label>
                  <input
                    id={`delete-recipient-${wallet.id}`}
                    autoComplete="off"
                    placeholder="0.0.12345-abcde"
                    maxLength={32}
                    value={deleteRecipient}
                    onChange={(event) => setDeleteRecipient(event.target.value)}
                  />
                </>
              )}
              <div className="button-pair">
                <button className="secondary-button" type="submit" disabled={isBusy}>
                  Review deletion
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => toggleSection('delete')}>
                  Cancel
                </button>
              </div>
            </form>
          )}
        </div>
      )}

      {overview && (
        <div className="testnet-overview">
          <div className="testnet-overview-heading">
            {refreshButton('Refresh transactions')}
            <h3>{showAssets ? 'Account assets' : 'Recent transactions'}</h3>
          </div>
          {showAssets && hiddenHere > 0 && (
            <button
              className="link-button"
              type="button"
              onClick={() => setShowHidden((current) => !current)}
            >
              {showHidden ? 'Hide hidden tokens' : `Show hidden tokens (${hiddenHere})`}
            </button>
          )}
          {showAssets ? (
            <ul className="testnet-transactions testnet-assets" aria-label="Account assets">
              <li>
                <span className="testnet-kind">HBAR</span>
                <span className="testnet-token-id" />
                <span className="testnet-net">{formatHbar(overview.balance_tinybars, 2)} ℏ</span>
              </li>
              {assetTokens.map((token) => (
                <li key={token.token_id}>
                  <span className="testnet-kind">
                    {tokenLabel(token)}
                    {token.name && token.symbol ? (
                      <span className="testnet-token-name"> {token.name}</span>
                    ) : null}
                    {hiddenTokens.includes(token.token_id) ? (
                      <span className="testnet-token-name"> (hidden)</span>
                    ) : null}
                  </span>
                  <span className="testnet-token-id">{token.token_id}</span>
                  <span className="testnet-net">{formatTokenAmount(token.balance, token.decimals)}</span>
                </li>
              ))}
            </ul>
          ) : overview.transactions.length === 0 ? (
            <p className="field-hint">No transactions yet.</p>
          ) : (
            <ul className="testnet-transactions" aria-label="Recent transactions">
              {overview.transactions.map((transaction) => (
                <li key={transaction.transaction_id}>
                  <span className="testnet-kind">{transaction.kind}</span>
                  <span className="testnet-counterparty">{transaction.counterparty ?? ''}</span>
                  <span className="testnet-result">{transaction.result}</span>
                  <span className="testnet-time">
                    {transaction.consensus_time
                      ? new Date(transaction.consensus_time).toLocaleString()
                      : ''}
                  </span>
                  <span className="testnet-net">{formatNet(transaction.net_tinybars)}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </section>
  )
}

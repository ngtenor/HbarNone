import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { WalletSummary } from './types'

type TransactionSummary = {
  transaction_id: string
  kind: string
  result: string
  consensus_time: string
  net_tinybars: number
}

type AccountOverview = {
  account_id: string
  balance_tinybars: number
  transactions: TransactionSummary[]
}

const TINYBARS_PER_HBAR = 100_000_000

function formatHbar(tinybars: number) {
  return (tinybars / TINYBARS_PER_HBAR).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 8,
  })
}

function formatNet(tinybars: number) {
  const sign = tinybars > 0 ? '+' : ''
  return `${sign}${formatHbar(tinybars)} ℏ`
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

type Props = {
  wallet: WalletSummary
  onWalletUpdated: (wallet: WalletSummary) => void
  onBalance?: (walletId: string, accountId: string, tinybars: number) => void
}

export function TestnetPanel({ wallet, onWalletUpdated, onBalance }: Props) {
  const [overview, setOverview] = useState<AccountOverview | null>(null)
  const [accountInput, setAccountInput] = useState('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [isBusy, setIsBusy] = useState(false)
  const [recipient, setRecipient] = useState('')
  const [amount, setAmount] = useState('')
  const [memo, setMemo] = useState('')
  const [confirming, setConfirming] = useState(false)
  const refreshTimer = useRef(0)
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

  const handleFind = () => run(async () => {
    const updated = await invoke<WalletSummary>('find_wallet_account', { walletId: wallet.id })
    onWalletUpdated(updated)
    setOverview(null)
  })

  const handleLink = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    return run(async () => {
      const updated = await invoke<WalletSummary>('link_wallet_account', {
        walletId: wallet.id,
        accountId: accountInput,
      })
      onWalletUpdated(updated)
      setAccountInput('')
      setOverview(null)
    })
  }

  const handleUnlink = () => run(async () => {
    const updated = await invoke<WalletSummary>('link_wallet_account', {
      walletId: wallet.id,
      accountId: null,
    })
    onWalletUpdated(updated)
    setOverview(null)
  })

  const handleRefresh = () => run(async () => {
    const result = await invoke<AccountOverview | null>('get_account_overview', {
      walletId: wallet.id,
    })
    setOverview(result)
    if (result && wallet.account_id) {
      onBalance?.(wallet.id, wallet.account_id, result.balance_tinybars)
    }
    if (!result) setNotice('This account was not found on Hedera Testnet.')
  })

  // Opening a wallet (or linking an account) refreshes its balance.
  useEffect(() => {
    if (wallet.account_id) void handleRefresh()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallet.id, wallet.account_id])

  useEffect(() => () => window.clearTimeout(refreshTimer.current), [])

  const resetSend = () => {
    setRecipient('')
    setAmount('')
    setMemo('')
    setConfirming(false)
  }

  const handleReview = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    setError('')
    setNotice('')
    if (recipient.trim() === wallet.account_id) {
      setError('The recipient must be a different account.')
      return
    }
    setConfirming(true)
  }

  const handleSend = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('send_hbar', {
      walletId: wallet.id,
      recipient: recipient.trim(),
      amountHbar: amount.trim(),
      memo,
    })
    resetSend()
    setNotice(
      `Submitted ${result.transactionId}. Testnet usually confirms within a few seconds; ` +
        'the balance refreshes automatically.',
    )
    window.clearTimeout(refreshTimer.current)
    refreshTimer.current = window.setTimeout(() => void handleRefresh(), 8000)
  })

  return (
    <section className="panel testnet-panel" aria-labelledby={`testnet-heading-${wallet.id}`}>
      <div className="panel-heading">
        <div>
          <p className="eyebrow">HEDERA TESTNET</p>
          <h2 id={`testnet-heading-${wallet.id}`}>Account and balance</h2>
        </div>
      </div>
      <p className="dialog-copy">
        The app contacts Hedera&apos;s public Testnet mirror node when the vault unlocks, when
        you open a wallet and when you click a button here, which tells its operator your
        account IDs and IP address. Sending HBAR also contacts a Testnet node directly.
        Transfers are signed inside the app and only ever go to Testnet, which has no value.
      </p>

      {wallet.account_id ? (
        <>
          <p className="testnet-account">
            Account <strong>{wallet.account_id}</strong>
          </p>
          <div className="wallet-tab-actions">
            <button className="secondary-button" type="button" disabled={isBusy} onClick={handleRefresh}>
              {isBusy ? 'Contacting Testnet...' : 'Refresh balance'}
            </button>
            <button className="secondary-button" type="button" disabled={isBusy} onClick={handleUnlink}>
              Unlink account
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="field-hint">
            No account is linked. A new key has no Hedera account until it is funded.
          </p>
          <div className="wallet-tab-actions">
            <button className="secondary-button" type="button" disabled={isBusy} onClick={handleFind}>
              {isBusy ? 'Contacting Testnet...' : 'Find account from this wallet’s key'}
            </button>
          </div>
          <form className="testnet-link-form" onSubmit={handleLink}>
            <label htmlFor={`account-id-${wallet.id}`}>Or link an account ID</label>
            <input
              id={`account-id-${wallet.id}`}
              autoComplete="off"
              placeholder="0.0.12345"
              maxLength={32}
              required
              value={accountInput}
              onChange={(event) => setAccountInput(event.target.value)}
            />
            <button className="secondary-button" type="submit" disabled={isBusy}>
              Link account
            </button>
          </form>
        </>
      )}

      {error && <p className="error-message" role="alert">{error}</p>}
      {notice && <p className="field-hint" role="status">{notice}</p>}

      {wallet.account_id && (
        <div className="testnet-send">
          <h3>Send HBAR</h3>
          {confirming ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm transfer">
              <p>Please check the details. A transfer cannot be undone.</p>
              <dl>
                <dt>Network</dt>
                <dd>Hedera Testnet (no real value)</dd>
                <dt>From</dt>
                <dd>{wallet.account_id}</dd>
                <dt>To</dt>
                <dd>{recipient.trim()}</dd>
                <dt>Amount</dt>
                <dd>{amount.trim()} ℏ</dd>
                {memo && (
                  <>
                    <dt>Memo</dt>
                    <dd>{memo}</dd>
                  </>
                )}
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <div className="wallet-tab-actions">
                <button className="primary-button" type="button" disabled={isBusy} onClick={handleSend}>
                  {isBusy ? 'Sending...' : 'Sign and send'}
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => setConfirming(false)}>
                  Back
                </button>
              </div>
            </div>
          ) : (
            <form className="testnet-send-form" onSubmit={handleReview}>
              <label htmlFor={`send-to-${wallet.id}`}>Recipient account ID</label>
              <input
                id={`send-to-${wallet.id}`}
                autoComplete="off"
                placeholder="0.0.12345"
                maxLength={32}
                required
                value={recipient}
                onChange={(event) => setRecipient(event.target.value)}
              />
              <label htmlFor={`send-amount-${wallet.id}`}>Amount (ℏ)</label>
              <input
                id={`send-amount-${wallet.id}`}
                autoComplete="off"
                inputMode="decimal"
                placeholder="1.5"
                maxLength={20}
                required
                value={amount}
                onChange={(event) => setAmount(event.target.value)}
              />
              <label htmlFor={`send-memo-${wallet.id}`}>Memo (optional)</label>
              <input
                id={`send-memo-${wallet.id}`}
                autoComplete="off"
                maxLength={100}
                value={memo}
                onChange={(event) => setMemo(event.target.value)}
              />
              <button className="secondary-button" type="submit" disabled={isBusy}>
                Review transfer
              </button>
            </form>
          )}
        </div>
      )}

      {overview && (
        <div className="testnet-overview">
          <p className="testnet-balance">
            Balance <strong>{formatHbar(overview.balance_tinybars)} ℏ</strong>
          </p>
          {overview.transactions.length === 0 ? (
            <p className="field-hint">No transactions yet.</p>
          ) : (
            <ul className="testnet-transactions" aria-label="Recent transactions">
              {overview.transactions.map((transaction) => (
                <li key={transaction.transaction_id}>
                  <span className="testnet-kind">{transaction.kind}</span>
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

import { useState } from 'react'
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
}

export function TestnetPanel({ wallet, onWalletUpdated }: Props) {
  const [overview, setOverview] = useState<AccountOverview | null>(null)
  const [accountInput, setAccountInput] = useState('')
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [isBusy, setIsBusy] = useState(false)

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
    if (!result) setNotice('This account was not found on Hedera Testnet.')
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
        Read-only. The app contacts Hedera&apos;s public Testnet mirror node only when you
        click a button here, which tells its operator your account ID and IP address.
        Testnet HBAR has no value.
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

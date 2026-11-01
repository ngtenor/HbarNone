import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { AddressBookView, WalletSummary } from './types'

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
  account: string | null
  otherWallets: WalletSummary[]
  addressBook: AddressBookView
  onWalletUpdated: (wallet: WalletSummary) => void
  onAccountMoved: (target: WalletSummary, sourceWalletId: string, accountId: string) => void
  onAccountDeleted: (updated: WalletSummary, accountId: string) => void
  onBalance?: (walletId: string, accountId: string, tinybars: number) => void
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
  const [funder, setFunder] = useState('')
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
  const [section, setSection] = useState<'send' | 'rename' | 'move' | 'delete' | null>(null)
  const toggleSection = (next: 'send' | 'rename' | 'move' | 'delete') => {
    setSection((current) => (current === next ? null : next))
    // Leaving a section discards anything typed into it and any pending review.
    setRecipient('')
    setUseOtherRecipient(false)
    setAmount('')
    setMemo('')
    setConfirming(false)
    setMoveTarget('')
    setConfirmingMove(false)
    setMoveAcknowledged(false)
    setDeleteRecipient('')
    setUseOtherDeleteRecipient(false)
    setConfirmingDelete(false)
    setDeleteAcknowledged(false)
    setError('')
    setNotice('')
  }
  const moveTimer = useRef(0)
  const deleteTimer = useRef(0)
  const refreshTimer = useRef(0)
  const createTimer = useRef(0)
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

  const handleRefresh = () => run(async () => {
    if (!account) return
    const result = await invoke<AccountOverview | null>('get_account_overview', {
      walletId: wallet.id,
      accountId: account,
    })
    setOverview(result)
    if (result) onBalance?.(wallet.id, account, result.balance_tinybars)
    if (!result) setNotice('This account was not found on Hedera Testnet.')
  })

  // Selecting an account refreshes its balance; the panel remounts per account.
  useEffect(() => {
    if (account) void handleRefresh()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wallet.id, account])

  useEffect(() => () => {
    window.clearTimeout(refreshTimer.current)
    window.clearTimeout(createTimer.current)
    window.clearTimeout(moveTimer.current)
    window.clearTimeout(deleteTimer.current)
  }, [])

  const targetName = otherWallets.find((item) => item.id === moveTarget)?.name ?? ''
  const accountSummary = wallet.accounts.find((item) => item.id === account)
  const accountLabel = accountSummary
    ? `${accountSummary.name} ${accountSummary.display_id}`
    : account
  const sameAccount = (typed: string) => typed.trim().split('-')[0] === account
  const funderRows = addressBook.rows.filter((row) => row.network === 'testnet' && row.wallet_id)
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

  // Testnet needs a few seconds to reach consensus on the key change.
  async function finishMove(target: string, attemptsLeft: number) {
    if (!account) return
    try {
      const updated = await invoke<WalletSummary>('finish_account_move', {
        walletId: wallet.id,
        accountId: account,
        targetWalletId: target,
      })
      onAccountMoved(updated, wallet.id, account)
    } catch (failure) {
      if (attemptsLeft > 0) {
        moveTimer.current = window.setTimeout(() => void finishMove(target, attemptsLeft - 1), 6000)
      } else {
        setError(`${errorMessage(failure)} Use “Check again” in a moment.`)
        setPendingMove(target)
      }
    }
  }

  async function finishDelete(attemptsLeft: number) {
    if (!account) return
    try {
      const updated = await invoke<WalletSummary>('finish_account_delete', {
        walletId: wallet.id,
        accountId: account,
      })
      onAccountDeleted(updated, account)
    } catch (failure) {
      if (attemptsLeft > 0) {
        deleteTimer.current = window.setTimeout(() => void finishDelete(attemptsLeft - 1), 6000)
      } else {
        setError(`${errorMessage(failure)} Use “Check again” in a moment.`)
        setPendingDelete(true)
      }
    }
  }

  const handleDelete = () => run(async () => {
    await invoke<{ transactionId: string }>('delete_account', {
      walletId: wallet.id,
      accountId: account,
      recipient: deleteRecipient.trim(),
    })
    setConfirmingDelete(false)
    setDeleteAcknowledged(false)
    setNotice('Deletion submitted. Waiting for Testnet to confirm it...')
    window.clearTimeout(deleteTimer.current)
    deleteTimer.current = window.setTimeout(() => void finishDelete(3), 8000)
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
    await invoke<{ transactionId: string }>('move_account', {
      walletId: wallet.id,
      accountId: account,
      targetWalletId: moveTarget,
    })
    setConfirmingMove(false)
    setMoveAcknowledged(false)
    setNotice('Key change submitted. Waiting for Testnet to confirm it...')
    window.clearTimeout(moveTimer.current)
    moveTimer.current = window.setTimeout(() => void finishMove(moveTarget, 3), 8000)
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
  }

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

  const handleSend = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('send_hbar', {
      walletId: wallet.id,
      fromAccount: account,
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

  const handleCreate = () => run(async () => {
    const result = await invoke<{ transactionId: string }>('create_account', {
      walletId: wallet.id,
      fromAccount: funder,
      initialHbar: openingBalance.trim(),
    })
    setOpeningBalance('')
    setConfirmingCreate(false)
    setNotice(
      `Submitted ${result.transactionId}. The new account appears here in a few seconds ` +
        'once Testnet confirms it.',
    )
    window.clearTimeout(createTimer.current)
    createTimer.current = window.setTimeout(() => {
      void invoke<WalletSummary>('find_wallet_accounts', { walletId: wallet.id })
        .then(onWalletUpdated)
        .catch(() => undefined)
    }, 8000)
  })

  return (
    <section className={`panel testnet-panel${account ? '' : ' create-panel'}`} aria-label={account ? 'Account' : 'Create account'}>
      {account ? (
        <>
          <div className="account-heading">
            <h2 className="section-title">Account: {accountLabel}</h2>
            <strong className="account-heading-balance">
              {overview ? `${formatHbar(overview.balance_tinybars)} ℏ` : ''}
            </strong>
          </div>
          <div className="wallet-tab-actions account-actions">
            <button
              className="secondary-button"
              type="button"
              aria-expanded={section === 'send'}
              onClick={() => toggleSection('send')}
            >
              Send
            </button>
            <button className="secondary-button" type="button" disabled={isBusy} onClick={handleRefresh}>
              {isBusy ? 'Contacting Testnet...' : 'Refresh balance'}
            </button>
            <button
              className="secondary-button"
              type="button"
              onClick={() => {
                setNameInput(accountSummary?.named ? accountSummary.name : '')
                toggleSection('rename')
              }}
            >
              {accountSummary?.named ? 'Rename account' : 'Name this account'}
            </button>
            {otherWallets.length > 0 && (
              <button
                className="secondary-button"
                type="button"
                aria-expanded={section === 'move'}
                onClick={() => toggleSection('move')}
              >
                Move account
              </button>
            )}
            <button
              className="secondary-button"
              type="button"
              aria-expanded={section === 'delete'}
              onClick={() => toggleSection('delete')}
            >
              Delete account
            </button>
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

      {account && section === 'send' && (
        <div className="testnet-send">
          <h3 className="section-title">Send HBAR</h3>
          {confirming ? (
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
              <div className="button-pair">
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
              <label htmlFor={`send-book-${wallet.id}`}>Recipient from the address book</label>
              <select
                id={`send-book-${wallet.id}`}
                disabled={useOtherRecipient}
                value={useOtherRecipient ? '' : recipient}
                onChange={(event) => setRecipient(event.target.value)}
              >
                <option value="">Select an account</option>
                {addressBook.rows
                  .filter((row) => row.id !== account)
                  .map((row) => (
                    <option key={`${row.network}/${row.id}`} value={row.display_id}>
                      {row.name} {row.display_id}
                    </option>
                  ))}
              </select>
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
              <label htmlFor={`send-amount-${wallet.id}`}>Amount (ℏ)</label>
              <input
                id={`send-amount-${wallet.id}`}
                autoComplete="off"
                inputMode="decimal"
                placeholder="1.2345"
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

      {!account && (
        <div className="testnet-send create-account">
          <h3 className="section-title">Create account</h3>
          <p className="field-hint">
            Creates a new Testnet account controlled by this wallet&apos;s key. It is added to
            this wallet automatically. Choose an account from the dropdown box below to fund the
            account:
          </p>
          {confirmingCreate ? (
            <div className="testnet-confirm" role="group" aria-label="Confirm new account">
              <dl className="create-confirm">
                <dt>Network</dt>
                <dd>Hedera Testnet (no real value)</dd>
                <dt>Funded from</dt>
                <dd>{funderLabel}</dd>
                <dt>Opening balance</dt>
                <dd>{openingBalance.trim()} ℏ</dd>
                <dt>Key</dt>
                <dd>This wallet&apos;s key</dd>
                <dt>Maximum fee</dt>
                <dd>1 ℏ (the real fee is far lower)</dd>
              </dl>
              <div className="button-pair">
                <button className="primary-button" type="button" disabled={isBusy} onClick={handleCreate}>
                  {isBusy ? 'Creating...' : 'Sign and create'}
                </button>
                <button className="secondary-button" type="button" disabled={isBusy} onClick={() => setConfirmingCreate(false)}>
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
                setConfirmingCreate(true)
              }}
            >
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
                    {row.name} {row.display_id}
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

      {account && section === 'move' && otherWallets.length > 0 && (
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
                <button className="primary-button" type="button" disabled={isBusy || !moveAcknowledged} onClick={handleMove}>
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
              <button className="secondary-button" type="submit" disabled={isBusy}>
                Review move
              </button>
            </form>
          )}
        </div>
      )}

      {account && section === 'delete' && (
        <div className="testnet-send">
          <h3 className="section-title">Delete account</h3>
          <p className="field-hint">
            Permanently deletes this account and sends its remaining ℏ to another account. The
            recipient may be controlled by the same key. If the recipient is in this vault its key
            signs too.
          </p>
          {pendingDelete ? (
            <div className="wallet-tab-actions">
              <button className="secondary-button" type="button" disabled={isBusy} onClick={handleCheckDelete}>
                Check again
              </button>
            </div>
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
                <button className="primary-button" type="button" disabled={isBusy || !deleteAcknowledged} onClick={handleDelete}>
                  {isBusy ? 'Deleting...' : 'Sign and delete'}
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
                  await invoke<string>('check_recipient', {
                    recipient: deleteRecipient,
                    forSend: false,
                    forDelete: true,
                  })
                  setConfirmingDelete(true)
                })
              }}
            >
              <label htmlFor={`delete-book-${wallet.id}`}>Recipient from the address book</label>
              <select
                id={`delete-book-${wallet.id}`}
                disabled={useOtherDeleteRecipient}
                value={useOtherDeleteRecipient ? '' : deleteRecipient}
                onChange={(event) => setDeleteRecipient(event.target.value)}
              >
                <option value="">Select an account</option>
                {addressBook.rows
                  .filter((row) => row.id !== account)
                  .map((row) => (
                    <option key={`${row.network}/${row.id}`} value={row.display_id}>
                      {row.name} {row.display_id}
                    </option>
                  ))}
              </select>
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
              {!addressBook.allowExternalDeletes && (
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
              <button className="secondary-button" type="submit" disabled={isBusy}>
                Review deletion
              </button>
            </form>
          )}
        </div>
      )}

      {overview && (
        <div className="testnet-overview">
          <h3>Recent transactions</h3>
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

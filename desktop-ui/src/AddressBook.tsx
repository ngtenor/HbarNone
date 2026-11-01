import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { AddressBookView } from './types'

const MAX_NAME_CHARS = 25
const TINYBARS_PER_HBAR = 100_000_000

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatHbar(tinybars: number) {
  return `${(tinybars / TINYBARS_PER_HBAR).toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })} ℏ`
}

type Props = {
  view: AddressBookView
  onChange: (view: AddressBookView) => void
  onRenamed: (accountId: string, name: string) => void
}

const UNNAMED = 'NONAME'

export function AddressBook({ view, onChange, onRenamed }: Props) {
  const [editing, setEditing] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [name, setName] = useState('')
  const [address, setAddress] = useState('')
  const [error, setError] = useState('')
  const [isBusy, setIsBusy] = useState(false)
  const [balances, setBalances] = useState<Record<string, number | null>>({})
  const idsKey = view.rows.map((row) => row.id).join(',')

  // Balances come from the public mirror node, one account at a time.
  useEffect(() => {
    let cancelled = false
    void (async () => {
      for (const id of idsKey ? idsKey.split(',') : []) {
        try {
          const tinybars = await invoke<number | null>('get_address_balance', { accountId: id })
          if (cancelled) return
          setBalances((existing) => ({ ...existing, [id]: tinybars }))
        } catch {
          if (cancelled) return
        }
      }
    })()
    return () => {
      cancelled = true
    }
  }, [idsKey])

  async function handleAdd(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (isBusy) return
    setError('')
    setIsBusy(true)
    try {
      const updated = await invoke<AddressBookView>('add_address_book_entry', {
        name,
        address,
      })
      onChange(updated)
      setName('')
      setAddress('')
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setIsBusy(false)
    }
  }

  async function handleRename(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (editing === null) return
    setError('')
    try {
      const updated = await invoke<AddressBookView>('rename_address_book_account', {
        accountId: editing,
        name: editName,
      })
      onChange(updated)
      onRenamed(editing, editName.trim())
      setEditing(null)
    } catch (failure) {
      setError(errorMessage(failure))
    }
  }

  function renderRow(row: AddressBookView['rows'][number]) {
    return (
      <li className="address-row" key={`${row.network}/${row.id}`}>
        {editing === row.id ? (
          <form className="address-rename" onSubmit={handleRename} noValidate>
            <input
              aria-label={`Name for ${row.display_id}`}
              autoComplete="off"
              maxLength={MAX_NAME_CHARS}
              value={editName}
              onChange={(event) => setEditName(event.target.value)}
            />
            <button className="secondary-button" type="submit">Save</button>
            <button className="secondary-button" type="button" onClick={() => setEditing(null)}>
              Cancel
            </button>
          </form>
        ) : (
          <button
            className="link-button account-name"
            type="button"
            title="Click to rename"
            onClick={() => {
              setEditing(row.id)
              setEditName(row.name === UNNAMED ? '' : row.name)
              setError('')
            }}
          >
            {row.name}
          </button>
        )}
        <span>{row.wallet_name ?? 'Outside the vault'}</span>
        <span className="account-number">{row.display_id}</span>
        <span className="account-balance">
          {row.id in balances
            ? balances[row.id] === null ? 'not found' : formatHbar(balances[row.id] as number)
            : '…'}
        </span>
        {row.wallet_id === null ? (
          <button
            className="secondary-button"
            type="button"
            aria-label={`Remove ${row.name} from the address book`}
            onClick={() => void handleRemove(row.id)}
          >
            Remove
          </button>
        ) : (
          <span />
        )}
      </li>
    )
  }

  async function handleRemove(accountId: string) {
    setError('')
    try {
      onChange(await invoke<AddressBookView>('remove_address_book_entry', { accountId }))
    } catch (failure) {
      setError(errorMessage(failure))
    }
  }

  const namedRows = view.rows.filter((row) => row.name !== UNNAMED)
  const unnamedRows = view.rows.filter((row) => row.name === UNNAMED)

  return (
    <section
      className="vault-tab-panel settings-tab-panel"
      id="panel-addressbook"
      role="tabpanel"
      aria-labelledby="tab-addressbook"
    >
      <div className="settings-heading">
        <p className="eyebrow">RECIPIENTS</p>
        <h1>Address book</h1>
      </div>

      <section className="panel settings-panel" aria-labelledby="address-book-add-heading">
        <h2 id="address-book-add-heading">Add an outside account</h2>
        <p className="dialog-copy">
          Accounts in your wallets are listed automatically. Add other people&apos;s accounts
          here, with their full account number including the checksum. The account is checked on
          Testnet before it is added.
        </p>
        <form className="testnet-send-form" onSubmit={handleAdd} noValidate>
          <label htmlFor="address-book-name">Name</label>
          <input
            id="address-book-name"
            autoComplete="off"
            maxLength={MAX_NAME_CHARS}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <label htmlFor="address-book-address">Account number with checksum</label>
          <input
            id="address-book-address"
            autoComplete="off"
            placeholder="0.0.12345-abcde"
            maxLength={32}
            value={address}
            onChange={(event) => setAddress(event.target.value)}
          />
          {error && <p className="error-message" role="alert">{error}</p>}
          <button className="secondary-button" type="submit" disabled={isBusy}>
            {isBusy ? 'Checking...' : 'Add to address book'}
          </button>
        </form>
      </section>

      <section className="panel settings-panel" aria-label="Address book entries">
        <div className="address-columns" aria-hidden="true">
          <span>Name</span>
          <span>Wallet</span>
          <span>Account</span>
          <span>Balance</span>
          <span />
        </div>
        {view.rows.length === 0 ? (
          <p className="empty-state">No accounts yet.</p>
        ) : (
          <>
            <ul className="address-rows" aria-label="Address book">
              {namedRows.map(renderRow)}
            </ul>
            {unnamedRows.length > 0 && (
              <>
                <h3 className="address-subheading">Unnamed accounts</h3>
                <ul className="address-rows" aria-label="Unnamed accounts">
                  {unnamedRows.map(renderRow)}
                </ul>
              </>
            )}
          </>
        )}
      </section>
    </section>
  )
}

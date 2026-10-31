import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App'

type Wallet = { id: string; name: string; created_at: string; account_id: string | null }

type Backend = {
  status: 'uninitialized' | 'locked' | 'unlocked'
  wallets: Wallet[]
  autoLockSeconds: number
  backupWallets: Wallet[]
  exportSaved: boolean
  restoreCancelled: boolean
  foundAccount: string | null
  overview: unknown
  detection: { keyType: string | null; accounts: string[] }
  calls: { cmd: string; args?: Record<string, unknown> }[]
}

const PHRASE = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima'

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

function wallet(id: string, name: string, account_id: string | null = null): Wallet {
  return { id, name, created_at: '2026-01-01T00:00:00Z', account_id }
}

let backend: Backend

function vaultState() {
  return {
    status: backend.status,
    wallets: backend.status === 'unlocked' ? backend.wallets : [],
    legacyWallets: [],
    autoLockSeconds: backend.autoLockSeconds,
  }
}

function installBackend(overrides: Partial<Backend> = {}) {
  backend = {
    status: 'locked',
    wallets: [wallet('w1', 'Savings'), wallet('w2', 'Trading')],
    autoLockSeconds: 300,
    backupWallets: [wallet('b1', 'Restored')],
    exportSaved: true,
    restoreCancelled: false,
    foundAccount: null,
    overview: null,
    detection: { keyType: null, accounts: [] },
    calls: [],
    ...overrides,
  }
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    backend.calls.push({ cmd, args })
    switch (cmd) {
      case 'get_vault_state':
        return vaultState()
      case 'unlock_vault':
        if (args?.vaultPassword !== 'correct password') throw new Error('incorrect vault password')
        backend.status = 'unlocked'
        return vaultState()
      case 'lock_vault':
        backend.status = 'locked'
        return undefined
      case 'record_activity':
      case 'set_auto_lock_seconds':
        return args?.seconds ?? undefined
      case 'reveal_wallet_mnemonic':
        return PHRASE
      case 'delete_wallet':
        backend.wallets = backend.wallets.filter((item) => item.id !== args?.walletId)
        return undefined
      case 'rename_wallet': {
        const name = String(args?.name).trim()
        if (backend.wallets.some((item) => item.id !== args?.walletId
          && item.name.toLowerCase() === name.toLowerCase())) {
          throw new Error(`a wallet named '${name}' already exists`)
        }
        const target = backend.wallets.find((item) => item.id === args?.walletId)!
        target.name = name
        return { ...target }
      }
      case 'link_wallet_account': {
        const target = backend.wallets.find((item) => item.id === args?.walletId)!
        const accountId = args?.accountId as string | null
        if (accountId !== null && !/^0\.0\.[1-9]\d*$/.test(accountId.trim())) {
          throw new Error('account ID must look like 0.0.12345')
        }
        target.account_id = accountId === null ? null : accountId.trim()
        return { ...target }
      }
      case 'find_wallet_account': {
        if (!backend.foundAccount) throw new Error('no Testnet account uses this wallet\'s key yet')
        const target = backend.wallets.find((item) => item.id === args?.walletId)!
        target.account_id = backend.foundAccount
        return { ...target }
      }
      case 'get_account_overview':
        return backend.overview
      case 'get_wallet_keys':
        return { publicKey: 'ab'.repeat(32), evmAddress: null }
      case 'detect_private_key_type':
        return backend.detection
      case 'import_private_key': {
        if (backend.wallets.some((item) => item.name === args?.name)) {
          throw new Error('name taken')
        }
        const created = {
          ...wallet(`k${backend.wallets.length + 1}`, String(args?.name)),
          secret_kind: 'privatekey',
          key_type: String(args?.keyType),
        }
        backend.wallets.push(created)
        return { ...created }
      }
      case 'export_vault_backup':
        return backend.exportSaved
      case 'backup_and_erase_vault':
        if (!backend.exportSaved) return null
        backend.wallets = []
        backend.status = 'uninitialized'
        return { status: 'uninitialized', wallets: [], legacyWallets: [], autoLockSeconds: 300 }
      case 'restore_vault_backup':
        if (backend.restoreCancelled) return null
        if (args?.backupPassword !== 'backup password') {
          throw new Error('incorrect backup password or damaged backup')
        }
        backend.wallets = backend.backupWallets
        backend.status = 'unlocked'
        return vaultState()
      default:
        throw new Error(`unexpected command: ${cmd}`)
    }
  })
}

function commandsCalled(cmd: string) {
  return backend.calls.filter((call) => call.cmd === cmd)
}

async function unlock(user: ReturnType<typeof userEvent.setup>) {
  await user.type(await screen.findByLabelText('Vault password'), 'correct password')
  await user.click(screen.getByRole('button', { name: 'Unlock vault' }))
  await screen.findByRole('tab', { name: 'HOME' })
}

async function openWalletTab(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole('button', { name: new RegExp(name) }))
  await screen.findByRole('heading', { level: 1, name })
}

describe('vault gates', () => {
  beforeEach(() => installBackend())

  it('shows the unlock form for a locked vault and reports a wrong password', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.type(await screen.findByLabelText('Vault password'), 'wrong')
    await user.click(screen.getByRole('button', { name: 'Unlock vault' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('incorrect vault password')
    expect(screen.queryByRole('tab', { name: 'HOME' })).not.toBeInTheDocument()
  })

  it('shows a locked padlock instead of unlock headings, then icon buttons once unlocked', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByLabelText('Vault password')
    expect(screen.getByRole('img', { name: 'Vault locked' })).toBeInTheDocument()
    expect(screen.queryByText('VAULT LOCKED')).not.toBeInTheDocument()
    expect(screen.queryByText('Unlock your vault')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Settings' })).not.toBeInTheDocument()

    await unlock(user)
    expect(screen.queryByRole('img', { name: 'Vault locked' })).not.toBeInTheDocument()
    const actions = ['Create / import wallet', 'Settings', 'Lock vault']
    const buttons = actions.map((name) => screen.getByRole('button', { name }))
    const header = buttons[0].parentElement!
    expect(Array.from(header.children)).toEqual(buttons)
    expect(screen.queryByText('Welcome home')).not.toBeInTheDocument()
    expect(screen.queryByText('IN THIS VAULT')).not.toBeInTheDocument()
    expect(screen.queryByText('Vault unlocked')).not.toBeInTheDocument()
  })

  it('lists wallets after unlocking and locks again from the header', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    expect(screen.getByRole('button', { name: /Savings/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Trading/ })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Lock vault' }))
    expect(await screen.findByRole('button', { name: 'Unlock vault' })).toBeInTheDocument()
    expect(screen.queryByText('Savings')).not.toBeInTheDocument()
    expect(commandsCalled('lock_vault')).toHaveLength(1)
  })
})

describe('hedera testnet panel', () => {
  beforeEach(() => installBackend())

  it('does not contact the network for wallets without a linked account', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    expect(screen.getByText(/when the vault\s+unlocks, when you open a wallet/)).toBeInTheDocument()
    expect(commandsCalled('find_wallet_account')).toHaveLength(0)
    expect(commandsCalled('get_account_overview')).toHaveLength(0)
  })

  it('shows each balance and the total on HOME, and refreshes when a wallet opens', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [
        wallet('w1', 'Savings', '0.0.1001'),
        wallet('w2', 'Trading', '0.0.1002'),
        wallet('w3', 'Empty'),
      ],
      overview: { account_id: 'x', balance_tinybars: 250_000_000, transactions: [] },
    })
    render(<App />)
    await unlock(user)
    await waitFor(() => expect(screen.getAllByText('2.50 ℏ')).toHaveLength(2))
    expect(screen.getByLabelText('Total balance')).toHaveTextContent('5.00 ℏ')
    expect(commandsCalled('get_account_overview')).toHaveLength(2)

    await openWalletTab(user, 'Savings')
    await waitFor(() => expect(commandsCalled('get_account_overview')).toHaveLength(3))
  })

  it('leaves the total blank when no balance is known', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    expect(screen.getByLabelText('Total balance')).toHaveTextContent('')
  })

  it('links an account ID, rejects a malformed one, and unlinks', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')

    await user.type(screen.getByLabelText('Or link an account ID'), 'nonsense')
    await user.click(screen.getByRole('button', { name: 'Link account' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('account ID must look like')

    await user.clear(screen.getByLabelText('Or link an account ID'))
    await user.type(screen.getByLabelText('Or link an account ID'), '0.0.1001')
    await user.click(screen.getByRole('button', { name: 'Link account' }))
    expect(await screen.findByText('0.0.1001')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Unlink account' }))
    expect(await screen.findByLabelText('Or link an account ID')).toBeInTheDocument()
  })

  it('finds the account from the wallet key, or explains when none exists', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')

    const find = 'Find account from this wallet’s key'
    await user.click(screen.getByRole('button', { name: find }))
    expect(await screen.findByRole('alert')).toHaveTextContent('no Testnet account')

    backend.foundAccount = '0.0.2002'
    await user.click(screen.getByRole('button', { name: find }))
    expect(await screen.findByText('0.0.2002')).toBeInTheDocument()
  })

  it('shows the balance and recent transactions after Refresh', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: '0.0.1001',
        balance_tinybars: 250_000_000,
        transactions: [
          {
            transaction_id: '0.0.1001-1-1',
            kind: 'CRYPTOTRANSFER',
            result: 'SUCCESS',
            consensus_time: '2026-01-01T00:00:00+00:00',
            net_tinybars: -100_100_000,
          },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Refresh balance' }))
    expect(await screen.findByText(/2\.50 ℏ/)).toBeInTheDocument()
    const list = screen.getByRole('list', { name: 'Recent transactions' })
    expect(within(list).getByText('CRYPTOTRANSFER')).toBeInTheDocument()
    expect(within(list).getByText(/-1\.001 ℏ/)).toBeInTheDocument()
  })

  it('says so when the account is not found on Testnet', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')], overview: null })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Refresh balance' }))
    expect(await screen.findByRole('status')).toHaveTextContent('not found on Hedera Testnet')
  })
})

describe('tabs', () => {
  beforeEach(() => installBackend())

  it('opens wallet, settings and create tabs and keeps HOME persistent', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)

    await openWalletTab(user, 'Savings')
    expect(screen.getByRole('tab', { name: 'Savings' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: 'HOME' })).toHaveAttribute('aria-selected', 'false')

    await user.click(screen.getByRole('tab', { name: 'HOME' }))
    await user.click(screen.getByRole('button', { name: /Settings/ }))
    expect(await screen.findByRole('tab', { name: 'Settings' })).toHaveAttribute('aria-selected', 'true')

    await user.click(screen.getByRole('tab', { name: 'HOME' }))
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    expect(await screen.findByRole('tab', { name: 'Create / import' })).toBeInTheDocument()
    expect(screen.getAllByRole('tab')).toHaveLength(4)
  })

  it('closes a tab and falls back to HOME, which cannot be closed', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Trading')

    await user.click(screen.getByRole('button', { name: 'Close Trading tab' }))
    expect(screen.queryByRole('tab', { name: 'Trading' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'HOME' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('button', { name: /Close HOME/ })).not.toBeInTheDocument()
  })

  it('moves between tabs with the arrow keys', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')

    screen.getByRole('tab', { name: 'Savings' }).focus()
    await user.keyboard('{ArrowLeft}')
    expect(screen.getByRole('tab', { name: 'HOME' })).toHaveFocus()
  })
})

describe('reveal flow', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  async function openReveal(user: ReturnType<typeof userEvent.setup>) {
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Reveal recovery phrase' }))
    await user.click(await screen.findByRole('button', { name: 'Reveal phrase' }))
    await screen.findByText('alpha')
  }

  it('shows the phrase, blocks copying, and requires acknowledgment to confirm', async () => {
    const user = userEvent.setup()
    await openReveal(user)
    expect(commandsCalled('reveal_wallet_mnemonic')[0].args).toEqual({ walletId: 'w1' })
    expect(screen.getByText('lima')).toBeInTheDocument()

    const grid = document.querySelector('.mnemonic-grid')!
    const copy = new Event('copy', { bubbles: true, cancelable: true })
    expect(grid.dispatchEvent(copy)).toBe(false)

    const confirm = screen.getByRole('button', { name: 'Confirm and hide phrase' })
    expect(confirm).toBeDisabled()
    await user.click(screen.getByRole('checkbox'))
    expect(confirm).toBeEnabled()
    await user.click(confirm)
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
  })

  it('clears the phrase when the window loses focus', async () => {
    const user = userEvent.setup()
    await openReveal(user)
    act(() => {
      window.dispatchEvent(new Event('blur'))
    })
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('clears the phrase on Escape', async () => {
    const user = userEvent.setup()
    await openReveal(user)
    await user.keyboard('{Escape}')
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
  })

  it('shows backend errors without displaying a phrase', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    invokeMock.mockRejectedValueOnce(new Error('vault is locked'))
    await user.click(screen.getByRole('button', { name: 'Reveal recovery phrase' }))
    await user.click(await screen.findByRole('button', { name: 'Reveal phrase' }))
    expect(await screen.findByText('vault is locked')).toBeInTheDocument()
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
  })
})

describe('delete flow', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  it('does nothing when deletion is cancelled', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Delete wallet' }))
    const dialog = await screen.findByRole('alertdialog')
    expect(within(dialog).getByText('Delete Savings?')).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
    expect(commandsCalled('delete_wallet')).toHaveLength(0)
    expect(screen.getByRole('tab', { name: 'Savings' })).toBeInTheDocument()
  })

  it('deletes the wallet, closes its tab and returns to HOME', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Delete wallet' }))
    const dialog = await screen.findByRole('alertdialog')
    await user.click(within(dialog).getByRole('button', { name: 'Delete wallet' }))

    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(commandsCalled('delete_wallet')[0].args).toEqual({ walletId: 'w1' })
    expect(screen.queryByRole('tab', { name: 'Savings' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'HOME' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('button', { name: /Savings/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Trading/ })).toBeInTheDocument()
  })

  it('keeps the wallet and shows the error when deletion fails', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Delete wallet' }))
    const dialog = await screen.findByRole('alertdialog')
    invokeMock.mockRejectedValueOnce(new Error('could not save vault'))
    await user.click(within(dialog).getByRole('button', { name: 'Delete wallet' }))
    expect(await within(dialog).findByText('could not save vault')).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Savings' })).toBeInTheDocument()
  })
})

describe('private key import', () => {
  beforeEach(() => installBackend())

  async function openKeyImport(user: ReturnType<typeof userEvent.setup>) {
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    await user.click(await screen.findByRole('button', { name: 'Import' }))
    await user.click(screen.getByRole('button', { name: 'Private key' }))
  }

  it('detects the key type, imports the key and links the single account found', async () => {
    backend.detection = { keyType: 'ecdsa', accounts: ['0.0.555'] }
    const user = userEvent.setup()
    await openKeyImport(user)
    await user.type(screen.getByLabelText('Private key (hex or DER)'), '11'.repeat(32))
    await user.type(screen.getByLabelText('Wallet name'), 'Funded')
    await user.click(screen.getByRole('button', { name: /Import encrypted wallet/ }))

    await screen.findByRole('tab', { name: 'HOME' })
    expect(commandsCalled('import_private_key')[0].args).toEqual({
      name: 'Funded',
      privateKey: '11'.repeat(32),
      keyType: 'ecdsa',
    })
    expect(commandsCalled('link_wallet_account')[0].args).toEqual({
      walletId: 'k3',
      accountId: '0.0.555',
    })
  })

  it('asks the user to choose when the type cannot be detected, then imports', async () => {
    const user = userEvent.setup()
    await openKeyImport(user)
    await user.type(screen.getByLabelText('Private key (hex or DER)'), '22'.repeat(32))
    await user.type(screen.getByLabelText('Wallet name'), 'Manual')
    await user.click(screen.getByRole('button', { name: /Import encrypted wallet/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Choose Ed25519 or ECDSA')
    expect(commandsCalled('import_private_key')).toHaveLength(0)

    await user.selectOptions(screen.getByLabelText('Key type'), 'ed25519')
    await user.click(screen.getByRole('button', { name: /Import encrypted wallet/ }))
    await screen.findByRole('tab', { name: 'HOME' })
    expect(commandsCalled('import_private_key')[0].args?.keyType).toBe('ed25519')
    expect(commandsCalled('detect_private_key_type')).toHaveLength(1)
  })
  it('clears the import form and its error when the vault locks', async () => {
    const user = userEvent.setup()
    await openKeyImport(user)
    await user.type(screen.getByLabelText('Private key (hex or DER)'), '33'.repeat(32))
    await user.type(screen.getByLabelText('Wallet name'), 'Leftover')
    await user.click(screen.getByRole('button', { name: /Import encrypted wallet/ }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Choose Ed25519 or ECDSA')

    await user.click(screen.getByRole('button', { name: 'Lock vault' }))
    await unlock(user)
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Wallet name')).toHaveValue('')
    await user.click(screen.getByRole('button', { name: 'Import' }))
    await user.click(screen.getByRole('button', { name: 'Private key' }))
    expect(screen.getByLabelText('Private key (hex or DER)')).toHaveValue('')
  })
})

describe('rename flow', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  it('renames a wallet and updates its tab and the HOME list', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Rename wallet' }))
    const input = screen.getByLabelText('New wallet name')
    expect(input).toHaveValue('Savings')
    await user.clear(input)
    await user.type(input, 'Rainy day')
    await user.click(screen.getByRole('button', { name: 'Save name' }))

    expect(await screen.findByRole('heading', { level: 1, name: 'Rainy day' })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Rainy day' })).toBeInTheDocument()
    expect(commandsCalled('rename_wallet')[0].args).toEqual({ walletId: 'w1', name: 'Rainy day' })
    expect(screen.queryByLabelText('New wallet name')).not.toBeInTheDocument()
  })

  it('shows the error for a duplicate name and keeps the old name', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Rename wallet' }))
    const input = screen.getByLabelText('New wallet name')
    await user.clear(input)
    await user.type(input, 'trading')
    await user.click(screen.getByRole('button', { name: 'Save name' }))

    expect(await screen.findByText("a wallet named 'trading' already exists")).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Savings' })).toBeInTheDocument()
  })

  it('limits the name length to 40 characters and can be cancelled', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Rename wallet' }))
    expect(screen.getByLabelText('New wallet name')).toHaveAttribute('maxlength', '40')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByLabelText('New wallet name')).not.toBeInTheDocument()
    expect(commandsCalled('rename_wallet')).toHaveLength(0)
  })
})

describe('backup and restore', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  async function openSettings(user: ReturnType<typeof userEvent.setup>) {
    render(<App />)
    await user.click(await screen.findByRole('button', { name: /Settings/ }))
    await screen.findByRole('heading', { level: 1, name: 'Settings' })
  }

  it('reports a saved or cancelled backup', async () => {
    const user = userEvent.setup()
    await openSettings(user)
    await user.click(screen.getByRole('button', { name: 'Choose location and save backup' }))
    expect(await screen.findByText(/Backup saved/)).toBeInTheDocument()

    backend.exportSaved = false
    await user.click(screen.getByRole('button', { name: 'Choose location and save backup' }))
    expect(await screen.findByText('Backup cancelled.')).toBeInTheDocument()
  })

  it('only backs up and erases after confirmation, then shows the create-vault gate', async () => {
    const user = userEvent.setup()
    await openSettings(user)
    const eraseButton = screen.getByRole('button', { name: 'Choose location, save backup and erase' })
    expect(eraseButton).toBeDisabled()

    backend.exportSaved = false
    await user.click(screen.getByRole('checkbox', { name: /removes every wallet from this device/ }))
    await user.click(eraseButton)
    expect(await screen.findByText('Cancelled. Nothing was erased.')).toBeInTheDocument()
    expect(backend.wallets.length).toBeGreaterThan(0)

    backend.exportSaved = true
    await user.click(eraseButton)
    expect(await screen.findByRole('button', { name: 'Create vault' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Restore backup' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { level: 1, name: 'Settings' })).not.toBeInTheDocument()
    expect(backend.status).toBe('uninitialized')
  })

  it('keeps the erase confirmation ticked when the window loses focus for the file dialog', async () => {
    const user = userEvent.setup()
    await openSettings(user)
    const box = screen.getByRole('checkbox', { name: /removes every wallet from this device/ })
    await user.click(box)
    act(() => {
      window.dispatchEvent(new Event('blur'))
    })
    expect(box).toBeChecked()
  })

  it('requires confirmation before restoring and reports a wrong password', async () => {
    const user = userEvent.setup()
    await openSettings(user)
    const restore = screen.getByRole('button', { name: 'Choose backup file and restore' })
    expect(restore).toBeDisabled()

    await user.type(screen.getByLabelText('Password of the backup file'), 'nope')
    await user.click(screen.getByRole('checkbox', { name: /restoring replaces all wallets/ }))
    await user.click(restore)
    expect(await screen.findByText('incorrect backup password or damaged backup')).toBeInTheDocument()
    expect(screen.getByLabelText('Password of the backup file')).toHaveValue('')
  })

  it('replaces the wallets after a successful restore', async () => {
    const user = userEvent.setup()
    await openSettings(user)
    await user.type(screen.getByLabelText('Password of the backup file'), 'backup password')
    await user.click(screen.getByRole('checkbox', { name: /restoring replaces all wallets/ }))
    await user.click(screen.getByRole('button', { name: 'Choose backup file and restore' }))
    expect(await screen.findByText('Backup restored with 1 wallet(s).')).toBeInTheDocument()
    expect(commandsCalled('restore_vault_backup')[0].args).toEqual({ backupPassword: 'backup password' })

    await user.click(screen.getByRole('tab', { name: 'HOME' }))
    expect(screen.getByRole('button', { name: /Restored/ })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Savings/ })).not.toBeInTheDocument()
  })

  it('leaves everything unchanged when the file dialog is cancelled', async () => {
    backend.restoreCancelled = true
    const user = userEvent.setup()
    await openSettings(user)
    await user.type(screen.getByLabelText('Password of the backup file'), 'backup password')
    await user.click(screen.getByRole('checkbox', { name: /restoring replaces all wallets/ }))
    await user.click(screen.getByRole('button', { name: 'Choose backup file and restore' }))
    expect(await screen.findByText('Restore cancelled.')).toBeInTheDocument()
    expect(backend.wallets.map((item) => item.name)).toEqual(['Savings', 'Trading'])
  })

  it('starts with two plain buttons and shows only the chosen section', async () => {
    installBackend({ status: 'uninitialized' })
    const user = userEvent.setup()
    render(<App />)
    const createButton = await screen.findByRole('button', { name: 'Create vault' })
    expect(screen.getByRole('button', { name: 'Restore backup' })).toBeInTheDocument()
    expect(screen.queryByLabelText('Vault password')).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Password of the backup file')).not.toBeInTheDocument()

    await user.click(createButton)
    expect(screen.getByLabelText('Vault password')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Generate six-word passphrase' }))
    expect(screen.getByLabelText('Vault password')).toHaveAttribute('type', 'password')
    await user.click(screen.getAllByRole('button', { name: 'Show vault password' })[0])
    expect(screen.getByLabelText('Vault password')).toHaveAttribute('type', 'text')
    expect(screen.getByLabelText('Confirm vault password')).toHaveAttribute('type', 'text')
    await user.click(screen.getByRole('button', { name: 'Generate six-word passphrase' }))
    expect(screen.getByLabelText('Vault password')).toHaveAttribute('type', 'text')
    expect(screen.queryByRole('button', { name: 'Show' })).not.toBeInTheDocument()
    expect(screen.getByText(/eff\.org long word list/)).toBeInTheDocument()
    expect(screen.queryByLabelText('Password of the backup file')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Back' }))

    await user.click(screen.getByRole('button', { name: 'Restore backup' }))
    expect(screen.getByLabelText('Password of the backup file')).toBeInTheDocument()
    expect(screen.queryByLabelText('Vault password')).not.toBeInTheDocument()
  })

  it('offers restore on the create-vault screen without a confirmation checkbox', async () => {
    installBackend({ status: 'uninitialized' })
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Restore backup' }))
    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument()
    await user.type(screen.getByLabelText('Password of the backup file'), 'backup password')
    await user.click(screen.getByRole('button', { name: 'Choose backup file and restore' }))
    expect(await screen.findByRole('tab', { name: 'HOME' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Restored/ })).toBeInTheDocument()
  })
})

describe('change vault password', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  it('rejects mismatched new passwords without calling the backend', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('button', { name: /Settings/ }))
    await user.type(await screen.findByLabelText('Current vault password'), 'correct password')
    await user.type(screen.getByLabelText('New vault password'), 'first one')
    await user.type(screen.getByLabelText('Confirm new vault password'), 'second one')
    await user.click(screen.getByRole('button', { name: 'Change password' }))
    expect(await screen.findByText('New vault passwords do not match.')).toBeInTheDocument()
    expect(commandsCalled('change_vault_password')).toHaveLength(0)
  })

  it('clears the password fields when the window loses focus', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('button', { name: /Settings/ }))
    await user.type(await screen.findByLabelText('Current vault password'), 'correct password')
    act(() => {
      window.dispatchEvent(new Event('blur'))
    })
    expect(screen.getByLabelText('Current vault password')).toHaveValue('')
  })
})

describe('auto-lock', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    installBackend({ status: 'unlocked', autoLockSeconds: 60 })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  async function advance(ms: number) {
    for (let elapsed = 0; elapsed < ms; elapsed += 250) {
      await act(async () => {
        await vi.advanceTimersByTimeAsync(250)
      })
    }
  }

  async function renderUnlocked() {
    render(<App />)
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0)
    })
    expect(screen.getByRole('tab', { name: 'HOME' })).toBeInTheDocument()
  }

  it('locks after the inactivity period and clears wallet data', async () => {
    await renderUnlocked()
    await advance(59_000)
    expect(commandsCalled('lock_vault')).toHaveLength(0)

    await advance(1_500)
    expect(commandsCalled('lock_vault')).toHaveLength(1)
    expect(screen.getByRole('button', { name: 'Unlock vault' })).toBeInTheDocument()
    expect(screen.queryByText('Savings')).not.toBeInTheDocument()
  })

  it('resets the timer on user activity and tells the backend', async () => {
    await renderUnlocked()
    await advance(50_000)
    act(() => {
      fireEvent.keyDown(window, { key: 'a' })
    })
    expect(commandsCalled('record_activity')).toHaveLength(1)

    await advance(50_000)
    expect(commandsCalled('lock_vault')).toHaveLength(0)

    await advance(11_000)
    expect(commandsCalled('lock_vault')).toHaveLength(1)
  })

  it('closes an open recovery phrase when the vault auto-locks', async () => {
    await renderUnlocked()
    fireEvent.click(screen.getByRole('button', { name: /Savings/ }))
    fireEvent.click(screen.getByRole('button', { name: 'Reveal recovery phrase' }))
    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Reveal phrase' }))
      await vi.advanceTimersByTimeAsync(0)
    })
    expect(screen.getByText('alpha')).toBeInTheDocument()

    await advance(61_000)
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Unlock vault' })).toBeInTheDocument()
  })
})

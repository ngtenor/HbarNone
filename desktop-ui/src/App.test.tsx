import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import App from './App'

type Account = {
  network: 'testnet' | 'mainnet'
  id: string
  display_id: string
  name: string
  named: boolean
}
type Wallet = {
  id: string
  name: string
  created_at: string
  network?: string
  accounts: Account[]
  secret_kind?: string
  key_type?: string
  custody?: 'hot' | 'cold'
}

type Backend = {
  status: 'uninitialized' | 'locked' | 'unlocked'
  wallets: Wallet[]
  autoLockSeconds: number
  backupWallets: Wallet[]
  exportSaved: boolean
  restoreCancelled: boolean
  foundAccounts: string[]
  overview: unknown
  overviewErrors: Record<string, string>
  sendError: string | null
  transactionResult?: string
  hidden: string[]
  supplyTokens?: string[]
  nftSupplyTokens?: string[]
  detection: { keyType: string | null; accounts: string[] }
  allowExternalSends: boolean
  allowExternalDeletes: boolean
  allowNftCustomFees: boolean
  tokenInfo?: Record<string, unknown>
  outside: { id: string; name: string }[]
  deletedOnNetwork?: string[]
  sharedAccounts?: ReturnType<typeof sharedAccountView>[]
  calls: { cmd: string; args?: Record<string, unknown> }[]
}

const PHRASE = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima'

const invokeMock = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }))

// The camera cannot run in tests. When a test sets `scanner.reply`, the
// scanner renders a button that delivers that reply; otherwise the real
// component renders (to test its no-camera message).
const scanner = vi.hoisted(() => ({ reply: null as string | null }))
vi.mock('./QrScanner', async (importOriginal) => {
  const real = await importOriginal<typeof import('./QrScanner')>()
  return {
    QrScanner: (props: Parameters<typeof real.QrScanner>[0]) =>
      scanner.reply === null ? (
        <real.QrScanner {...props} />
      ) : (
        <button type="button" onClick={() => props.onScan(scanner.reply!)}>
          Deliver scanned reply
        </button>
      ),
  }
})

function testnetAccount(id: string): Account {
  return { network: 'testnet', id, display_id: `${id}-abcde`, name: 'NONAME', named: false }
}

function wallet(id: string, name: string, accounts: string | string[] = []): Wallet {
  return {
    id,
    name,
    created_at: '2026-01-01T00:00:00Z',
    network: 'testnet',
    accounts: [accounts].flat().map(testnetAccount),
    secret_kind: 'mnemonic',
  }
}

const SHARED_KEYS = ['aa'.repeat(32), 'bb'.repeat(32), 'cc'.repeat(32)]

function sharedAccountView(id: string, name?: unknown) {
  return {
    id,
    displayId: `${id}-abcde`,
    name: name ? String(name) : 'NONAME',
    named: Boolean(name),
    required: 2,
    total: 3,
    keys: SHARED_KEYS,
    description: `2 of 3:\n${SHARED_KEYS.map((key) => `  Ed25519 ${key}`).join('\n')}`,
  }
}

let backend: Backend

function vaultState() {
  return {
    status: backend.status,
    wallets: backend.status === 'unlocked'
      ? backend.wallets.map((item) => ({
        ...item,
        accounts: item.accounts.map((account) => ({ ...account })),
      }))
      : [],
    legacyWallets: [],
    autoLockSeconds: backend.autoLockSeconds,
  }
}

function addressBookView() {
  const rows = [
    ...backend.wallets.flatMap((owner) => owner.accounts.map((item) => ({
      network: item.network,
      id: item.id,
      display_id: item.display_id,
      name: item.name,
      wallet_id: owner.id,
      wallet_name: owner.name,
    }))),
    ...backend.outside.map((item) => ({
      network: 'testnet',
      id: item.id,
      display_id: `${item.id}-abcde`,
      name: item.name,
      wallet_id: null,
      wallet_name: null,
    })),
  ]
  return {
    rows,
    allowExternalSends: backend.allowExternalSends,
    allowExternalDeletes: backend.allowExternalDeletes,
  }
}

async function typeRecipient(user: ReturnType<typeof userEvent.setup>, value: string) {
  await user.click(await screen.findByLabelText('Check to send to a different account'))
  await user.type(await screen.findByLabelText('Recipient account ID'), value)
}

function installBackend(overrides: Partial<Backend> = {}) {
  // Each test starts in the Testnet view.
  localStorage.removeItem('networkView')
  scanner.reply = null
  backend = {
    status: 'locked',
    wallets: [wallet('w1', 'Savings'), wallet('w2', 'Trading')],
    autoLockSeconds: 300,
    backupWallets: [wallet('b1', 'Restored')],
    exportSaved: true,
    restoreCancelled: false,
    foundAccounts: [],
    overview: null,
    overviewErrors: {},
    sendError: null,
    hidden: [],
    detection: { keyType: null, accounts: [] },
    allowExternalSends: true,
    allowExternalDeletes: false,
    allowNftCustomFees: false,
    outside: [],
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
      case 'start_over_vault':
        backend.status = 'uninitialized'
        backend.wallets = []
        return { status: 'uninitialized', wallets: [], legacyWallets: [], autoLockSeconds: backend.autoLockSeconds }
      case 'prune_address_book':
        backend.outside = backend.outside.filter(
          (item) => !(backend.deletedOnNetwork ?? []).includes(item.id),
        )
        for (const owner of backend.wallets) {
          owner.accounts = owner.accounts.filter(
            (item) => !(backend.deletedOnNetwork ?? []).includes(item.id),
          )
        }
        return addressBookView()
      case 'lock_vault':
        backend.status = 'locked'
        return undefined
      case 'record_activity':
      case 'set_auto_lock_seconds':
        return args?.seconds ?? undefined
      case 'reveal_wallet_mnemonic':
        return PHRASE
      case 'reveal_wallet_private_key':
        return 'ab'.repeat(32)
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
        const accountId = String(args?.accountId).trim()
        if (!/^0\.0\.[1-9]\d*$/.test(accountId)) {
          throw new Error('account ID must look like 0.0.12345')
        }
        if (!target.accounts.some((item) => item.id === accountId)) {
          target.accounts.push(testnetAccount(accountId))
        }
        return { ...target }
      }
      case 'find_wallet_accounts': {
        if (backend.foundAccounts.length === 0) {
          throw new Error('no Testnet account uses this wallet\'s key yet')
        }
        const target = backend.wallets.find((item) => item.id === args?.walletId)!
        for (const id of backend.foundAccounts) {
          if (!target.accounts.some((item) => item.id === id)) {
            target.accounts.push(testnetAccount(id))
          }
        }
        return { ...target }
      }
      case 'get_transaction_result':
        return backend.transactionResult ?? 'SUCCESS'
      case 'get_account_balance':
        return backend.overview
          ? (backend.overview as { balance_tinybars: number }).balance_tinybars
          : null
      case 'get_token_info':
        return {
          token_id: '0.0.8',
          name: 'Alpha',
          symbol: 'ALP',
          decimals: 2,
          token_type: 'FUNGIBLE_COMMON',
          total_supply: '100000',
          treasury: '0.0.5',
          deleted: false,
          has_admin_key: true,
          admin_key: 'ab'.repeat(32),
          memo: '',
          custom_fees: { fixed_fees: [], royalty_fees: [], has_other_fees: false },
          ...(backend.tokenInfo ?? {}),
        }
      case 'get_hidden_tokens':
        return [...backend.hidden]
      case 'get_supply_tokens':
        return backend.supplyTokens ?? args?.tokenIds ?? []
      case 'get_nft_supply_tokens':
        return backend.nftSupplyTokens ?? args?.tokenIds ?? []
      case 'create_nft_collection':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000014' }
      case 'mint_nft':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000015' }
      case 'set_token_hidden':
        backend.hidden = args?.hidden
          ? [...backend.hidden.filter((id) => id !== args.tokenId), String(args.tokenId)]
          : backend.hidden.filter((id) => id !== args?.tokenId)
        return [...backend.hidden]
      case 'associate_token':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000011' }
      case 'dissociate_token':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000012' }
      case 'create_token':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000009' }
      case 'delete_token':
      case 'change_token_supply':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000010' }
      case 'get_account_overview':
        if (backend.overviewErrors[String(args?.accountId)]) {
          throw new Error(backend.overviewErrors[String(args?.accountId)])
        }
        return backend.overview
      case 'send_hbar':
      case 'send_token':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000005' }
      case 'send_nft':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000013' }
      case 'get_allow_nft_custom_fees':
        return backend.allowNftCustomFees
      case 'set_allow_nft_custom_fees':
        backend.allowNftCustomFees = Boolean(args?.allowed)
        return backend.allowNftCustomFees
      case 'prepare_unsigned_transfer':
        return {
          payload: 'HVTX1-REQ:00',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'transfer',
          sender: '0.0.1001',
          receiver: '0.0.2002',
          deleteAccount: '',
          newKey: '',
          tinybars: 100_000_000,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_delete':
        return {
          payload: 'HVTX1-REQ:01',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'delete',
          sender: '0.0.1002-abcde',
          receiver: '0.0.1002-abcde',
          deleteAccount: '0.0.1001-abcde',
          newKey: '',
          tinybars: 0,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_create':
        return {
          payload: 'HVTX1-REQ:02',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'create',
          sender: '0.0.1001-abcde',
          receiver: '',
          deleteAccount: '',
          newKey: 'ab'.repeat(32),
          tinybars: 200_000_000,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_token_transfer':
        return {
          payload: 'HVTX1-REQ:03',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'token-transfer',
          sender: '0.0.1001-abcde',
          receiver: '0.0.2002-abcde',
          deleteAccount: '',
          newKey: '',
          tinybars: 0,
          tokenId: '0.0.8',
          tokenUnits: '150',
          tokenDecimals: 2,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_nft_transfer':
        return {
          payload: 'HVTX1-REQ:06',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'nft-transfer',
          sender: '0.0.1001-abcde',
          receiver: '0.0.2002-abcde',
          deleteAccount: '',
          newKey: '',
          tinybars: 0,
          tokenId: (args as { tokenId: string }).tokenId,
          tokenUnits: '',
          tokenDecimals: 0,
          serialNumber: (args as { serialNumber: string }).serialNumber,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_token_association':
        return {
          payload: 'HVTX1-REQ:04',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: (args as { associate: boolean }).associate ? 'token-associate' : 'token-dissociate',
          sender: '0.0.1001-abcde',
          receiver: '',
          deleteAccount: '',
          newKey: '',
          tinybars: 0,
          tokenId: (args as { tokenId: string }).tokenId,
          tokenUnits: '',
          tokenDecimals: 0,
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'prepare_unsigned_move':
        return {
          payload: 'HVTX1-REQ:05',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'key-update',
          sender: '0.0.1001-abcde',
          receiver: '',
          deleteAccount: '',
          newKey: 'cd'.repeat(32),
          keyStructure: [],
          currentKey: 'ab'.repeat(32),
          tinybars: 0,
          tokenId: '',
          tokenUnits: '',
          tokenDecimals: 0,
          serialNumber: '',
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'verify_move_signature': {
        const response = String(args?.response)
        if (response.includes('CURRENT')) return 'current'
        if (response.includes('NEW')) return 'new'
        throw new Error('The signature was made with a key that is not part of this move.')
      }
      case 'submit_signed_move':
        return { transactionId: '0.0.1001-1700000000-000000009' }
      case 'submit_signed_request':
        return { transactionId: '0.0.1001-1700000000-000000008' }
      case 'list_shared_accounts':
        return [...(backend.sharedAccounts ?? [])]
      case 'get_account_key_structure':
        return {
          description: `2 of 3:\n${SHARED_KEYS.map((key) => `  Ed25519 ${key}`).join('\n')}`,
          required: 2,
          total: 3,
          keys: SHARED_KEYS,
        }
      case 'add_shared_account': {
        const added = sharedAccountView(String(args?.accountId).split('-')[0], args?.name)
        backend.sharedAccounts = [...(backend.sharedAccounts ?? []), added]
        return added
      }
      case 'remove_shared_account':
        backend.sharedAccounts = (backend.sharedAccounts ?? []).filter(
          (item) => item.id !== args?.accountId,
        )
        return undefined
      case 'get_shared_overview':
        return backend.overview ?? {
          account_id: args?.accountId,
          balance_tinybars: 500_000_000,
          transactions: [],
        }
      case 'prepare_shared_transfer':
        return {
          payload: 'HVTX1-REQ:10',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'transfer',
          sender: String(args?.accountId),
          receiver: String(args?.recipient),
          deleteAccount: '',
          newKey: '',
          keyStructure: [],
          tinybars: 150_000_000,
          tokenId: '',
          tokenUnits: '',
          tokenDecimals: 0,
          serialNumber: '',
          memo: String(args?.memo ?? ''),
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'check_shared_signatures': {
        const responses = (args?.responses as string[]) ?? []
        if (responses.some((item) => !item.startsWith('HVTX1-SIG:') || item.endsWith('BAD'))) {
          throw new Error('This is not a valid signature from a signer.')
        }
        const unique = [...new Set(responses)]
        return {
          signedKeys: unique,
          required: 2,
          total: 3,
          matched: Math.min(unique.length, 3),
          satisfied: unique.length >= 2,
        }
      }
      case 'submit_shared_transfer':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.5005-1700000000-000000021' }
      case 'create_shared_account':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000022' }
      case 'finish_shared_create': {
        const added = sharedAccountView('0.0.9001', args?.name)
        backend.sharedAccounts = [...(backend.sharedAccounts ?? []), added]
        return added
      }
      case 'prepare_shared_key_change':
        return {
          payload: 'HVTX1-REQ:11',
          qrSize: 2,
          qrModules: [true, false, false, true],
          action: 'key-update',
          sender: String(args?.accountId),
          receiver: '',
          deleteAccount: '',
          newKey: '',
          keyStructure: ['2 of 3:', ...SHARED_KEYS.map((key) => `  Ed25519 ${key}`)],
          tinybars: 0,
          tokenId: '',
          tokenUnits: '',
          tokenDecimals: 0,
          serialNumber: '',
          memo: '',
          expiresAtSeconds: Math.floor(Date.now() / 1000) + 170,
        }
      case 'submit_shared_key_change':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000023' }
      case 'finish_shared_convert': {
        const owner = backend.wallets.find((item) => item.id === args?.walletId)!
        owner.accounts = owner.accounts.filter((item) => item.id !== args?.accountId)
        const added = sharedAccountView(String(args?.accountId), args?.name)
        backend.sharedAccounts = [...(backend.sharedAccounts ?? []), added]
        return added
      }
      case 'create_account':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000006' }
      case 'move_account':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000007' }
      case 'finish_account_move': {
        const from = backend.wallets.find((item) => item.id === args?.walletId)!
        const to = backend.wallets.find((item) => item.id === args?.targetWalletId)!
        from.accounts = from.accounts.filter((item) => item.id !== args?.accountId)
        to.accounts.push(testnetAccount(String(args?.accountId)))
        return { ...to }
      }
      case 'delete_account':
        if (backend.sendError) throw new Error(backend.sendError)
        return { transactionId: '0.0.1001-1700000000-000000008' }
      case 'finish_account_delete': {
        const owner = backend.wallets.find((item) => item.id === args?.walletId)!
        owner.accounts = owner.accounts.filter((item) => item.id !== args?.accountId)
        return { ...owner }
      }
      case 'check_recipient':
        if (!/^0\.0\.[1-9]\d*-[a-z]{5}$/i.test(String(args?.recipient).trim())) {
          throw new Error('Enter the account number with its checksum, like 0.0.12345-abcde.')
        }
        return String(args?.recipient).split('-')[0]
      case 'rename_account': {
        const name = String(args?.name).trim()
        if (!name) throw new Error('an account name cannot be empty')
        const owner = backend.wallets.find((item) => item.id === args?.walletId)!
        const target = owner.accounts.find((item) => item.id === args?.accountId)!
        target.name = name
        target.named = true
        return { ...owner }
      }
      case 'get_address_book':
        return addressBookView()
      case 'set_allow_external_sends':
        backend.allowExternalSends = Boolean(args?.allowed)
        return addressBookView()
      case 'set_allow_external_deletes':
        backend.allowExternalDeletes = Boolean(args?.allowed)
        return addressBookView()
      case 'add_address_book_entry': {
        const id = String(args?.address).split('-')[0]
        if (!String(args?.name).trim()) throw new Error('an address book name is required')
        if (!/^0\.0\.[1-9]\d*-[a-z]{5}$/i.test(String(args?.address).trim())) {
          throw new Error('Enter the account number with its checksum, like 0.0.12345-abcde.')
        }
        backend.outside.push({ id, name: String(args?.name).trim() })
        return addressBookView()
      }
      case 'rename_address_book_account': {
        const name = String(args?.name).trim()
        if (!name) throw new Error('an account name cannot be empty')
        for (const owner of backend.wallets) {
          for (const item of owner.accounts) {
            if (item.id === args?.accountId) {
              item.name = name
              item.named = true
            }
          }
        }
        for (const item of backend.outside) {
          if (item.id === args?.accountId) item.name = name
        }
        return addressBookView()
      }
      case 'remove_address_book_entry':
        backend.outside = backend.outside.filter((item) => item.id !== args?.accountId)
        return addressBookView()
      case 'get_address_balance':
        if (!args?.network) {
          throw new Error('command get_address_balance missing required key network')
        }
        return 250_000_000
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
      case 'import_watch_only_wallet': {
        if (backend.wallets.some((item) => item.name === args?.name)) {
          throw new Error('name taken')
        }
        const created = {
          ...wallet(`w${backend.wallets.length + 1}`, String(args?.name)),
          network: args?.network ? String(args.network) : 'testnet',
          secret_kind: 'watchonly',
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
  await screen.findByRole('tab', { name: 'Wallets' })
}

async function openWalletTab(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole('button', { name: new RegExp(name) }))
  expect(await screen.findByRole('tab', { name })).toHaveAttribute('aria-selected', 'true')
}

async function chooseMenu(user: ReturnType<typeof userEvent.setup>, menu: string, item: string) {
  await user.click(screen.getByRole('button', { name: menu }))
  await user.click(await screen.findByRole('menuitem', { name: item }))
}

async function openAccount(
  user: ReturnType<typeof userEvent.setup>,
  name: string,
  section?: string,
  accountId = '0\\.0\\.1001',
) {
  await openWalletTab(user, name)
  await user.click(await screen.findByRole('button', { name: new RegExp(accountId) }))
  await screen.findByRole('button', { name: 'Back to wallet' })
  if (section === 'Send') await chooseMenu(user, 'Send', 'HBAR')
  else if (section) await chooseMenu(user, 'Account management', section)
}

describe('home account links', () => {
  it('returns to the wallet from the account using the Back icon', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    const back = screen.getByRole('button', { name: 'Back to wallet' })
    expect(back).toHaveTextContent('')
    expect(back.querySelector('svg')).toBeInTheDocument()
    await user.click(back)
    expect(screen.getByRole('button', { name: 'Wallet management' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Back to wallet' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Savings' })).toHaveAttribute('aria-selected', 'true')
  })

  it('opens the account page when an account is clicked on HOME', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])] })
    render(<App />)
    await unlock(user)
    await user.click(await screen.findByRole('button', { name: /0\.0\.1002/ }))
    expect(await screen.findByRole('button', { name: 'Back to wallet' })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Savings' })).toHaveAttribute('aria-selected', 'true')
    await user.click(screen.getByRole('tab', { name: 'Wallets' }))
    await user.click(screen.getByRole('button', { name: /Savings\s*Added/ }))
    expect(await screen.findByRole('button', { name: 'Wallet management' })).toBeInTheDocument()
    expect(screen.queryByTitle('Back to wallet')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Wallet management' })).toBeInTheDocument()
  })
})

describe('vault gates', () => {
  beforeEach(() => installBackend())

  it('shows the unlock form for a locked vault and reports a wrong password', async () => {
    const user = userEvent.setup()
    render(<App />)
    await user.type(await screen.findByLabelText('Vault password'), 'wrong')
    await user.click(screen.getByRole('button', { name: 'Unlock vault' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('incorrect vault password')
    expect(screen.queryByRole('tab', { name: 'Wallets' })).not.toBeInTheDocument()
    // Submitting the now-empty field clears the message and shows no browser prompt.
    await user.click(screen.getByRole('button', { name: 'Unlock vault' }))
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
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
    const actions = ['Address book', 'Create / import wallet', 'Settings', 'Help', 'Lock vault']
    const buttons = actions.map((name) => screen.getByRole('button', { name }))
    const header = buttons[0].parentElement!
    const networkSwitch = screen.getByRole('group', { name: 'Network' })
    expect(Array.from(header.children)).toEqual([networkSwitch, ...buttons])
    expect(screen.queryByText('Welcome home')).not.toBeInTheDocument()
    expect(screen.queryByText('IN THIS VAULT')).not.toBeInTheDocument()
    expect(screen.queryByText('Vault unlocked')).not.toBeInTheDocument()
  })

  it('opens the Help tab from the toolbar and shows the compiled help text', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: 'Help' }))
    expect(await screen.findByRole('tab', { name: 'Help' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('heading', { name: 'Example title' })).toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Wallets' }))
    await user.click(screen.getByRole('tab', { name: 'Help' }))
    expect(screen.getByRole('heading', { name: 'Example title' })).toBeInTheDocument()
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
    expect(screen.queryByRole('heading', { name: 'Create account' })).not.toBeInTheDocument()
    expect(commandsCalled('find_wallet_accounts')).toHaveLength(0)
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
    expect(commandsCalled('get_account_balance')).toHaveLength(2)

    await openWalletTab(user, 'Savings')
    expect(commandsCalled('get_account_overview')).toHaveLength(0)
    await user.click(await screen.findByRole('button', { name: /0\.0\.1001/ }))
    await waitFor(() => expect(commandsCalled('get_account_overview')).toHaveLength(1))
  })

  it('refreshes every account balance from the wallet page', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])],
      foundAccounts: ['0.0.1003'],
      overview: { account_id: 'x', balance_tinybars: 100_000_000, transactions: [] },
    })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    expect(screen.getByText(/Click on an account above to see further account information/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Refresh accounts and balances' }))
    await waitFor(() => expect(commandsCalled('get_account_overview')).toHaveLength(3))
    expect(commandsCalled('find_wallet_accounts')).toHaveLength(1)
    expect(commandsCalled('get_account_overview').map((call) => call.args?.accountId))
      .toEqual(['0.0.1001', '0.0.1002', '0.0.1003'])
    expect(screen.getByRole('button', { name: /0\.0\.1003/ })).toHaveTextContent('1.00 ℏ')
  })

  it('refreshes existing accounts when discovery fails and reports balance errors', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])],
      overview: { account_id: 'x', balance_tinybars: 100_000_000, transactions: [] },
      overviewErrors: { '0.0.1002': 'mirror unavailable' },
    })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Refresh accounts and balances' }))
    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Account discovery failed')
    expect(alert).toHaveTextContent('Balance refresh for 0.0.1002 failed: mirror unavailable')
    expect(commandsCalled('get_account_overview')).toHaveLength(2)
    expect(screen.getByRole('button', { name: /0\.0\.1001/ })).toHaveTextContent('1.00 ℏ')
  })

  it('shows one account section at a time and hides it when its button is clicked again', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading')],
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    expect(screen.queryByRole('heading', { name: 'Send HBAR' })).not.toBeInTheDocument()
    await chooseMenu(user, 'Send', 'HBAR')
    await chooseMenu(user, 'Account management', 'Name this account')
    expect(screen.queryByRole('heading', { name: 'Send HBAR' })).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Name this account' })).toBeInTheDocument()
    await chooseMenu(user, 'Send', 'HBAR')
    expect(screen.queryByRole('heading', { name: 'Name this account' })).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Send HBAR' })).toBeInTheDocument()
    await chooseMenu(user, 'Account management', 'Move account')
    expect(screen.queryByRole('heading', { name: 'Send HBAR' })).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Move account to another wallet' })).toBeInTheDocument()
    await chooseMenu(user, 'Account management', 'Delete account')
    expect(screen.queryByRole('heading', { name: 'Move account to another wallet' })).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Delete account' })).toBeInTheDocument()
    await chooseMenu(user, 'Account management', 'Delete account')
    expect(screen.queryByRole('heading', { name: 'Delete account' })).not.toBeInTheDocument()
  })

  it('creates an account only after the confirmation is accepted', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')], foundAccounts: ['0.0.1003'] })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Create account')
    await user.selectOptions(await screen.findByLabelText('Fund from'), '0.0.1001-abcde')
    expect(screen.getByLabelText('Opening balance (ℏ)')).toHaveAttribute('placeholder', '1.2345')
    await user.type(screen.getByLabelText('Opening balance (ℏ)'), '2')
    await user.click(screen.getByRole('button', { name: 'Review new account' }))
    const confirm = screen.getByRole('group', { name: 'Confirm new account' })
    expect(within(confirm).getByText('2 ℏ')).toBeInTheDocument()
    expect(commandsCalled('create_account')).toHaveLength(0)

    await user.click(screen.getByRole('button', { name: 'Sign and create' }))
    expect(await screen.findByText('SUCCESS!')).toBeInTheDocument()
    expect(commandsCalled('create_account')[0].args).toEqual({
      walletId: 'w1',
      fromAccount: '0.0.1001-abcde',
      initialHbar: '2',
    })
  })

  it('rejects an invalid or unaffordable opening balance', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Create account')
    await user.selectOptions(await screen.findByLabelText('Fund from'), '0.0.1001-abcde')
    const field = screen.getByLabelText('Opening balance (ℏ)')
    await user.type(field, '1.2.3')
    await user.click(screen.getByRole('button', { name: 'Review new account' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('valid opening balance')
    await user.clear(field)
    await user.type(field, '500')
    await user.click(screen.getByRole('button', { name: 'Review new account' }))
    expect(await screen.findByText(/less than the opening balance/)).toBeInTheDocument()
    expect(screen.queryByRole('group', { name: 'Confirm new account' })).not.toBeInTheDocument()
  })

  it('moves an account to another wallet after confirmation and Testnet’s confirmation', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading')] })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Move account')
      await user.selectOptions(await screen.findByLabelText('New wallet'), 'w2')
      await user.click(screen.getByRole('button', { name: 'Review move' }))
      const confirm = screen.getByRole('group', { name: 'Confirm account move' })
      expect(within(confirm).getByText('Trading')).toBeInTheDocument()
      expect(commandsCalled('move_account')).toHaveLength(0)

      expect(screen.getByRole('button', { name: 'Sign and move' })).toBeDisabled()
      await user.click(within(confirm).getByRole('checkbox'))
      await user.click(screen.getByRole('button', { name: 'Sign and move' }))
      await screen.findByText('SUCCESS!')
      expect(commandsCalled('move_account')[0].args).toEqual({
        walletId: 'w1',
        accountId: '0.0.1001',
        targetWalletId: 'w2',
      })
      await waitFor(() => expect(commandsCalled('finish_account_move')).toHaveLength(1))
      await waitFor(() => expect(screen.queryByRole('button', { name: 'Sign and move' })).not.toBeInTheDocument())
      expect(backend.wallets[0].accounts).toHaveLength(0)
      expect(backend.wallets[1].accounts).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('deletes an account only after the warning is acknowledged and Testnet confirms', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
      installBackend({ wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])] })
      render(<App />)
      await unlock(user)
      await openWalletTab(user, 'Savings')
      await user.click(await screen.findByRole('button', { name: /0\.0\.1001/ }))
      await chooseMenu(user, 'Account management', 'Delete account')
      await user.selectOptions(
        await screen.findByLabelText('Recipient from the address book'),
        '0.0.1002-abcde',
      )
      await user.click(screen.getByRole('button', { name: 'Review deletion' }))
      const confirm = await screen.findByRole('group', { name: 'Confirm account deletion' })
      expect(within(confirm).getByRole('alert')).toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Sign and delete' })).toBeDisabled()
      await user.click(within(confirm).getByRole('checkbox'))
      await user.click(screen.getByRole('button', { name: 'Sign and delete' }))
      await screen.findByText('SUCCESS!')
      expect(commandsCalled('delete_account')[0].args).toEqual({
        walletId: 'w1',
        accountId: '0.0.1001',
        recipient: '0.0.1002-abcde',
      })
      await waitFor(() => expect(commandsCalled('finish_account_delete')).toHaveLength(1))
      expect(backend.wallets[0].accounts.map((item) => item.id)).toEqual(['0.0.1002'])
    } finally {
      vi.useRealTimers()
    }
  })

  it('lists several accounts per wallet, totals them, and sends from the selected one', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])],
      overview: { account_id: 'x', balance_tinybars: 150_000_000, transactions: [] },
    })
    render(<App />)
    await unlock(user)
    const rows = await screen.findByRole('list', { name: 'Savings accounts' })
    await waitFor(() => expect(within(rows).getAllByText('1.50 ℏ')).toHaveLength(2))
    expect(screen.getByLabelText('Total balance')).toHaveTextContent('3.00 ℏ')

    await openWalletTab(user, 'Savings')
    expect(commandsCalled('get_account_overview')).toHaveLength(0)
    expect(screen.queryByLabelText('Recipient account ID')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /0\.0\.1002/ }))
    await chooseMenu(user, 'Send', 'HBAR')
    await typeRecipient(user, '0.0.2002-abcde')
    await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await user.click(await screen.findByRole('button', { name: 'Sign and send' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('send_hbar')[0].args).toMatchObject({ fromAccount: '0.0.1002' })
  })

  it('leaves the total blank when no balance is known', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    expect(screen.getByLabelText('Total balance')).toHaveTextContent('')
  })

  it('finds the accounts from the wallet key, or explains when none exist', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')

    const find = 'Refresh accounts and balances'
    await user.click(screen.getByRole('button', { name: find }))
    expect(await screen.findByRole('alert')).toHaveTextContent('no Testnet account')

    backend.foundAccounts = ['0.0.2002', '0.0.2003']
    await user.click(screen.getByRole('button', { name: find }))
    expect(await screen.findByRole('button', { name: /0\.0\.2002/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /0\.0\.2003/ })).toBeInTheDocument()
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
            counterparty: '0.0.2002-abcde',
          },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Refresh balance' }))
    expect((await screen.findAllByText(/2\.50 ℏ/)).length).toBeGreaterThan(0)
    await user.click(screen.getByRole('button', { name: 'Recent transactions' }))
    const list = screen.getByRole('list', { name: 'Recent transactions' })
    expect(within(list).getByText('CRYPTOTRANSFER')).toBeInTheDocument()
    expect(within(list).getByText(/-1\.00 ℏ/)).toBeInTheDocument()
    expect(within(list).getByText('0.0.2002-abcde')).toBeInTheDocument()
  })

  it('says so when the account is not found on Testnet', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')], overview: null })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Refresh balance' }))
    expect(await screen.findByRole('status')).toHaveTextContent('not found on Hedera Testnet')
  })

  describe('send HBAR', () => {
    async function openSend(user: ReturnType<typeof userEvent.setup>) {
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      await typeRecipient(user, '0.0.2002-abcde')
      await user.type(screen.getByLabelText('Amount (ℏ)'), '1.5')
    }

    it('rejects a recipient without a checksum at review', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      await typeRecipient(user, '0.0.2002')
      await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      expect(await screen.findByRole('alert')).toHaveTextContent('checksum')
      expect(screen.queryByRole('group', { name: 'Confirm transfer' })).not.toBeInTheDocument()
    })

    it('shows a confirmation and sends only after it is accepted', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      await openSend(user)
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      const confirm = await screen.findByRole('group', { name: 'Confirm transfer' })
      expect(within(confirm).getByText('NONAME - 0.0.1001-abcde')).toBeInTheDocument()
      expect(within(confirm).getByText('0.0.2002-abcde')).toBeInTheDocument()
      expect(within(confirm).getByText('1.5 ℏ')).toBeInTheDocument()
      expect(commandsCalled('send_hbar')).toHaveLength(0)

      await user.click(screen.getByRole('button', { name: 'Sign and send' }))
      expect(await screen.findByText('SUCCESS!')).toBeInTheDocument()
      expect(commandsCalled('get_transaction_result')[0].args).toEqual({
        transactionId: '0.0.1001-1700000000-000000005',
      })
      expect(commandsCalled('send_hbar').map((call) => call.args)).toEqual([
        { walletId: 'w1', fromAccount: '0.0.1001', recipient: '0.0.2002-abcde', amountHbar: '1.5', memo: '' },
      ])
      expect(screen.queryByRole('group', { name: 'Confirm transfer' })).not.toBeInTheDocument()
    })

    it('reports a transaction that Testnet rejects', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')], transactionResult: 'INSUFFICIENT_PAYER_BALANCE' })
      await openSend(user)
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await user.click(await screen.findByRole('button', { name: 'Sign and send' }))
      expect(await screen.findByText(/The transaction failed/)).toBeInTheDocument()
      expect(screen.getByText(/INSUFFICIENT_PAYER_BALANCE/)).toBeInTheDocument()
      expect(screen.queryByText('SUCCESS!')).not.toBeInTheDocument()
      await user.click(screen.getByRole('alertdialog').parentElement!)
      expect(screen.queryByText(/The transaction failed/)).not.toBeInTheDocument()
    })

    it('shrinks to a corner spinner when clicked away, then shows a thumbs up and fades', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      await openSend(user)
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await user.click(await screen.findByRole('button', { name: 'Sign and send' }))
      await user.click((await screen.findByRole('alertdialog')).parentElement!)
      expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
      expect(await screen.findByLabelText('Transaction succeeded')).toBeInTheDocument()
      await waitFor(() => expect(screen.queryByLabelText('Transaction succeeded')).not.toBeInTheDocument(), {
        timeout: 4000,
      })
    })

    it('can go back from the confirmation without sending', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      await openSend(user)
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await user.click(await screen.findByRole('button', { name: 'Back' }))
      expect(screen.getByLabelText('Recipient account ID')).toHaveValue('0.0.2002-abcde')
      expect(commandsCalled('send_hbar')).toHaveLength(0)
    })

    it('refuses to send to the wallet’s own account', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      await typeRecipient(user, '0.0.1001')
      await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      expect(await screen.findByRole('alert')).toHaveTextContent('different account')
      expect(screen.queryByRole('group', { name: 'Confirm transfer' })).not.toBeInTheDocument()
    })

    it('shows the node’s error and keeps the confirmation', async () => {
      const user = userEvent.setup()
      installBackend({
        wallets: [wallet('w1', 'Savings', '0.0.1001')],
        sendError: 'The node rejected the transfer: INVALID_SIGNATURE',
      })
      await openSend(user)
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await user.click(await screen.findByRole('button', { name: 'Sign and send' }))
      expect(screen.getByRole('group', { name: 'Confirm transfer' })).toBeInTheDocument()
    })

    it('offers no send form until an account is linked', async () => {
      const user = userEvent.setup()
      installBackend()
      render(<App />)
      await unlock(user)
      await openWalletTab(user, 'Savings')
      expect(screen.queryByText('Send HBAR')).not.toBeInTheDocument()
    })

    it('limits recipients to the address book when outside sends are disabled', async () => {
      const user = userEvent.setup()
      installBackend({
        wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading', '0.0.1002')],
        allowExternalSends: false,
      })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      expect(await screen.findByLabelText('Check to send to a different account')).toBeDisabled()
      expect(screen.getByText(/Send limited to accounts in the address book/)).toBeInTheDocument()
      const book = screen.getByLabelText('Recipient from the address book')
      expect(within(book).queryByRole('option', { name: /0\.0\.1001/ })).not.toBeInTheDocument()
      await user.selectOptions(book, '0.0.1002-abcde')
      await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await user.click(await screen.findByRole('button', { name: 'Sign and send' }))
      await screen.findByText('SUCCESS!')
      expect(commandsCalled('send_hbar')[0].args).toMatchObject({ recipient: '0.0.1002-abcde' })
    })

    it('wipes a reviewed transfer when another section is opened', async () => {
      const user = userEvent.setup()
      installBackend({
        wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading', '0.0.1002')],
      })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      await user.selectOptions(await screen.findByLabelText('Recipient from the address book'), '0.0.1002-abcde')
      await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      await screen.findByRole('group', { name: 'Confirm transfer' })
      await chooseMenu(user, 'Account management', 'Name this account')
      await chooseMenu(user, 'Send', 'HBAR')
      expect(await screen.findByLabelText('Recipient from the address book')).toHaveValue('')
      expect(screen.getByLabelText('Amount (ℏ)')).toHaveValue('')
    })

    it('asks for a recipient when nothing is chosen', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      render(<App />)
      await unlock(user)
      await openAccount(user, 'Savings', 'Send')
      await user.type(await screen.findByLabelText('Amount (ℏ)'), '1')
      await user.click(screen.getByRole('button', { name: 'Review transfer' }))
      expect(await screen.findByRole('alert')).toHaveTextContent('Choose a recipient')
    })
  })

  describe('account names and the address book', () => {
    it('names and renames an account, showing the name first', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      render(<App />)
      await unlock(user)
      const rows = await screen.findByRole('list', { name: 'Savings accounts' })
      expect(rows).toHaveTextContent('NONAME - 0.0.1001-abcde')

      await openAccount(user, 'Savings')
      await chooseMenu(user, 'Account management', 'Name this account')
      expect(screen.getByRole('heading', { name: 'Name this account' })).toBeInTheDocument()
      expect(screen.getByLabelText(/Account name/)).toHaveAttribute('maxLength', '25')
      await user.type(screen.getByLabelText(/Account name/), 'Rent')
      await user.click(screen.getByRole('button', { name: 'Save name' }))
      await user.click(screen.getByRole('button', { name: 'Account management' }))
      expect(await screen.findByRole('menuitem', { name: 'Rename account' })).toBeInTheDocument()
      expect(screen.getAllByText('Rent').length).toBeGreaterThan(0)
      expect(commandsCalled('rename_account')[0].args).toMatchObject({ accountId: '0.0.1001', name: 'Rent' })
    })

    it('lists wallet accounts and adds and removes an outside account', async () => {
      const user = userEvent.setup()
      installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      render(<App />)
      await unlock(user)
      await user.click(screen.getByRole('button', { name: 'Address book' }))
      const list = await screen.findByRole('list', { name: 'Unnamed accounts' })
      expect(within(list).getByText('Savings')).toBeInTheDocument()

      // The add form is hidden until opened, and Close hides it again.
      expect(screen.queryByLabelText('Name')).not.toBeInTheDocument()
      await user.click(screen.getByRole('button', { name: 'Add outside account' }))
      await user.click(await screen.findByRole('button', { name: 'Close' }))
      expect(screen.queryByLabelText('Name')).not.toBeInTheDocument()

      await user.click(screen.getByRole('button', { name: 'Add outside account' }))
      await user.click(await screen.findByRole('button', { name: 'Add to address book' }))
      expect(await screen.findByRole('alert')).toHaveTextContent('name is required')

      await user.type(screen.getByLabelText('Name'), 'Landlord')
      await user.clear(screen.getByLabelText('Account number with checksum'))
      await user.type(screen.getByLabelText('Account number with checksum'), '0.0.3003-abcde')
      await user.click(screen.getByRole('button', { name: 'Add to address book' }))
      expect(await within(await screen.findByRole('list', { name: 'Address book' })).findByText('Landlord'))
        .toBeInTheDocument()
      expect(await screen.findAllByText('2.50 ℏ')).not.toHaveLength(0)

      await user.click(screen.getByRole('button', { name: 'Remove Landlord from the address book' }))
      await waitFor(() => expect(screen.queryByText('Landlord')).not.toBeInTheDocument())
    })

    it('lists unnamed accounts below the named ones and renames from the address book', async () => {
      const user = userEvent.setup()
      installBackend({
        wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])],
        outside: [{ id: '0.0.3003', name: 'Landlord' }],
      })
      render(<App />)
      await unlock(user)
      await user.click(screen.getByRole('button', { name: 'Address book' }))
      const named = await screen.findByRole('list', { name: 'Address book' })
      expect(within(named).getByText('Landlord')).toBeInTheDocument()
      const unnamed = screen.getByRole('list', { name: 'Unnamed accounts' })
      expect(within(unnamed).getAllByText('NONAME')).toHaveLength(2)

      await user.click(within(unnamed).getAllByText('NONAME')[0])
      await user.type(screen.getByLabelText('Name for 0.0.1001-abcde'), 'Rent')
      await user.click(screen.getByRole('button', { name: 'Save' }))
      await waitFor(() => expect(within(screen.getByRole('list', { name: 'Address book' })).getByText('Rent'))
        .toBeInTheDocument())
      expect(within(screen.getByRole('list', { name: 'Unnamed accounts' })).getAllByText('NONAME')).toHaveLength(1)

      await user.click(within(screen.getByRole('list', { name: 'Address book' })).getByText('Landlord'))
      await user.clear(screen.getByLabelText('Name for 0.0.3003-abcde'))
      await user.type(screen.getByLabelText('Name for 0.0.3003-abcde'), 'Owner')
      await user.click(screen.getByRole('button', { name: 'Save' }))
      expect(await screen.findByText('Owner')).toBeInTheDocument()
    })

    it('changes the outside-send setting from Settings', async () => {
      const user = userEvent.setup()
      installBackend({ allowExternalSends: false })
      render(<App />)
      await unlock(user)
      await user.click(screen.getByRole('button', { name: 'Settings' }))
      await user.click(await screen.findByLabelText(/Allow sending to accounts/))
      await waitFor(() => expect(backend.allowExternalSends).toBe(true))
    })

    it('changes the outside-delete setting from Settings', async () => {
      const user = userEvent.setup()
      installBackend({ allowExternalDeletes: false })
      render(<App />)
      await unlock(user)
      await user.click(screen.getByRole('button', { name: 'Settings' }))
      await user.click(await screen.findByLabelText(/funds of a deleted account/))
      await waitFor(() => expect(backend.allowExternalDeletes).toBe(true))
    })
  })
})

describe('tabs', () => {
  beforeEach(() => installBackend())

  it('shows the wallet type in its tab and puts the added date first in its details', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [
        wallet('w1', 'Hot'),
        { ...wallet('w2', 'Cold'), custody: 'cold' },
        { ...wallet('w3', 'Watch'), secret_kind: 'watchonly' },
      ],
    })
    render(<App />)
    await unlock(user)
    for (const [name, badge] of [['Hot', 'wallet-hot'], ['Cold', 'wallet-cold'], ['Watch', 'wallet-watch-only']]) {
      await openWalletTab(user, name)
      const tab = screen.getByRole('tab', { name })
      expect(tab.querySelector('.wallet-badge')).toHaveClass(badge)
      const details = screen.getByText('Key type').parentElement!
      expect(details.tagName).toBe('DL')
      expect(details.children[0]).toHaveTextContent('Added')
      expect(details.children[1]).toHaveTextContent(new Date('2026-01-01T00:00:00Z').toLocaleDateString())
      expect(details.children[2]).toHaveTextContent('Key type')
      expect(screen.queryByRole('heading', { name })).not.toBeInTheDocument()
      await user.click(screen.getByRole('tab', { name: 'Wallets' }))
    }
  })

  it('keeps the toolbar and Wallets tabs open, replacing the leftmost tab content', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)

    expect(screen.getAllByRole('tab').map((tab) => tab.getAttribute('aria-label') ?? tab.textContent))
      .toEqual(['Create / import', 'Wallets'])
    expect(screen.queryByRole('button', { name: /Close (create or import|Wallets) tab/i })).not.toBeInTheDocument()
    await openWalletTab(user, 'Savings')
    expect(screen.getByRole('tab', { name: 'Savings' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: 'Wallets' })).toHaveAttribute('aria-selected', 'false')

    const toolbarTab = screen.getByRole('tab', { name: 'Create / import' })
    await user.click(screen.getByRole('button', { name: /Settings/ }))
    expect(await screen.findByRole('tab', { name: 'Settings' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: 'Settings' })).toBe(toolbarTab)
    expect(screen.queryByRole('tab', { name: 'Create / import' })).not.toBeInTheDocument()
    expect(screen.getAllByRole('tab')).toHaveLength(3)
    await user.click(screen.getByRole('button', { name: 'Help' }))
    expect(screen.queryByRole('tab', { name: 'Settings' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Help' })).toHaveAttribute('aria-selected', 'true')
    await user.click(screen.getByRole('button', { name: 'Address book' }))
    expect(screen.queryByRole('tab', { name: 'Help' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Address book' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getAllByRole('tab')).toHaveLength(3)
    expect(screen.getAllByRole('tab')).toEqual([
      toolbarTab, screen.getByRole('tab', { name: 'Wallets' }), screen.getByRole('tab', { name: 'Savings' }),
    ])
    expect(screen.queryByRole('button', { name: /Close (settings|help|address book) tab/i })).not.toBeInTheDocument()
    await user.click(screen.getByRole('tab', { name: 'Wallets' }))
    await user.click(screen.getByRole('tab', { name: 'Address book' }))
    expect(screen.getByRole('tab', { name: 'Address book' })).toHaveAttribute('aria-selected', 'true')
    await user.click(screen.getByRole('button', { name: 'Create / import wallet' }))
    expect(screen.getByRole('tab', { name: 'Create / import' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('tab', { name: 'Address book' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.getByRole('tab', { name: 'Create / import' })).toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Wallets' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByRole('tab', { name: 'Savings' })).toBeInTheDocument()
  })

  it('closes a tab and falls back to HOME, which cannot be closed', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Trading')

    await user.click(screen.getByRole('button', { name: 'Close Trading tab' }))
    expect(screen.queryByRole('tab', { name: 'Trading' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Wallets' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('button', { name: /Close Wallets/ })).not.toBeInTheDocument()
  })

  it('clears settings passwords when another toolbar tab replaces settings', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: 'Settings' }))
    await user.type(screen.getByLabelText('Current vault password'), 'sensitive')
    await user.click(screen.getByRole('tab', { name: 'Wallets' }))
    await user.click(screen.getByRole('button', { name: 'Help' }))
    await user.click(screen.getByRole('button', { name: 'Settings' }))
    expect(screen.getByLabelText('Current vault password')).toHaveValue('')
    expect(screen.queryByRole('tab', { name: 'Help' })).not.toBeInTheDocument()
  })

  it('clears an unfinished import when the toolbar tab is replaced', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('tab', { name: 'Create / import' }))
    await user.click(screen.getByRole('button', { name: 'Import' }))
    await user.type(screen.getByLabelText('English BIP39 recovery phrase'), 'secret phrase')
    await user.click(screen.getByRole('button', { name: 'Settings' }))
    await user.click(screen.getByRole('button', { name: 'Create / import wallet' }))
    expect(screen.getByLabelText('English BIP39 recovery phrase')).toHaveValue('')
  })

  it('moves between tabs with the arrow keys', async () => {
    const user = userEvent.setup()
    render(<App />)
    await unlock(user)
    await openWalletTab(user, 'Savings')

    screen.getByRole('tab', { name: 'Savings' }).focus()
    await user.keyboard('{ArrowLeft}')
    expect(screen.getByRole('tab', { name: 'Wallets' })).toHaveFocus()
    await user.keyboard('{Home}')
    expect(screen.getByRole('tab', { name: 'Create / import' })).toHaveFocus()
    await user.keyboard('{End}')
    expect(screen.getByRole('tab', { name: 'Savings' })).toHaveFocus()
  })
})

describe('reveal flow', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  it('labels private-key reveal actions and dialog correctly', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [{
        ...wallet('w1', 'Savings'),
        secret_kind: 'privatekey' as const,
        key_type: 'ed25519' as const,
      }],
    })
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Reveal private key')
    expect(screen.getByRole('heading', { name: 'Private key' })).toBeInTheDocument()
  })

  async function openReveal(user: ReturnType<typeof userEvent.setup>) {
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Reveal recovery phrase')
    const dialog = screen.getByRole('dialog')
    await user.click(within(dialog).getByRole('button', { name: 'Reveal recovery phrase' }))
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
    expect(grid).not.toHaveClass('selectable-secret')

    const confirm = screen.getByRole('button', { name: 'Confirm and hide phrase' })
    expect(confirm).toBeDisabled()
    await user.click(screen.getByRole('checkbox'))
    expect(confirm).toBeEnabled()
    await user.click(confirm)
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
  })

  it('reveals a copyable private key for a recovery-phrase wallet', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Reveal private key')
    const dialog = screen.getByRole('dialog')
    expect(within(dialog).getByRole('heading', { name: 'Private key' })).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Reveal private key' }))
    await screen.findByText('ab'.repeat(32), { selector: 'strong' })
    expect(commandsCalled('reveal_wallet_private_key')[0].args).toEqual({ walletId: 'w1' })
    expect(commandsCalled('reveal_wallet_mnemonic')).toHaveLength(0)
    const grid = document.querySelector('.mnemonic-grid')!
    expect(grid.dispatchEvent(new Event('copy', { bubbles: true, cancelable: true }))).toBe(true)
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
    await chooseMenu(user, 'Wallet management', 'Reveal recovery phrase')
    const dialog = await screen.findByRole('dialog')
    await user.click(within(dialog).getByRole('button', { name: 'Reveal recovery phrase' }))
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
    await chooseMenu(user, 'Wallet management', 'Delete wallet')
    const dialog = await screen.findByRole('alertdialog')
    expect(within(dialog).getByText('Delete Savings?')).toBeInTheDocument()
    await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument()
    expect(commandsCalled('delete_wallet')).toHaveLength(0)
    expect(screen.getByRole('tab', { name: 'Savings' })).toBeInTheDocument()
  })

  it('keeps Delete wallet disabled until the checkbox is ticked', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Delete wallet')
    const dialog = await screen.findByRole('alertdialog')
    expect(within(dialog).getByRole('button', { name: 'Delete wallet' })).toBeDisabled()
    await user.click(within(dialog).getByRole('checkbox'))
    expect(within(dialog).getByRole('button', { name: 'Delete wallet' })).toBeEnabled()
  })

  it('deletes the wallet, closes its tab and returns to HOME', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Delete wallet')
    const dialog = await screen.findByRole('alertdialog')
    await user.click(within(dialog).getByRole('checkbox'))
    await user.click(within(dialog).getByRole('button', { name: 'Delete wallet' }))

    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(commandsCalled('delete_wallet')[0].args).toEqual({ walletId: 'w1' })
    expect(screen.queryByRole('tab', { name: 'Savings' })).not.toBeInTheDocument()
    expect(screen.getByRole('tab', { name: 'Wallets' })).toHaveAttribute('aria-selected', 'true')
    expect(screen.queryByRole('button', { name: /Savings/ })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Trading/ })).toBeInTheDocument()
  })

  it('keeps the wallet and shows the error when deletion fails', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Delete wallet')
    const dialog = await screen.findByRole('alertdialog')
    invokeMock.mockRejectedValueOnce(new Error('could not save vault'))
    await user.click(within(dialog).getByRole('checkbox'))
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

  it('detects the key type, imports the key and links every account found', async () => {
    backend.detection = { keyType: 'ecdsa', accounts: ['0.0.555'] }
    backend.foundAccounts = ['0.0.555', '0.0.556']
    const user = userEvent.setup()
    await openKeyImport(user)
    await user.type(screen.getByLabelText('Private key (hex or DER)'), '11'.repeat(32))
    await user.type(screen.getByLabelText('Wallet name'), 'Funded')
    await user.click(screen.getByRole('button', { name: /Import encrypted wallet/ }))

    await screen.findByRole('tab', { name: 'Wallets' })
    expect(commandsCalled('import_private_key')[0].args).toEqual({
      name: 'Funded',
      privateKey: '11'.repeat(32),
      keyType: 'ecdsa',
    })

    expect(commandsCalled('find_wallet_accounts')[0].args).toEqual({ walletId: 'k3' })
    expect((await screen.findAllByText('0.0.556-abcde')).length).toBeGreaterThan(0)
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
    await screen.findByRole('tab', { name: 'Wallets' })
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

describe('watch-only wallet', () => {
  it('imports a public key and prevents signing actions in the UI', async () => {
    const user = userEvent.setup()
    installBackend()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    await user.click(await screen.findByRole('button', { name: 'Import' }))
    await user.click(screen.getByRole('button', { name: 'Watch-only public key' }))
    const publicKey = 'd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a'
    expect(screen.queryByLabelText('Public key (hex or DER)')).not.toBeInTheDocument()
    const scanButton = screen.getByRole('button', { name: 'Scan to create wallet' })
    expect(scanButton).toBeDisabled()
    await user.type(screen.getByLabelText('Wallet name'), 'Monitor')
    expect(scanButton).toBeEnabled()
    scanner.reply = `HVTX1-PUB:ed25519:${publicKey}`
    await user.click(scanButton)
    await user.click(await screen.findByRole('button', { name: 'Deliver scanned reply' }))

    expect(commandsCalled('import_watch_only_wallet')[0].args).toEqual({
      name: 'Monitor',
      publicKey,
      keyType: 'ed25519',
      network: 'testnet',
    })
    await openWalletTab(user, 'Monitor')
    expect(screen.getByTitle('Watch-only wallet')).toHaveClass('wallet-watch-only')
    await user.click(screen.getByRole('button', { name: 'Wallet management' }))
    expect(screen.queryByRole('menuitem', { name: /Reveal/ })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Rename wallet' })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Delete wallet' })).toBeInTheDocument()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('heading', { name: 'Create account' })).not.toBeInTheDocument()
    await chooseMenu(user, 'Wallet management', 'Create account')
    expect(screen.getByRole('heading', { name: 'Create account' })).toBeInTheDocument()
  })

  it('rejects a malformed public-key QR without creating a wallet', async () => {
    const user = userEvent.setup()
    installBackend()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    await user.click(await screen.findByRole('button', { name: 'Import' }))
    await user.click(screen.getByRole('button', { name: 'Watch-only public key' }))
    await user.type(screen.getByLabelText('Wallet name'), 'Monitor')
    scanner.reply = 'HVTX1-PUB:rsa:deadbeef'
    await user.click(screen.getByRole('button', { name: 'Scan to create wallet' }))
    await user.click(await screen.findByRole('button', { name: 'Deliver scanned reply' }))

    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The scanned code is not a public key from the signer.',
    )
    expect(commandsCalled('import_watch_only_wallet')).toHaveLength(0)
  })

  it('clears the form and closes the tab when Cancel is clicked', async () => {
    const user = userEvent.setup()
    installBackend()
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    await user.type(screen.getByLabelText('Wallet name'), 'Draft')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByLabelText('Wallet name')).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))
    expect(screen.getByLabelText('Wallet name')).toHaveValue('')
  })

  it('shows balances but omits transaction controls for a linked account', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      overview: { account_id: 'x', balance_tinybars: 100_000_000, transactions: [] },
      wallets: [{
        ...wallet('w-watch', 'Monitor', ['0.0.1001']),
        secret_kind: 'watchonly',
        key_type: 'ed25519',
      }],
    })
    render(<App />)
    await openAccount(user, 'Monitor')
    expect(screen.getByRole('button', { name: 'Refresh balance' })).toBeInTheDocument()
    expect(await screen.findByRole('button', { name: 'Refresh transactions' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Send' })).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Account management' }))
    expect(screen.getByRole('menuitem', { name: 'Name this account' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Move account' })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Delete account' })).toBeInTheDocument()
  })

  it('swaps Recent transactions for Account assets, listing HBAR then tokens alphabetically', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [
          { token_id: '0.0.9', symbol: 'ZED', name: 'Zed Coin', balance: '5', decimals: 0 },
          { token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 },
          { token_id: '0.0.7', symbol: 'EMP', name: 'Empty', balance: '0', decimals: 0 },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    expect(await screen.findByRole('heading', { name: 'Account assets' })).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Recent transactions' })).not.toBeInTheDocument()
    const rows = within(screen.getByRole('list', { name: 'Account assets' })).getAllByRole('listitem')
    expect(rows.map((row) => row.textContent)).toEqual([
      'HBAR1.50 ℏ',
      'ALP Alpha0.0.8123.45',
      'EMP Empty0.0.70',
      'ZED Zed Coin0.0.95',
    ])
    await user.click(screen.getByRole('button', { name: 'Recent transactions' }))
    expect(screen.getByRole('heading', { name: 'Recent transactions' })).toBeInTheDocument()
    expect(screen.queryByRole('list', { name: 'Account assets' })).not.toBeInTheDocument()
  })

  it('lists tokens in the Send menu with balances, greying out empty ones', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [
          { token_id: '0.0.9', symbol: 'ZED', name: 'Zed Coin', balance: '5', decimals: 0 },
          { token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 },
          { token_id: '0.0.7', symbol: 'EMP', name: 'Empty', balance: '0', decimals: 0 },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    const items = screen.getAllByRole('menuitem')
    expect(items.map((item) => item.textContent)).toEqual(['HBAR', 'ALP123.45', 'EMP0', 'ZED5'])
    expect(screen.getByRole('menuitem', { name: /EMP/ })).toBeDisabled()
    await user.click(screen.getByRole('menuitem', { name: /ZED/ }))
    expect(screen.getByRole('heading', { name: 'Send ZED' })).toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Send HBAR' })).not.toBeInTheDocument()
  })

  it('switches straight from a token send to an HBAR send with one click', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    await user.click(screen.getByRole('menuitem', { name: /ALP/ }))
    expect(screen.getByRole('heading', { name: 'Send ALP' })).toBeInTheDocument()
    await chooseMenu(user, 'Send', 'HBAR')
    expect(screen.getByRole('heading', { name: 'Send HBAR' })).toBeInTheDocument()
  })

  it('sends a token with the chosen amount after review', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading', '0.0.1002')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    await user.click(screen.getByRole('menuitem', { name: /ALP/ }))
    await user.selectOptions(screen.getByLabelText('Recipient from the address book'), '0.0.1002-abcde')
    await user.type(screen.getByLabelText('Amount'), '1.5')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    expect(await screen.findByText('1.5 ALP')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Sign and send' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('send_token')[0].args).toMatchObject({
      fromAccount: '0.0.1001',
      tokenId: '0.0.8',
      amount: '1.5',
    })
    expect(commandsCalled('send_hbar')).toHaveLength(0)
  })

  it('looks up a token from Token management', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await chooseMenu(user, 'Token management', 'Token info')
    await user.type(screen.getByLabelText('Token ID'), '0.0.8')
    await user.click(screen.getByRole('button', { name: 'Look up token' }))
    const details = await screen.findByLabelText('Token details')
    expect(commandsCalled('get_token_info')[0].args).toEqual({ tokenId: '0.0.8' })
    expect(details).toHaveTextContent('Alpha')
    expect(details).toHaveTextContent('1,000.00')
    expect(details).toHaveTextContent('Yes (the token can be changed or deleted)')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('heading', { name: 'Token info' })).not.toBeInTheDocument()
  })

  it('creates a token after a review step, and Cancel discards the form', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await chooseMenu(user, 'Token management', 'Create token')
    await user.click(screen.getByRole('button', { name: 'Review token' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a token name and symbol.')
    await user.type(screen.getByLabelText('Token name'), 'Gold')
    await user.type(screen.getByLabelText('Symbol'), 'GLD')
    await user.clear(screen.getByLabelText(/Decimals/))
    await user.type(screen.getByLabelText(/Decimals/), '2')
    await user.click(screen.getByRole('button', { name: 'Review token' }))
    expect(screen.getByRole('alert')).toHaveTextContent('initial supply')
    await user.type(screen.getByLabelText(/Initial supply/), '1000')
    await user.click(screen.getByRole('button', { name: 'Review token' }))
    const review = screen.getByRole('group', { name: 'Confirm token creation' })
    expect(review).toHaveTextContent('Gold')
    expect(review).toHaveTextContent('at most 20')
    expect(commandsCalled('create_token')).toHaveLength(0)
    await user.click(screen.getByRole('button', { name: 'Sign and create token' }))
    await waitFor(() => expect(commandsCalled('create_token')).toHaveLength(1))
    expect(commandsCalled('create_token')[0].args).toEqual({
      walletId: 'w1',
      fromAccount: '0.0.1001',
      name: 'Gold',
      symbol: 'GLD',
      decimals: 2,
      initialSupply: '1000',
    })
    expect(screen.queryByRole('heading', { name: 'Create token' })).not.toBeInTheDocument()

    await chooseMenu(user, 'Token management', 'Create token')
    await user.type(screen.getByLabelText('Token name'), 'Temp')
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('heading', { name: 'Create token' })).not.toBeInTheDocument()
  })

  it('deletes a held token only after it is chosen and acknowledged', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await chooseMenu(user, 'Token management', 'Delete token')
    const submit = screen.getByRole('button', { name: 'Sign and delete token' })
    expect(submit).toBeDisabled()
    await user.selectOptions(screen.getByLabelText('Token to delete'), '0.0.8')
    expect(submit).toBeDisabled()
    await user.click(screen.getByRole('checkbox'))
    await user.click(submit)
    await waitFor(() => expect(commandsCalled('delete_token')).toHaveLength(1))
    expect(commandsCalled('delete_token')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.8',
    })
  })

  it('greys out Mint and Burn when the account holds no supply-key token', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      supplyTokens: [],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Token management' }))
    expect(screen.getByRole('menuitem', { name: /Mint token/ })).toBeDisabled()
    expect(screen.getByRole('menuitem', { name: /Burn token/ })).toBeDisabled()
  })

  it('mints and burns a chosen token only with a valid amount', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await chooseMenu(user, 'Token management', 'Mint token')
    await user.click(screen.getByRole('button', { name: 'Sign and mint' }))
    expect(await screen.findByText('Choose a token.')).toBeInTheDocument()
    expect(commandsCalled('change_token_supply')).toHaveLength(0)
    await user.selectOptions(screen.getByLabelText('Token'), '0.0.8')
    await user.type(screen.getByLabelText('Amount to mint (whole tokens)'), '0')
    await user.click(screen.getByRole('button', { name: 'Sign and mint' }))
    expect(await screen.findByText(/greater than zero/)).toBeInTheDocument()
    await user.clear(screen.getByLabelText('Amount to mint (whole tokens)'))
    await user.type(screen.getByLabelText('Amount to mint (whole tokens)'), '12.5')
    await user.click(screen.getByRole('button', { name: 'Sign and mint' }))
    await waitFor(() => expect(commandsCalled('change_token_supply')).toHaveLength(1))
    expect(commandsCalled('change_token_supply')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.8',
      amount: '12.5',
      mint: true,
    })

    await chooseMenu(user, 'Token management', 'Burn token')
    await user.selectOptions(screen.getByLabelText('Token'), '0.0.8')
    await user.type(screen.getByLabelText('Amount to burn (whole tokens)'), '3')
    await user.click(screen.getByRole('button', { name: 'Sign and burn' }))
    await waitFor(() => expect(commandsCalled('change_token_supply')).toHaveLength(2))
    expect(commandsCalled('change_token_supply')[1].args).toMatchObject({ amount: '3', mint: false })
  })

  it('adds a token only after a lookup and review, and removes one with a zero balance', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [
          { token_id: '0.0.7', symbol: 'EMP', name: 'Empty', balance: '0', decimals: 0 },
          { token_id: '0.0.9', symbol: 'ZED', name: 'Zed Coin', balance: '5', decimals: 0 },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await chooseMenu(user, 'Token management', 'Associate token')
    expect(screen.queryByRole('button', { name: 'Sign and add token' })).not.toBeInTheDocument()
    await user.type(screen.getByLabelText('Token ID'), '0.0.8')
    await user.click(screen.getByRole('button', { name: 'Look up token' }))
    expect(await screen.findByRole('group', { name: 'Confirm adding token' })).toHaveTextContent('Alpha')
    expect(commandsCalled('associate_token')).toHaveLength(0)
    await user.click(screen.getByRole('button', { name: 'Sign and add token' }))
    await waitFor(() => expect(commandsCalled('associate_token')).toHaveLength(1))
    expect(commandsCalled('associate_token')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.8',
    })

    await chooseMenu(user, 'Token management', 'Disassociate token')
    const submit = screen.getByRole('button', { name: 'Sign and remove token' })
    expect(submit).toBeDisabled()
    expect(screen.getByRole('option', { name: /ZED.*must be 0/ })).toBeDisabled()
    await user.selectOptions(screen.getByLabelText('Token to remove'), '0.0.7')
    await user.click(submit)
    await waitFor(() => expect(commandsCalled('dissociate_token')).toHaveLength(1))
    expect(commandsCalled('dissociate_token')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.7',
    })
  })

  it('hides a token everywhere and shows it again on request', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [
          { token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 },
          { token_id: '0.0.9', symbol: 'ZED', name: 'Zed Coin', balance: '5', decimals: 0 },
        ],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await chooseMenu(user, 'Token management', 'Hide or show tokens')
    await user.click(screen.getByRole('button', { name: 'Hide ZED' }))
    expect(await screen.findByRole('button', { name: 'Show ZED' })).toBeInTheDocument()
    expect(commandsCalled('set_token_hidden')[0].args).toEqual({
      walletId: 'w1',
      tokenId: '0.0.9',
      hidden: true,
    })
    await user.click(screen.getByRole('button', { name: 'Close' }))

    await user.click(screen.getByRole('button', { name: 'Send' }))
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual(['HBAR', 'ALP123.45'])
    await user.keyboard('{Escape}')
    const rows = () => within(screen.getByRole('list', { name: 'Account assets' })).getAllByRole('listitem')
    expect(rows().map((row) => row.textContent)).toEqual(['HBAR1.50 ℏ', 'ALP Alpha0.0.8123.45'])
    await user.click(screen.getByRole('button', { name: 'Show hidden tokens (1)' }))
    expect(rows().map((row) => row.textContent)).toEqual([
      'HBAR1.50 ℏ',
      'ALP Alpha0.0.8123.45',
      'ZED Zed Coin (hidden)0.0.95',
    ])
    await user.click(screen.getByRole('button', { name: 'Hide hidden tokens' }))
    expect(rows()).toHaveLength(2)
  })

  it('lets watch-only wallets associate and disassociate tokens through a signing request', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [{ ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' }],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.7', symbol: 'EMP', name: 'Empty', balance: '0', decimals: 0 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Watcher')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Token management' }))
    expect(screen.getByRole('menuitem', { name: /Associate token/ })).toBeEnabled()
    expect(screen.getByRole('menuitem', { name: /Disassociate token/ })).toBeEnabled()
    expect(screen.getByRole('menuitem', { name: /Hide or show tokens/ })).toBeEnabled()
    await user.keyboard('{Escape}')

    await chooseMenu(user, 'Token management', 'Associate token')
    await user.type(screen.getByLabelText('Token ID'), '0.0.8')
    await user.click(screen.getByRole('button', { name: 'Look up token' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))
    const request = await screen.findByRole('group', { name: 'Signing request' })
    expect(request).toHaveTextContent('Associate token')
    expect(request).toHaveTextContent('0.0.8')
    expect(commandsCalled('prepare_unsigned_token_association')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.8',
      associate: true,
    })
    expect(commandsCalled('associate_token')).toHaveLength(0)
    await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
    await user.click(screen.getByRole('button', { name: 'Submit signed association' }))
    await waitFor(() => expect(commandsCalled('submit_signed_request')).toHaveLength(1))

    await chooseMenu(user, 'Token management', 'Disassociate token')
    await user.selectOptions(screen.getByLabelText('Token to remove'), '0.0.7')
    await user.click(screen.getByRole('button', { name: 'Create signing request' }))
    expect(await screen.findByRole('group', { name: 'Signing request' })).toHaveTextContent('Disassociate token')
    expect(commandsCalled('prepare_unsigned_token_association')[1].args).toMatchObject({
      tokenId: '0.0.7',
      associate: false,
    })
    expect(commandsCalled('dissociate_token')).toHaveLength(0)
  })

  it('sends a token from a watch-only wallet through a signing request', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [
        { ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' },
        wallet('w2', 'Trading', '0.0.1002'),
      ],
      overview: {
        account_id: 'x',
        balance_tinybars: 150_000_000,
        transactions: [],
        tokens: [{ token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 }],
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Watcher')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    await user.click(screen.getByRole('menuitem', { name: /ALP/ }))
    await user.selectOptions(screen.getByLabelText('Recipient from the address book'), '0.0.1002-abcde')
    await user.type(screen.getByLabelText('Amount'), '1.5')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))
    const request = await screen.findByRole('group', { name: 'Signing request' })
    expect(request).toHaveTextContent('1.5')
    expect(request).toHaveTextContent('(raw 150)')
    expect(commandsCalled('prepare_unsigned_token_transfer')[0].args).toMatchObject({
      fromAccount: '0.0.1001',
      tokenId: '0.0.8',
      amount: '1.5',
    })
    expect(commandsCalled('prepare_unsigned_transfer')).toHaveLength(0)
    await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
    await user.click(screen.getByRole('button', { name: 'Submit signed transfer' }))
    await screen.findByText('SUCCESS!')
  })

  it('disables token creation and deletion for watch-only wallets', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [{ ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' }],
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Watcher')
    await user.click(screen.getByRole('button', { name: 'Token management' }))
    expect(screen.getByRole('menuitem', { name: /Create token/ })).toBeDisabled()
    expect(screen.getByRole('menuitem', { name: /Delete token/ })).toBeDisabled()
    expect(screen.getByRole('menuitem', { name: /Mint token/ })).toBeDisabled()
    expect(screen.getByRole('menuitem', { name: /Burn token/ })).toBeDisabled()
  })

  it('closes the Send, Move and Delete sections with Cancel', async () => {
    const user = userEvent.setup()
    installBackend({ wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading')] })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    const cases: [string, string, string][] = [
      ['Send', 'HBAR', 'Send HBAR'],
      ['Account management', 'Move account', 'Move account to another wallet'],
      ['Account management', 'Delete account', 'Delete account'],
    ]
    for (const [menu, item, heading] of cases) {
      await chooseMenu(user, menu, item)
      expect(screen.getByRole('heading', { name: heading })).toBeInTheDocument()
      await user.click(screen.getByRole('button', { name: item === 'HBAR' ? 'Close' : 'Cancel' }))
      expect(screen.queryByRole('heading', { name: heading })).not.toBeInTheDocument()
    }
  })

  it('closes a dropdown menu with Escape and refreshes everything from either refresh icon', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: { account_id: 'x', balance_tinybars: 100_000_000, transactions: [] },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Send' }))
    expect(screen.getByRole('menuitem', { name: 'HBAR' })).toBeInTheDocument()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('menuitem', { name: 'HBAR' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Account management' }))
    await user.click(screen.getByRole('tab', { name: 'Savings' }))
    expect(screen.queryByRole('menuitem', { name: 'Delete account' })).not.toBeInTheDocument()

    await screen.findByRole('button', { name: 'Refresh transactions' })
    const before = commandsCalled('get_account_overview').length
    await user.click(await screen.findByRole('button', { name: 'Refresh balance' }))
    await waitFor(() => expect(commandsCalled('get_account_overview')).toHaveLength(before + 1))
    await user.click(await screen.findByRole('button', { name: 'Refresh transactions' }))
    await waitFor(() => expect(commandsCalled('get_account_overview')).toHaveLength(before + 2))
  })

  it('exports a signing request, then submits the pasted signature', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [{
        ...wallet('w-watch', 'Monitor', ['0.0.1001']),
        secret_kind: 'watchonly',
        key_type: 'ed25519',
      }],
    })
    render(<App />)
    await openAccount(user, 'Monitor')
    await chooseMenu(user, 'Send', 'HBAR')
    await typeRecipient(user, '0.0.2002-abcde')
    await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))

    expect(await screen.findByRole('img', { name: 'Signing request QR code' })).toBeInTheDocument()
    expect(screen.queryByText(/network fee/i)).not.toBeInTheDocument()
    expect(commandsCalled('send_hbar')).toHaveLength(0)
    const submit = screen.getByRole('button', { name: 'Submit signed transfer' })
    expect(submit).toBeDisabled()
    await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
    await user.click(submit)
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('submit_signed_request')[0].args).toEqual({
      request: 'HVTX1-REQ:00',
      response: 'HVTX1-SIG:AB',
    })
  })

  it('deletes a watch-only account through a signing request and Testnet’s confirmation', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime })
      installBackend({
        status: 'unlocked',
        wallets: [{
          ...wallet('w-watch', 'Monitor', ['0.0.1001', '0.0.1002']),
          secret_kind: 'watchonly',
          key_type: 'ed25519',
        }],
      })
      render(<App />)
      await openWalletTab(user, 'Monitor')
      await user.click(await screen.findByRole('button', { name: /0\.0\.1001/ }))
      await chooseMenu(user, 'Account management', 'Delete account')
      expect(screen.queryByLabelText('Check to send to a different account')).not.toBeInTheDocument()
      await user.selectOptions(
        await screen.findByLabelText('Recipient (another account of this wallet)'),
        '0.0.1002-abcde',
      )
      await user.click(screen.getByRole('button', { name: 'Review deletion' }))
      const confirm = await screen.findByRole('group', { name: 'Confirm account deletion' })
      await user.click(within(confirm).getByRole('checkbox'))
      await user.click(screen.getByRole('button', { name: 'Create signing request' }))
      expect(await screen.findByRole('img', { name: 'Signing request QR code' })).toBeInTheDocument()
      expect(commandsCalled('prepare_unsigned_delete')[0].args).toEqual({
        walletId: 'w-watch',
        accountId: '0.0.1001',
        recipient: '0.0.1002-abcde',
      })
      expect(commandsCalled('delete_account')).toHaveLength(0)
      await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
      await user.click(screen.getByRole('button', { name: 'Submit signed deletion' }))
      await screen.findByText('SUCCESS!')
      await waitFor(() => expect(commandsCalled('finish_account_delete')).toHaveLength(1))
      expect(backend.wallets[0].accounts.map((item) => item.id)).toEqual(['0.0.1002'])
    } finally {
      vi.useRealTimers()
    }
  })

  it('creates an account for a watch-only wallet through a signing request', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [{
        ...wallet('w-watch', 'Monitor', ['0.0.1001']),
        secret_kind: 'watchonly',
        key_type: 'ed25519',
      }],
      foundAccounts: ['0.0.1003'],
    })
    render(<App />)
    await openWalletTab(user, 'Monitor')
    await chooseMenu(user, 'Wallet management', 'Create account')
    await user.selectOptions(await screen.findByLabelText('Fund from'), '0.0.1001-abcde')
    await user.type(screen.getByLabelText('Opening balance (ℏ)'), '2')
    await user.click(screen.getByRole('button', { name: 'Review new account' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))
    expect(await screen.findByText(/this wallet.s own key/)).toBeInTheDocument()
    expect(commandsCalled('create_account')).toHaveLength(0)
    await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
    await user.click(screen.getByRole('button', { name: 'Submit signed account creation' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('prepare_unsigned_create')[0].args).toEqual({
      walletId: 'w-watch',
      fromAccount: '0.0.1001',
      initialHbar: '2',
    })
    await waitFor(() => expect(backend.wallets[0].accounts).toHaveLength(2))
  })

  it('offers camera scanning of the reply and falls back to pasting when no camera exists', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [{
        ...wallet('w-watch', 'Monitor', ['0.0.1001']),
        secret_kind: 'watchonly',
        key_type: 'ed25519',
      }],
    })
    render(<App />)
    await openAccount(user, 'Monitor')
    await chooseMenu(user, 'Send', 'HBAR')
    await typeRecipient(user, '0.0.2002-abcde')
    await user.type(screen.getByLabelText('Amount (ℏ)'), '1')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))
    await user.click(await screen.findByRole('button', { name: 'Scan reply with camera' }))

    expect(await screen.findByText(/does not allow camera access/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Stop scanning' }))
    expect(screen.getByRole('button', { name: 'Scan reply with camera' })).toBeInTheDocument()
    expect(screen.getByLabelText('Signature from the signer')).toBeInTheDocument()
  })
})

describe('rename flow', () => {
  beforeEach(() => installBackend({ status: 'unlocked' }))

  it('switches between create account and rename above the accounts list and closes creation', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    expect(screen.queryByRole('heading', { name: 'Create account' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Wallet management' }))
    expect(screen.getAllByRole('menuitem').slice(0, 2).map((item) => item.textContent))
      .toEqual(['Rename wallet', 'Create account'])
    await user.click(screen.getByRole('menuitem', { name: 'Create account' }))
    const create = screen.getByRole('region', { name: 'Create account' })
    const accounts = screen.getByRole('group', { name: 'Accounts' })
    expect(create.compareDocumentPosition(accounts) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(within(create).getByRole('button', { name: 'Close' })).toBeInTheDocument()
    await user.type(screen.getByLabelText('Account name'), 'Draft')
    await chooseMenu(user, 'Wallet management', 'Rename wallet')
    expect(screen.queryByRole('heading', { name: 'Create account' })).not.toBeInTheDocument()
    const rename = screen.getByLabelText('New wallet name')
    expect(rename.closest('form')!.compareDocumentPosition(accounts) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    await chooseMenu(user, 'Wallet management', 'Create account')
    expect(screen.queryByLabelText('New wallet name')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Account name')).toHaveValue('')
    await user.click(within(screen.getByRole('region', { name: 'Create account' })).getByRole('button', { name: 'Close' }))
    expect(screen.queryByRole('heading', { name: 'Create account' })).not.toBeInTheDocument()
  })

  it('renames a wallet and updates its tab and the HOME list', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Rename wallet')
    const input = screen.getByLabelText('New wallet name')
    expect(input).toHaveValue('Savings')
    await user.clear(input)
    await user.type(input, 'Rainy day')
    await user.click(screen.getByRole('button', { name: 'Save name' }))

    expect(await screen.findByRole('tab', { name: 'Rainy day' })).toBeInTheDocument()
    expect(commandsCalled('rename_wallet')[0].args).toEqual({ walletId: 'w1', name: 'Rainy day' })
    expect(screen.queryByLabelText('New wallet name')).not.toBeInTheDocument()
  })

  it('shows the error for a duplicate name and keeps the old name', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openWalletTab(user, 'Savings')
    await chooseMenu(user, 'Wallet management', 'Rename wallet')
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
    await chooseMenu(user, 'Wallet management', 'Rename wallet')
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

    await user.click(screen.getByRole('tab', { name: 'Wallets' }))
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
    expect(await screen.findByRole('tab', { name: 'Wallets' })).toBeInTheDocument()
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
    expect(screen.getByRole('tab', { name: 'Wallets' })).toBeInTheDocument()
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
    fireEvent.click(screen.getByRole('button', { name: 'Wallet management' }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Reveal recovery phrase' }))
    const dialog = screen.getByRole('dialog')
    await act(async () => {
      fireEvent.click(within(dialog).getByRole('button', { name: 'Reveal recovery phrase' }))
      await vi.advanceTimersByTimeAsync(0)
    })
    expect(screen.getByText('alpha')).toBeInTheDocument()

    await advance(61_000)
    expect(screen.queryByText('alpha')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Unlock vault' })).toBeInTheDocument()
  })
})

describe('network separation', () => {
  function mainnetAccount(id: string): Account {
    return { network: 'mainnet', id, display_id: `${id}-abcde`, name: 'NONAME', named: false }
  }

  function mainnetWatcher(id: string, name: string, accounts: string[] = []): Wallet {
    return {
      ...wallet(id, name),
      network: 'mainnet',
      secret_kind: 'watchonly',
      accounts: accounts.map(mainnetAccount),
    }
  }

  beforeEach(() => installBackend({
    status: 'unlocked',
    wallets: [wallet('w1', 'Savings', '0.0.1001'), mainnetWatcher('m1', 'Watcher', ['0.0.5005'])],
  }))

  it('shows one network at a time and asks before switching to Mainnet', async () => {
    const user = userEvent.setup()
    render(<App />)
    expect(await screen.findByRole('button', { name: /Savings/ })).toBeInTheDocument()
    expect(screen.queryByText('Watcher')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Mainnet' }))
    // Nothing changes until the switch is confirmed.
    expect(screen.queryByText('Watcher')).not.toBeInTheDocument()
    await user.click(await screen.findByRole('button', { name: 'Switch to Mainnet' }))

    expect(await screen.findByRole('button', { name: /Watcher/ })).toBeInTheDocument()
    expect(screen.queryByText('Savings')).not.toBeInTheDocument()

    // Switching back to Testnet needs no confirmation.
    await user.click(screen.getByRole('button', { name: 'Testnet' }))
    expect(await screen.findByRole('button', { name: /Savings/ })).toBeInTheDocument()
    expect(screen.queryByText('Watcher')).not.toBeInTheDocument()
  })

  it('stays on Testnet when the Mainnet confirmation is cancelled', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByRole('button', { name: /Savings/ })
    await user.click(screen.getByRole('button', { name: 'Mainnet' }))
    await user.click(await screen.findByRole('button', { name: 'Cancel' }))
    expect(screen.getByRole('button', { name: /Savings/ })).toBeInTheDocument()
    expect(screen.queryByText('Watcher')).not.toBeInTheDocument()
  })

  it('offers only watch-only import on Mainnet and tags the wallet with the network', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByRole('button', { name: /Savings/ })
    await user.click(screen.getByRole('button', { name: 'Mainnet' }))
    await user.click(await screen.findByRole('button', { name: 'Switch to Mainnet' }))
    await user.click(screen.getByRole('button', { name: /Create \/ import wallet/ }))

    expect(await screen.findByRole('heading', { name: 'Add a Mainnet watch-only wallet' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Create' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Recovery phrase' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Private key' })).not.toBeInTheDocument()

    await user.type(screen.getByLabelText('Wallet name'), 'New watcher')
    scanner.reply = `HVTX1-PUB:ecdsa:${'ab'.repeat(33)}`
    await user.click(screen.getByRole('button', { name: 'Scan to create wallet' }))
    await user.click(await screen.findByRole('button', { name: 'Deliver scanned reply' }))
    await waitFor(() => {
      expect(commandsCalled('import_watch_only_wallet')[0]?.args).toMatchObject({
        network: 'mainnet',
        keyType: 'ecdsa',
        publicKey: 'ab'.repeat(33),
      })
    })
  })

  it('hides send, token and risky account actions for a Mainnet account', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByRole('button', { name: /Savings/ })
    await user.click(screen.getByRole('button', { name: 'Mainnet' }))
    await user.click(await screen.findByRole('button', { name: 'Switch to Mainnet' }))
    await openWalletTab(user, 'Watcher')
    await user.click(await screen.findByRole('button', { name: /0\.0\.5005/ }))

    expect(await screen.findByRole('button', { name: 'Account management' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Send' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Token management' })).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Account management' }))
    expect(await screen.findByRole('menuitem', { name: 'Name this account' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Delete account' })).not.toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Move account' })).not.toBeInTheDocument()
  })

  it('offers only same-network wallets as move targets', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [
        wallet('w1', 'Savings', '0.0.1001'),
        wallet('w2', 'Trading'),
        mainnetWatcher('m1', 'Watcher', ['0.0.5005']),
      ],
    })
    render(<App />)
    await openAccount(user, 'Savings', 'Move account')
    const dropdown = await screen.findByLabelText('New wallet')
    const names = within(dropdown).getAllByRole('option').map((option) => option.textContent)
    expect(names).toContain('Trading')
    expect(names).not.toContain('Watcher')
  })

  it('hides the move action when no same-network wallet exists', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openAccount(user, 'Savings')
    await user.click(screen.getByRole('button', { name: 'Account management' }))
    expect(await screen.findByRole('menuitem', { name: 'Name this account' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Move account' })).not.toBeInTheDocument()
  })

  it('keeps the Testnet and Mainnet address books separate', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [wallet('w1', 'Savings', '0.0.1001'), mainnetWatcher('m1', 'Watcher', ['0.0.5005'])],
      outside: [{ id: '0.0.7777', name: 'Landlord' }],
    })
    render(<App />)
    await screen.findByRole('button', { name: /Savings/ })
    await user.click(screen.getByRole('button', { name: 'Address book' }))

    // The Testnet book: the Testnet account and the outside entry, not the
    // Mainnet account.
    const unnamed = await screen.findByRole('list', { name: 'Unnamed accounts' })
    expect(within(unnamed).getByText('0.0.1001-abcde')).toBeInTheDocument()
    expect(within(await screen.findByRole('list', { name: 'Address book' })).getByText('Landlord'))
      .toBeInTheDocument()
    expect(screen.queryByText('0.0.5005-abcde')).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Add outside account' })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Mainnet' }))
    await user.click(await screen.findByRole('button', { name: 'Switch to Mainnet' }))
    await user.click(screen.getByRole('button', { name: 'Address book' }))

    // The Mainnet book: only the Mainnet account. Outside entries are
    // Testnet-only, so there is nothing to add here.
    const mainnetUnnamed = await screen.findByRole('list', { name: 'Unnamed accounts' })
    expect(within(mainnetUnnamed).getByText('0.0.5005-abcde')).toBeInTheDocument()
    expect(screen.queryByText('0.0.1001-abcde')).not.toBeInTheDocument()
    expect(screen.queryByText('Landlord')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Add outside account' })).not.toBeInTheDocument()
  })
})

describe('deleted tokens', () => {
  beforeEach(() => installBackend({
    status: 'unlocked',
    wallets: [wallet('w1', 'Savings', '0.0.1001')],
    overview: {
      account_id: 'x',
      balance_tinybars: 150_000_000,
      transactions: [],
      tokens: [
        { token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 },
        { token_id: '0.0.9', symbol: 'TGBP', name: 'Sterling', balance: '74851', decimals: 0, deleted: true },
      ],
    },
  }))

  it('shows deleted tokens in assets but keeps them out of the Send menu', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openAccount(user, 'Savings')
    const assets = await screen.findByRole('list', { name: 'Account assets' })
    expect(within(assets).getByText('(deleted)')).toBeInTheDocument()
    expect(within(assets).getByText('74,851')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Send' }))
    expect(await screen.findByRole('menuitem', { name: /ALP/ })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: /TGBP/ })).not.toBeInTheDocument()
  })

  it('lets a deleted token be dissociated whatever its balance', async () => {
    const user = userEvent.setup()
    render(<App />)
    await openAccount(user, 'Savings')
    await screen.findByRole('list', { name: 'Account assets' })
    await chooseMenu(user, 'Token management', 'Disassociate token')
    const select = await screen.findByLabelText('Token to remove')
    expect(within(select).getByRole('option', { name: /TGBP.*deleted token/ })).toBeEnabled()
    expect(within(select).getByRole('option', { name: /ALP.*must be 0/ })).toBeDisabled()
  })
})

describe('NFT support', () => {
  const nftOverview = {
    account_id: 'x',
    balance_tinybars: 150_000_000,
    transactions: [],
    tokens: [
      { token_id: '0.0.8', symbol: 'ALP', name: 'Alpha', balance: '12345', decimals: 2 },
      { token_id: '0.0.21', symbol: 'ART', name: 'Art', balance: '2', decimals: 0, is_nft: true },
    ],
    nfts: [
      { token_id: '0.0.21', serial_number: 1, metadata: 'ipfs://one' },
      { token_id: '0.0.21', serial_number: 2, metadata: '' },
    ],
  }
  const nftTokenInfo = {
    token_id: '0.0.21',
    name: 'Art',
    symbol: 'ART',
    decimals: 0,
    token_type: 'NON_FUNGIBLE_UNIQUE',
    total_supply: '2',
  }

  it('lists owned NFTs by token ID and serial number and sends one after review', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading', '0.0.1002')],
      overview: nftOverview,
      tokenInfo: nftTokenInfo,
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    const rows = within(screen.getByRole('list', { name: 'Account assets' })).getAllByRole('listitem')
    expect(rows.map((row) => row.textContent)).toEqual([
      'HBAR1.50 ℏ',
      'ALP Alpha0.0.8123.45',
      'ART #1 ipfs://one0.0.21NFT',
      'ART #20.0.21NFT',
    ])
    await user.click(screen.getByRole('button', { name: 'Send' }))
    expect(screen.getAllByRole('menuitem').map((item) => item.textContent)).toEqual([
      'HBAR',
      'ALP123.45',
      'ART #10.0.21',
      'ART #20.0.21',
    ])
    await user.click(screen.getByRole('menuitem', { name: /ART #2/ }))
    expect(screen.getByRole('heading', { name: 'Send ART #2' })).toBeInTheDocument()
    // An NFT send has no amount field; the serial number is fixed.
    expect(screen.queryByLabelText('Amount')).not.toBeInTheDocument()
    await user.selectOptions(screen.getByLabelText('Recipient from the address book'), '0.0.1002-abcde')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    const review = await screen.findByRole('group', { name: 'Confirm transfer' })
    expect(review).toHaveTextContent('Serial number2')
    expect(review).toHaveTextContent('0.0.21')
    // No custom fees, so no warning.
    expect(screen.queryByText(/charges custom fees/)).not.toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Sign and send' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('send_nft')[0].args).toMatchObject({
      walletId: 'w1',
      fromAccount: '0.0.1001',
      tokenId: '0.0.21',
      serialNumber: '2',
    })
    expect(commandsCalled('send_token')).toHaveLength(0)
    expect(commandsCalled('send_hbar')).toHaveLength(0)
  })

  it('shows the fee schedule and a receiver warning before sending an NFT with custom fees', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001'), wallet('w2', 'Trading', '0.0.1002')],
      overview: nftOverview,
      tokenInfo: {
        ...nftTokenInfo,
        custom_fees: {
          fixed_fees: [],
          royalty_fees: [
            {
              numerator: '5',
              denominator: '100',
              collector: '0.0.77',
              fallback: { amount: '100', denominating_token_id: '', collector: '' },
            },
          ],
          has_other_fees: false,
        },
      },
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    await user.click(screen.getByRole('menuitem', { name: /ART #1/ }))
    await user.selectOptions(screen.getByLabelText('Recipient from the address book'), '0.0.1002-abcde')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await screen.findByRole('group', { name: 'Confirm transfer' })
    expect(screen.getByText(/charges custom fees/)).toBeInTheDocument()
    const schedule = screen.getByRole('list', { name: 'Fee schedule' })
    expect(schedule).toHaveTextContent('Royalty: 5/100')
    expect(schedule).toHaveTextContent('charged to the RECEIVER')
    // A send rejected by the backend policy shows its error.
    backend.sendError = 'Sending it is disabled. You can allow such transfers in Settings.'
    await user.click(screen.getByRole('button', { name: 'Sign and send' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('allow such transfers in Settings')
  })

  it('changes the NFT custom fee setting from Settings', async () => {
    const user = userEvent.setup()
    installBackend({ allowNftCustomFees: false })
    render(<App />)
    await unlock(user)
    await user.click(screen.getByRole('button', { name: 'Settings' }))
    await user.click(await screen.findByLabelText(/NFTs whose token charges custom fees/))
    await waitFor(() => expect(backend.allowNftCustomFees).toBe(true))
  })

  it('sends an NFT from a watch-only wallet through a signing request', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [
        { ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' },
        wallet('w2', 'Trading', '0.0.1002'),
      ],
      overview: nftOverview,
      tokenInfo: nftTokenInfo,
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Watcher')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Send' }))
    await user.click(screen.getByRole('menuitem', { name: /ART #1/ }))
    await user.selectOptions(screen.getByLabelText('Recipient from the address book'), '0.0.1002-abcde')
    await user.click(screen.getByRole('button', { name: 'Review transfer' }))
    await user.click(await screen.findByRole('button', { name: 'Create signing request' }))
    const request = await screen.findByRole('group', { name: 'Signing request' })
    expect(request).toHaveTextContent('Serial number1')
    expect(request).toHaveTextContent('0.0.21')
    expect(commandsCalled('prepare_unsigned_nft_transfer')[0].args).toMatchObject({
      walletId: 'w1',
      fromAccount: '0.0.1001',
      tokenId: '0.0.21',
      serialNumber: '1',
    })
    expect(commandsCalled('send_nft')).toHaveLength(0)
    await user.type(screen.getByLabelText('Signature from the signer'), 'HVTX1-SIG:AB')
    await user.click(screen.getByRole('button', { name: 'Submit signed transfer' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('submit_signed_request')).toHaveLength(1)
  })

  it('creates an NFT collection and mints a serial after review', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: nftOverview,
      tokenInfo: nftTokenInfo,
      nftSupplyTokens: ['0.0.21'],
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await chooseMenu(user, 'Token management', 'Create NFT collection')
    await user.click(screen.getByRole('button', { name: 'Review collection' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Enter a collection name and symbol.')
    await user.type(screen.getByLabelText('Collection name'), 'Art')
    await user.type(screen.getByLabelText('Symbol'), 'ART')
    await user.click(screen.getByRole('button', { name: 'Review collection' }))
    const review = screen.getByRole('group', { name: 'Confirm NFT collection creation' })
    expect(review).toHaveTextContent('NFT (non-fungible)')
    expect(review).toHaveTextContent('supply key')
    await user.click(screen.getByRole('button', { name: 'Sign and create collection' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('create_nft_collection')[0].args).toMatchObject({
      walletId: 'w1',
      fromAccount: '0.0.1001',
      name: 'Art',
      symbol: 'ART',
    })

    await chooseMenu(user, 'Token management', 'Mint NFT')
    await user.click(screen.getByRole('button', { name: 'Sign and mint NFT' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Choose a collection.')
    await user.selectOptions(screen.getByLabelText('Collection'), '0.0.21')
    await user.click(screen.getByRole('button', { name: 'Sign and mint NFT' }))
    expect(screen.getByRole('alert')).toHaveTextContent('Enter the NFT metadata')
    await user.type(screen.getByLabelText(/Metadata/), 'ipfs://three')
    await user.click(screen.getByRole('button', { name: 'Sign and mint NFT' }))
    await screen.findByText('SUCCESS!')
    expect(commandsCalled('mint_nft')[0].args).toMatchObject({
      walletId: 'w1',
      accountId: '0.0.1001',
      tokenId: '0.0.21',
      metadata: 'ipfs://three',
    })
  })

  it('disables NFT creation and minting for watch-only wallets', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [{ ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' }],
      overview: nftOverview,
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Watcher')
    await screen.findByRole('button', { name: 'Refresh transactions' })
    await user.click(screen.getByRole('button', { name: 'Token management' }))
    expect(screen.getByRole('menuitem', { name: /Create NFT collection/ })).toBeDisabled()
    expect(screen.getByRole('menuitem', { name: /Mint NFT/ })).toBeDisabled()
  })

  it('offers NFT tokens for dissociation only once no serials are owned', async () => {
    const user = userEvent.setup()
    installBackend({
      wallets: [wallet('w1', 'Savings', '0.0.1001')],
      overview: {
        ...nftOverview,
        tokens: [
          ...nftOverview.tokens,
          { token_id: '0.0.22', symbol: 'GONE', name: 'Gone', balance: '0', decimals: 0, is_nft: true },
        ],
      },
      tokenInfo: nftTokenInfo,
    })
    render(<App />)
    await unlock(user)
    await openAccount(user, 'Savings')
    await chooseMenu(user, 'Token management', 'Disassociate token')
    const owned = screen.getByRole('option', { name: /ART \(0\.0\.21\)/ }) as HTMLOptionElement
    expect(owned.disabled).toBe(true)
    expect(owned.textContent).toContain('owns 2 NFT(s), must be 0')
    const empty = screen.getByRole('option', { name: /GONE \(0\.0\.22\)/ }) as HTMLOptionElement
    expect(empty.disabled).toBe(false)
    expect(empty.textContent).toContain('NFT token')
  })
})

describe('start over from the unlock screen', () => {
  beforeEach(() => installBackend())

  it('archives the vault after acknowledgment and returns to first run', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByLabelText('Vault password')
    await user.click(screen.getByRole('button', { name: 'Forgotten the vault password? Start over' }))

    const confirm = await screen.findByRole('button', { name: 'Archive vault and start over' })
    expect(confirm).toBeDisabled()
    await user.click(screen.getByLabelText(/I understand the current vault/))
    await user.click(confirm)

    expect(await screen.findByRole('button', { name: 'Create vault' })).toBeInTheDocument()
    expect(commandsCalled('start_over_vault')).toHaveLength(1)
  })

  it('does nothing when cancelled', async () => {
    const user = userEvent.setup()
    render(<App />)
    await screen.findByLabelText('Vault password')
    await user.click(screen.getByRole('button', { name: 'Forgotten the vault password? Start over' }))
    await user.click(await screen.findByRole('button', { name: 'Cancel' }))
    expect(screen.getByLabelText('Vault password')).toBeInTheDocument()
    expect(commandsCalled('start_over_vault')).toHaveLength(0)
  })
})

describe('address book pruning', () => {
  it('removes outside entries whose accounts were deleted on the network', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      outside: [
        { id: '0.0.7777', name: 'Gone' },
        { id: '0.0.8888', name: 'Alive' },
      ],
      deletedOnNetwork: ['0.0.7777'],
    })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Address book' }))

    expect(await screen.findByText('Removed 1 account deleted on the network.')).toBeInTheDocument()
    expect(screen.queryByText('Gone')).not.toBeInTheDocument()
    expect(screen.getByText('Alive')).toBeInTheDocument()
  })

  it('unlinks wallet accounts deleted on the network and refreshes the wallets', async () => {
    const user = userEvent.setup()
    installBackend({
      status: 'unlocked',
      wallets: [wallet('w1', 'Savings', ['0.0.1001', '0.0.1002'])],
      deletedOnNetwork: ['0.0.1002'],
    })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Address book' }))

    expect(await screen.findByText('Removed 1 account deleted on the network.')).toBeInTheDocument()
    expect(screen.queryByText(/0\.0\.1002/)).not.toBeInTheDocument()
    expect(screen.getByText(/0\.0\.1001/)).toBeInTheDocument()
    // The wallets themselves are refreshed from the backend.
    await waitFor(() => expect(commandsCalled('get_vault_state').length).toBeGreaterThan(1))
  })
})

describe('offline key rotation', () => {
  function installMoveBackend(targetWatchOnly = true) {
    installBackend({
      status: 'unlocked',
      wallets: [
        { ...wallet('w1', 'Watcher', '0.0.1001'), secret_kind: 'watchonly', key_type: 'ed25519' },
        targetWatchOnly
          ? { ...wallet('w2', 'Cold'), secret_kind: 'watchonly', key_type: 'ed25519' }
          : wallet('w2', 'Hot'),
      ],
    })
  }

  async function openSigningScreen(user: ReturnType<typeof userEvent.setup>) {
    render(<App />)
    await openAccount(user, 'Watcher')
    await chooseMenu(user, 'Account management', 'Move account')
    await user.selectOptions(screen.getByLabelText('New wallet'), 'w2')
    await user.click(screen.getByRole('button', { name: 'Review move' }))
    await user.click(screen.getByLabelText(/I understand that the other wallet/))
    await user.click(screen.getByRole('button', { name: 'Create signing request' }))
    return screen.findByRole('group', { name: 'Signing request' })
  }

  async function scan(user: ReturnType<typeof userEvent.setup>, reply: string) {
    scanner.reply = reply
    await user.click(screen.getByRole('button', { name: 'Scan signatures' }))
    await user.click(await screen.findByRole('button', { name: 'Deliver scanned reply' }))
  }

  it('moves a watch-only account after scanning signatures from both keys', async () => {
    const user = userEvent.setup()
    installMoveBackend()
    const request = await openSigningScreen(user)

    // The account and both keys (abbreviated) are shown without fee details.
    expect(request).toHaveTextContent('This account')
    expect(request).toHaveTextContent('0.0.1001-abcde')
    expect(request).toHaveTextContent('aba.....bab')
    expect(request).toHaveTextContent('cdc.....dcd')
    expect(request).not.toHaveTextContent('network fee')
    expect(commandsCalled('prepare_unsigned_move')[0].args).toEqual({
      walletId: 'w1',
      accountId: '0.0.1001',
      targetWalletId: 'w2',
    })

    // The target is watch-only, so both keys must sign before submitting.
    const currentBox = screen.getByRole('checkbox', { name: "This account's key signature" })
    const newBox = screen.getByRole('checkbox', { name: "Receiving wallet's key signature" })
    const submit = screen.getByRole('button', { name: 'Submit signed move' })
    expect(currentBox).not.toBeChecked()
    expect(newBox).not.toBeChecked()
    expect(submit).toBeDisabled()

    await scan(user, 'HVTX1-SIG:CURRENT')
    await waitFor(() => expect(currentBox).toBeChecked())
    expect(newBox).not.toBeChecked()
    expect(submit).toBeDisabled()

    await scan(user, 'HVTX1-SIG:NEW')
    await waitFor(() => expect(newBox).toBeChecked())
    expect(screen.getByRole('button', { name: 'Scan signatures' })).toBeDisabled()
    expect(submit).toBeEnabled()

    await user.click(submit)
    await waitFor(() => expect(commandsCalled('submit_signed_move')[0].args).toMatchObject({
      request: 'HVTX1-REQ:05',
      responses: ['HVTX1-SIG:CURRENT', 'HVTX1-SIG:NEW'],
      walletId: 'w1',
      targetWalletId: 'w2',
    }))
    expect(commandsCalled('move_account')).toHaveLength(0)
  })

  it('needs only the current key when the target wallet holds its key', async () => {
    const user = userEvent.setup()
    installMoveBackend(false)
    await openSigningScreen(user)

    expect(
      screen.queryByRole('checkbox', { name: "Receiving wallet's key signature" }),
    ).not.toBeInTheDocument()
    await scan(user, 'HVTX1-SIG:CURRENT')
    await waitFor(() =>
      expect(screen.getByRole('checkbox', { name: "This account's key signature" })).toBeChecked())
    await user.click(screen.getByRole('button', { name: 'Submit signed move' }))
    await waitFor(() => expect(commandsCalled('submit_signed_move')[0].args).toMatchObject({
      responses: ['HVTX1-SIG:CURRENT'],
      targetWalletId: 'w2',
    }))
  })

  it('rejects a signature from a wrong key and clears the error on the next click', async () => {
    const user = userEvent.setup()
    installMoveBackend()
    await openSigningScreen(user)

    await scan(user, 'HVTX1-SIG:OTHER')
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'The signature was made with a key that is not part of this move.',
    )
    expect(screen.getByRole('checkbox', { name: "This account's key signature" })).not.toBeChecked()

    // A click anywhere in the app closes the error.
    await user.click(screen.getByRole('heading', { name: 'Move account to another wallet' }))
    await waitFor(() => expect(screen.queryByRole('alert')).not.toBeInTheDocument())
  })

  it('rejects the same signature scanned twice', async () => {
    const user = userEvent.setup()
    installMoveBackend()
    await openSigningScreen(user)

    await scan(user, 'HVTX1-SIG:CURRENT')
    const currentBox = screen.getByRole('checkbox', { name: "This account's key signature" })
    await waitFor(() => expect(currentBox).toBeChecked())

    await scan(user, 'HVTX1-SIG:CURRENT')
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This signature was already submitted.',
    )
    expect(currentBox).toBeChecked()
    expect(screen.getByRole('button', { name: 'Submit signed move' })).toBeDisabled()
  })

  it('returns to the review with Back and leaves the screen with Cancel', async () => {
    const user = userEvent.setup()
    installMoveBackend()
    await openSigningScreen(user)

    await user.click(screen.getByRole('button', { name: 'Back' }))
    expect(screen.getByRole('group', { name: 'Confirm account move' })).toBeInTheDocument()

    await user.click(screen.getByLabelText(/I understand that the other wallet/))
    await user.click(screen.getByRole('button', { name: 'Create signing request' }))
    await screen.findByRole('group', { name: 'Signing request' })
    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(
      screen.queryByRole('heading', { name: 'Move account to another wallet' }),
    ).not.toBeInTheDocument()
  })

  it('opens the camera over the signing box and closes it on an outside click', async () => {
    const user = userEvent.setup()
    installMoveBackend()
    await openSigningScreen(user)

    await user.click(screen.getByRole('button', { name: 'Scan signatures' }))
    expect(await screen.findByText(/does not allow camera access/)).toBeInTheDocument()

    // A click anywhere outside the camera window closes it.
    await user.click(screen.getByRole('heading', { name: 'Move account to another wallet' }))
    await waitFor(() =>
      expect(screen.queryByText(/does not allow camera access/)).not.toBeInTheDocument())
    expect(screen.getByRole('button', { name: 'Scan signatures' })).toBeEnabled()
  })
})

describe('shared accounts (multisig)', () => {
  it('links a shared account after showing its key structure in full', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked' })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Link a shared account' }))
    await user.type(screen.getByLabelText('Account ID'), '0.0.5005')
    await user.click(screen.getByRole('button', { name: 'Fetch key structure' }))

    // Every key is shown before the user confirms the link.
    expect(await screen.findByText(/controlled by 2 of 3 keys/)).toBeInTheDocument()
    const structure = screen.getByLabelText('Key structure')
    expect(structure).toHaveTextContent('2 of 3:')
    expect(structure).toHaveTextContent('aa'.repeat(32))
    expect(structure).toHaveTextContent('cc'.repeat(32))

    await user.click(screen.getByRole('button', { name: 'Link this account' }))
    expect(await screen.findByText(/Linked shared account 0\.0\.5005-abcde/)).toBeInTheDocument()
    expect(commandsCalled('add_shared_account')[0].args).toMatchObject({ accountId: '0.0.5005' })
    expect(screen.getByText('2 of 3 keys')).toBeInTheDocument()
  })

  it('sends HBAR only once the threshold is satisfied', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', sharedAccounts: [sharedAccountView('0.0.5005')] })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Send HBAR' }))
    await user.type(screen.getByLabelText('Recipient account ID'), '0.0.2002-abcde')
    await user.type(screen.getByLabelText('Amount (HBAR)'), '1.5')
    await user.click(screen.getByRole('button', { name: 'Create signing request' }))

    await screen.findByRole('group', { name: 'Shared signing request' })
    expect(screen.queryByText(/network fee/i)).not.toBeInTheDocument()
    const submit = screen.getByRole('button', { name: 'Submit' })
    expect(submit).toBeDisabled()

    // The first reply is below the threshold; submit stays disabled.
    await user.type(screen.getByLabelText('Signer reply'), 'HVTX1-SIG:AA')
    await user.click(screen.getByRole('button', { name: 'Add signature' }))
    expect(
      await screen.findByText(/1 reply collected; 1 of the required 2 keys have signed/),
    ).toBeInTheDocument()
    expect(submit).toBeDisabled()

    // The second reply satisfies 2-of-3 and the transfer can be submitted.
    await user.type(screen.getByLabelText('Signer reply'), 'HVTX1-SIG:BB')
    await user.click(screen.getByRole('button', { name: 'Add signature' }))
    expect(
      await screen.findByText('The threshold is satisfied. The transfer can be submitted.'),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Submit' }))
    await waitFor(() =>
      expect(commandsCalled('submit_shared_transfer')[0].args).toMatchObject({
        request: 'HVTX1-REQ:10',
        responses: ['HVTX1-SIG:AA', 'HVTX1-SIG:BB'],
      }))
    expect(await screen.findByText(/Transfer submitted/)).toBeInTheDocument()
  })

  it('refuses an invalid reply and does not keep it', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', sharedAccounts: [sharedAccountView('0.0.5005')] })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Send HBAR' }))
    await user.type(screen.getByLabelText('Recipient account ID'), '0.0.2002-abcde')
    await user.type(screen.getByLabelText('Amount (HBAR)'), '1')
    await user.click(screen.getByRole('button', { name: 'Create signing request' }))
    await screen.findByRole('group', { name: 'Shared signing request' })

    await user.type(screen.getByLabelText('Signer reply'), 'HVTX1-SIG:BAD')
    await user.click(screen.getByRole('button', { name: 'Add signature' }))
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'This is not a valid signature from a signer.',
    )
    expect(screen.queryByText(/reply collected|replies collected/)).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Submit' })).toBeDisabled()
  })

  it('creates a shared account funded by a hot-wallet account', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Create a shared account' }))
    await user.selectOptions(
      screen.getByLabelText('Funding account (signs and pays)'),
      '0.0.1001',
    )
    await user.type(screen.getByLabelText('Opening balance (HBAR)'), '10')
    await user.type(
      screen.getByLabelText('Participant public keys (one per line)'),
      `${'aa'.repeat(32)}\n${'bb'.repeat(32)}\n${'cc'.repeat(32)}`,
    )
    await user.click(screen.getByRole('button', { name: 'Create account' }))

    await waitFor(() =>
      expect(commandsCalled('create_shared_account')[0].args).toMatchObject({
        fromAccount: '0.0.1001',
        initialHbar: '10',
        required: 2,
        keys: ['aa'.repeat(32), 'bb'.repeat(32), 'cc'.repeat(32)],
      }))
    expect(await screen.findByText(/Created shared account 0\.0\.9001-abcde/)).toBeInTheDocument()
    expect(commandsCalled('finish_shared_create')).toHaveLength(1)
    expect(screen.getByText('2 of 3 keys')).toBeInTheDocument()
  })

  it.each(['create_shared_account', 'finish_shared_create'])(
    'surfaces a fee failure from %s without retrying or leaving stale progress',
    async (failureCommand) => {
      const user = userEvent.setup()
      installBackend({ status: 'unlocked', wallets: [wallet('w1', 'Savings', '0.0.1001')] })
      const original = invokeMock.getMockImplementation()
      if (!original) throw new Error('Backend mock is missing')
      let rejectCreation: (error: string) => void = () => {
        throw new Error('Creation has not started')
      }
      invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
        cmd === failureCommand
          ? new Promise((_, reject) => { rejectCreation = reject })
          : original(cmd, args),
      )
      render(<App />)
      await user.click(await screen.findByRole('button', { name: 'Create a shared account' }))
      await user.selectOptions(screen.getByLabelText('Funding account (signs and pays)'), '0.0.1001')
      await user.type(screen.getByLabelText('Opening balance (HBAR)'), '20')
      await user.type(screen.getByLabelText('Participant public keys (one per line)'), SHARED_KEYS.join('\n'))
      await user.click(screen.getByRole('button', { name: 'Create account' }))
      expect(screen.getByRole('status')).toHaveTextContent(
        failureCommand === 'create_shared_account'
          ? 'Submitting account creation to Testnet'
          : 'Submitted. Waiting for Testnet to confirm the new account',
      )
      expect(screen.getByRole('button', { name: 'Working...' })).toBeDisabled()
      await act(async () => {
        rejectCreation(
          failureCommand === 'create_shared_account'
            ? 'INSUFFICIENT_TX_FEE. The network quote no longer covers the required fee.'
            : 'The account creation failed: INSUFFICIENT_TX_FEE',
        )
      })
      expect(await screen.findByRole('alert')).toHaveTextContent('INSUFFICIENT_TX_FEE')
      expect(screen.queryByRole('status')).not.toBeInTheDocument()
      expect(screen.getByRole('button', { name: 'Create account' })).toBeEnabled()
      expect(commandsCalled('finish_shared_create')).toHaveLength(0)
      expect(invokeMock.mock.calls.filter(([cmd]) => cmd === failureCommand)).toHaveLength(1)
    },
  )

  it('polls a pending shared creation without submitting another transaction', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    const original = invokeMock.getMockImplementation()
    if (!original) throw new Error('Backend mock is missing')
    let confirmations = 0
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'finish_shared_create' && ++confirmations === 1) {
        throw new Error('Testnet has not confirmed the creation yet.')
      }
      return original(cmd, args)
    })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Create a shared account' }))
    await user.selectOptions(screen.getByLabelText('Funding account (signs and pays)'), '0.0.1001')
    await user.type(screen.getByLabelText('Opening balance (HBAR)'), '20')
    await user.type(screen.getByLabelText('Participant public keys (one per line)'), SHARED_KEYS.join('\n'))
    vi.useFakeTimers()
    try {
      await act(async () => {
        fireEvent.submit(screen.getByRole('form', { name: 'Create a shared account' }))
      })
      expect(confirmations).toBe(1)
      expect(screen.getByRole('status')).toHaveTextContent('Waiting for Testnet to confirm')
      await act(async () => { await vi.advanceTimersByTimeAsync(3000) })
      expect(confirmations).toBe(2)
      expect(screen.getByRole('status')).toHaveTextContent('Created shared account')
      expect(commandsCalled('create_shared_account')).toHaveLength(1)
    } finally {
      vi.useRealTimers()
    }
  })

  it('converts a wallet account into a shared account', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', wallets: [wallet('w1', 'Savings', '0.0.1001')] })
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Convert an account' }))
    await user.selectOptions(screen.getByLabelText('Account to convert'), 'w1/0.0.1001')
    await user.type(
      screen.getByLabelText('Participant public keys (one per line)'),
      `${'aa'.repeat(32)}\n${'bb'.repeat(32)}\n${'cc'.repeat(32)}`,
    )
    await user.click(screen.getByRole('button', { name: 'Review key change' }))

    // The whole new key structure is shown before anything is signed.
    await screen.findByRole('group', { name: 'Shared signing request' })
    expect(commandsCalled('prepare_shared_key_change')[0].args).toMatchObject({
      walletId: 'w1',
      accountId: '0.0.1001',
      required: 2,
    })
    const structure = screen.getByLabelText('Key structure')
    expect(structure).toHaveTextContent('2 of 3:')
    expect(structure).toHaveTextContent('bb'.repeat(32))

    // The vault holds the current key, so it can submit without replies here.
    await user.click(screen.getByRole('button', { name: 'Submit' }))
    await waitFor(() =>
      expect(commandsCalled('submit_shared_key_change')[0].args).toMatchObject({
        walletId: 'w1',
        request: 'HVTX1-REQ:11',
        responses: [],
      }))
    expect(await screen.findByText(/is now a shared account/)).toBeInTheDocument()
    expect(commandsCalled('finish_shared_convert')[0].args).toMatchObject({
      walletId: 'w1',
      accountId: '0.0.1001',
    })
    expect(screen.getByText('2 of 3 keys')).toBeInTheDocument()
  })

  it('forgets a shared account without touching the network', async () => {
    const user = userEvent.setup()
    installBackend({ status: 'unlocked', sharedAccounts: [sharedAccountView('0.0.5005', 'Ours')] })
    render(<App />)
    expect(await screen.findByText(/Ours - 0\.0\.5005-abcde/)).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Forget' }))
    expect(await screen.findByText(/Stopped watching 0\.0\.5005-abcde/)).toBeInTheDocument()
    expect(screen.queryByText(/Ours - 0\.0\.5005-abcde/)).not.toBeInTheDocument()
    expect(commandsCalled('remove_shared_account')[0].args).toMatchObject({
      accountId: '0.0.5005',
    })
  })
})

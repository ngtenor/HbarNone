export type WalletAccount = {
  network: 'testnet' | 'mainnet'
  id: string
  display_id: string
  name: string
  named: boolean
}

export type AddressBookRow = {
  network: 'testnet' | 'mainnet'
  id: string
  display_id: string
  name: string
  wallet_id: string | null
  wallet_name: string | null
}

export type AddressBookView = {
  rows: AddressBookRow[]
  allowExternalSends: boolean
  allowExternalDeletes: boolean
}

export type WalletSummary = {
  id: string
  name: string
  created_at: string
  accounts: WalletAccount[]
  secret_kind: 'mnemonic' | 'privatekey'
  key_type: 'ed25519' | 'ecdsa'
  custody?: 'hot' | 'cold'
  signing?: 'single' | 'multisig'
}

export type WalletKeys = {
  publicKey: string
  evmAddress: string | null
}

export type KeyDetection = {
  keyType: 'ed25519' | 'ecdsa' | null
  accounts: string[]
}

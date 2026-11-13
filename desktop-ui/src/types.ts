export type WalletAccount = {
  network: 'testnet' | 'mainnet'
  id: string
  display_id: string
  name: string
  named: boolean
  signing?: 'single' | 'multisig'
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
  network: 'testnet' | 'mainnet'
  accounts: WalletAccount[]
  secret_kind: 'mnemonic' | 'privatekey' | 'watchonly'
  key_type: 'ed25519' | 'ecdsa'
  multisig_policy?: { threshold: number; keys: string[] } | null
  custody?: 'hot' | 'cold'
}

export type WalletKeys = {
  publicKey: string
  evmAddress: string | null
}

export type KeyDetection = {
  keyType: 'ed25519' | 'ecdsa' | null
  accounts: string[]
}

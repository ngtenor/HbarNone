export type WalletSummary = {
  id: string
  name: string
  created_at: string
  account_id: string | null
  secret_kind: 'mnemonic' | 'privatekey'
  key_type: 'ed25519' | 'ecdsa'
}

export type WalletKeys = {
  publicKey: string
  evmAddress: string | null
}

export type KeyDetection = {
  keyType: 'ed25519' | 'ecdsa' | null
  accounts: string[]
}

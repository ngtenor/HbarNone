import { useEffect, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import './App.css'

type WalletSummary = {
  id: string
  name: string
  created_at: string
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function App() {
  const [wallets, setWallets] = useState<WalletSummary[]>([])
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [isLoading, setIsLoading] = useState(true)
  const [isCreating, setIsCreating] = useState(false)

  useEffect(() => {
    let cancelled = false

    void invoke<WalletSummary[]>('list_wallets')
      .then((loadedWallets) => {
        if (!cancelled) setWallets(loadedWallets)
      })
      .catch((loadError: unknown) => {
        if (!cancelled) setError(errorMessage(loadError))
      })
      .finally(() => {
        if (!cancelled) setIsLoading(false)
      })

    return () => {
      cancelled = true
    }
  }, [])

  async function handleCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError('')
    setIsCreating(true)
    try {
      const wallet = await invoke<WalletSummary>('create_wallet', {
        name,
        password,
      })
      setWallets((existing) => [...existing, wallet])
      setName('')
      setPassword('')
    } catch (createError) {
      setError(errorMessage(createError))
    } finally {
      setIsCreating(false)
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="/" aria-label="HederaVault home">
          <span className="brand-mark" aria-hidden="true">H</span>
          <span>HederaVault</span>
        </a>
        <span className="network-label">
          <span className="status-dot" aria-hidden="true" />
          Local vault
        </span>
      </header>

      <section className="welcome">
        <p className="eyebrow">SELF-CUSTODY STARTS HERE</p>
        <h1>Your keys.<br />Your responsibility.</h1>
        <p className="welcome-copy">
          Create an encrypted wallet to get started. Your recovery phrase stays
          in the local vault and is never sent to the interface.
        </p>
      </section>

      <section className="content-grid" aria-label="Wallet management">
        <form className="panel create-panel" onSubmit={handleCreate}>
          <div className="panel-heading">
            <div>
              <p className="eyebrow">NEW WALLET</p>
              <h2>Create a wallet</h2>
            </div>
            <span className="step-number" aria-hidden="true">01</span>
          </div>

          <label htmlFor="wallet-name">Wallet name</label>
          <input
            id="wallet-name"
            autoComplete="off"
            maxLength={64}
            placeholder="e.g. Personal"
            required
            value={name}
            onChange={(event) => setName(event.target.value)}
          />

          <label htmlFor="wallet-password">Vault password</label>
          <input
            id="wallet-password"
            type="password"
            autoComplete="new-password"
            maxLength={1024}
            required
            value={password}
            onChange={(event) => setPassword(event.target.value)}
          />
          <p className="field-hint">
            This password encrypts your wallet on this device.
          </p>

          <button className="primary-button" type="submit" disabled={isCreating}>
            {isCreating ? 'Creating securely...' : 'Create encrypted wallet'}
            {!isCreating && <span aria-hidden="true">-&gt;</span>}
          </button>
        </form>

        <section className="panel wallet-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">ON THIS DEVICE</p>
              <h2>Your wallets</h2>
            </div>
            <span className="wallet-count">{wallets.length}</span>
          </div>

          {error && (
            <p className="error-message" role="alert">{error}</p>
          )}

          {isLoading ? (
            <p className="empty-state">Loading your local vault...</p>
          ) : wallets.length === 0 ? (
            <p className="empty-state">
              No wallets yet. Create one to begin.
            </p>
          ) : (
            <ul className="wallet-list">
              {wallets.map((wallet) => (
                <li className="wallet-item" key={wallet.id}>
                  <span className="wallet-icon" aria-hidden="true">H</span>
                  <span className="wallet-details">
                    <strong>{wallet.name}</strong>
                    <span>Added {new Date(wallet.created_at).toLocaleDateString()}</span>
                  </span>
                  <span className="encrypted-badge">ENCRYPTED</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </section>

      <footer className="security-note">
        <span aria-hidden="true">!</span>
        Wallet backup is not available yet. Development only; do not use real funds.
      </footer>
    </main>
  )
}

export default App

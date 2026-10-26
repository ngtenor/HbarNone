import { useEffect, useRef, useState } from 'react'
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
  const [walletToReveal, setWalletToReveal] = useState<WalletSummary | null>(null)
  const [revealPassword, setRevealPassword] = useState('')
  const [mnemonic, setMnemonic] = useState('')
  const [revealError, setRevealError] = useState('')
  const [isRevealing, setIsRevealing] = useState(false)
  const revealRequestId = useRef(0)

  function closeReveal() {
    revealRequestId.current += 1
    setWalletToReveal(null)
    setRevealPassword('')
    setMnemonic('')
    setRevealError('')
    setIsRevealing(false)
  }

  useEffect(() => {
    const hideOnBlur = () => closeReveal()
    const hideWhenBackgrounded = () => {
      if (document.visibilityState !== 'visible') closeReveal()
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeReveal()
    }

    window.addEventListener('blur', hideOnBlur)
    document.addEventListener('visibilitychange', hideWhenBackgrounded)
    window.addEventListener('keydown', closeOnEscape)

    return () => {
      window.removeEventListener('blur', hideOnBlur)
      document.removeEventListener('visibilitychange', hideWhenBackgrounded)
      window.removeEventListener('keydown', closeOnEscape)
    }
  }, [])

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

  async function handleReveal(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!walletToReveal) return

    setRevealError('')
    setIsRevealing(true)
    const requestId = ++revealRequestId.current
    try {
      const recoveredMnemonic = await invoke<string>('reveal_wallet_mnemonic', {
        walletId: walletToReveal.id,
        password: revealPassword,
      })
      if (requestId === revealRequestId.current) {
        setMnemonic(recoveredMnemonic)
        setRevealPassword('')
      }
    } catch (revealFailure) {
      if (requestId === revealRequestId.current) {
        setRevealError(errorMessage(revealFailure))
        setRevealPassword('')
      }
    } finally {
      if (requestId === revealRequestId.current) setIsRevealing(false)
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
          Create an encrypted wallet to get started. The recovery phrase stays
          encrypted unless you explicitly reveal it with your password.
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
            Use a long, unique password and store it safely. Password recovery
            is not available if you forget it.
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
                  <button
                    className="reveal-button"
                    type="button"
                    onClick={() => {
                      closeReveal()
                      setWalletToReveal(wallet)
                    }}
                  >
                    Show recovery phrase
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      </section>

      <footer className="security-note">
        <span aria-hidden="true">!</span>
        A displayed phrase may be captured by malware or screenshots. Development only; do not use real funds.
      </footer>

      {walletToReveal && (
        <div
          className="modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeReveal()
          }}
        >
          <section
            className="recovery-dialog"
            role="dialog"
            aria-modal="true"
            aria-labelledby="recovery-title"
          >
            <div className="panel-heading">
              <div>
                <p className="eyebrow">SENSITIVE WALLET DATA</p>
                <h2 id="recovery-title">Recovery phrase</h2>
              </div>
              <button
                className="close-button"
                type="button"
                aria-label="Close and hide recovery phrase"
                onClick={closeReveal}
              >
                ×
              </button>
            </div>

            {mnemonic ? (
              <>
                <p className="recovery-warning" role="alert">
                  Anyone who sees this phrase can take this wallet. Write it
                  down privately and store it securely. Never share it or enter
                  it on a device you do not trust.
                </p>
                <ol className="mnemonic-grid">
                  {mnemonic.split(' ').map((word, index) => (
                    <li key={`${index}-${word}`}>
                      <span>{index + 1}</span>
                      <strong>{word}</strong>
                    </li>
                  ))}
                </ol>
                <p className="recovery-warning secondary-warning">
                  Malware may capture passwords or screen contents. This reveal
                  does not make a compromised computer safe.
                </p>
                <button className="primary-button" type="button" onClick={closeReveal}>
                  Hide recovery phrase
                </button>
              </>
            ) : (
              <form onSubmit={handleReveal}>
                <p className="dialog-copy">
                  Re-enter this wallet’s password to reveal its 24-word
                  recovery phrase. The phrase will be visible on screen.
                </p>
                <label htmlFor="reveal-password">Vault password</label>
                <input
                  id="reveal-password"
                  type="password"
                  autoComplete="current-password"
                  maxLength={1024}
                  required
                  autoFocus
                  value={revealPassword}
                  onChange={(event) => setRevealPassword(event.target.value)}
                />
                {revealError && (
                  <p className="error-message" role="alert">{revealError}</p>
                )}
                <div className="dialog-actions">
                  <button className="secondary-button" type="button" onClick={closeReveal}>
                    Cancel
                  </button>
                  <button className="primary-button" type="submit" disabled={isRevealing}>
                    {isRevealing ? 'Checking password...' : 'Reveal phrase'}
                  </button>
                </div>
              </form>
            )}
          </section>
        </div>
      )}
    </main>
  )
}

export default App

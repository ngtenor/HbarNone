import { useEffect, useRef, useState } from 'react'
import type { FormEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { generatePassphrase } from './passphrase'
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
  const [walletMode, setWalletMode] = useState<'create' | 'import'>('create')
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [isPasswordVisible, setIsPasswordVisible] = useState(false)
  const [importMnemonic, setImportMnemonic] = useState('')
  const [error, setError] = useState('')
  const [isLoading, setIsLoading] = useState(true)
  const [isSavingWallet, setIsSavingWallet] = useState(false)
  const [walletToReveal, setWalletToReveal] = useState<WalletSummary | null>(null)
  const [walletToDelete, setWalletToDelete] = useState<WalletSummary | null>(null)
  const [isDeletingWallet, setIsDeletingWallet] = useState(false)
  const [deleteError, setDeleteError] = useState('')
  const [revealPassword, setRevealPassword] = useState('')
  const [mnemonic, setMnemonic] = useState('')
  const [revealError, setRevealError] = useState('')
  const [isRevealing, setIsRevealing] = useState(false)
  const [hasConfirmedWriteDown, setHasConfirmedWriteDown] = useState(false)
  const revealRequestId = useRef(0)

  function closeReveal() {
    revealRequestId.current += 1
    setWalletToReveal(null)
    setRevealPassword('')
    setMnemonic('')
    setRevealError('')
    setIsRevealing(false)
    setHasConfirmedWriteDown(false)
  }

  function closeDelete() {
    setWalletToDelete(null)
    setDeleteError('')
  }

  useEffect(() => {
    const hideOnBlur = () => {
      closeReveal()
      closeDelete()
    }
    const hideWhenBackgrounded = () => {
      if (document.visibilityState !== 'visible') {
        closeReveal()
        closeDelete()
      }
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeReveal()
        closeDelete()
      }
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

  async function handleSaveWallet(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError('')
    setIsSavingWallet(true)
    try {
      const wallet = walletMode === 'create'
        ? await invoke<WalletSummary>('create_wallet', { name, password })
        : await invoke<WalletSummary>('import_wallet', {
          name,
          mnemonic: importMnemonic,
          password,
        })
      setWallets((existing) => [...existing, wallet])
      setName('')
      setPassword('')
      setIsPasswordVisible(false)
      setImportMnemonic('')
    } catch (saveError) {
      setError(errorMessage(saveError))
    } finally {
      setIsSavingWallet(false)
    }
  }

  function handleGeneratePassphrase() {
    try {
      setPassword(generatePassphrase())
      setIsPasswordVisible(false)
      setError('')
    } catch {
      setError('Could not generate a secure passphrase on this device.')
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
        setHasConfirmedWriteDown(false)
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

  async function handleDeleteWallet() {
    if (!walletToDelete || isDeletingWallet) return

    const walletId = walletToDelete.id
    setError('')
    setDeleteError('')
    setIsDeletingWallet(true)
    try {
      await invoke('delete_wallet', { walletId })
      setWallets((existing) => existing.filter((wallet) => wallet.id !== walletId))
      setWalletToDelete(null)
    } catch (deleteError) {
      const message = errorMessage(deleteError)
      setDeleteError(message)
      setError(message)
    } finally {
      setIsDeletingWallet(false)
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
          Create an encrypted wallet or import a BIP39 recovery phrase. Stored
          phrases stay encrypted unless you reveal them with your password.
        </p>
      </section>

      <section className="content-grid" aria-label="Wallet management">
        <form className="panel create-panel" onSubmit={handleSaveWallet}>
          <div className="panel-heading">
            <div>
              <p className="eyebrow">{walletMode === 'create' ? 'NEW WALLET' : 'EXISTING WALLET'}</p>
              <h2>{walletMode === 'create' ? 'Create a wallet' : 'Import a wallet'}</h2>
            </div>
            <span className="step-number" aria-hidden="true">01</span>
          </div>

          <div className="mode-switch" role="group" aria-label="Wallet operation">
            <button
              className={walletMode === 'create' ? 'mode-button active' : 'mode-button'}
              type="button"
              aria-pressed={walletMode === 'create'}
              disabled={isSavingWallet}
              onClick={() => {
                setWalletMode('create')
                setImportMnemonic('')
                setError('')
              }}
            >
              Create
            </button>
            <button
              className={walletMode === 'import' ? 'mode-button active' : 'mode-button'}
              type="button"
              aria-pressed={walletMode === 'import'}
              disabled={isSavingWallet}
              onClick={() => {
                setWalletMode('import')
                setError('')
              }}
            >
              Import
            </button>
          </div>

          {walletMode === 'import' && (
            <>
              <label htmlFor="import-mnemonic">English BIP39 recovery phrase</label>
              <textarea
                id="import-mnemonic"
                autoComplete="off"
                autoCapitalize="none"
                maxLength={1024}
                spellCheck={false}
                required
                value={importMnemonic}
                onChange={(event) => setImportMnemonic(event.target.value)}
              />
              <p className="field-hint">
                The phrase is validated and encrypted in this device’s local vault. Do not import a phrase for a wallet containing real funds.
              </p>
            </>
          )}

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
          <div className="password-input-row">
            <input
              id="wallet-password"
              type={isPasswordVisible ? 'text' : 'password'}
              autoComplete="new-password"
              maxLength={1024}
              required
              value={password}
              onChange={(event) => setPassword(event.target.value)}
            />
            <button
              className="password-toggle"
              type="button"
              aria-label={isPasswordVisible ? 'Hide vault password' : 'Show vault password'}
              aria-pressed={isPasswordVisible}
              onClick={() => setIsPasswordVisible((visible) => !visible)}
            >
              {isPasswordVisible ? 'Hide' : 'Show'}
            </button>
          </div>
          <button
            className="secondary-button generate-passphrase"
            type="button"
            disabled={isSavingWallet}
            onClick={handleGeneratePassphrase}
          >
            Generate six-word passphrase
          </button>
          <p className="field-hint">
            Six words are chosen locally using this device’s secure random
            generator (about 77.5 bits of entropy). Store the passphrase safely;
            a forgotten vault password cannot be reset. Never use a recovery
            phrase as your vault password.
          </p>

          <button className="primary-button" type="submit" disabled={isSavingWallet}>
            {isSavingWallet
              ? 'Saving securely...'
              : walletMode === 'create' ? 'Create encrypted wallet' : 'Import encrypted wallet'}
            {!isSavingWallet && <span aria-hidden="true">-&gt;</span>}
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
                  <div className="wallet-actions">
                    <button
                      className="reveal-button"
                      type="button"
                      onClick={() => {
                        closeReveal()
                        setWalletToDelete(null)
                        setWalletToReveal(wallet)
                      }}
                    >
                      Show recovery phrase
                    </button>
                    <button
                      className="delete-wallet-button"
                      type="button"
                      aria-label={`Delete ${wallet.name}`}
                      onClick={() => {
                        closeReveal()
                        setError('')
                        setDeleteError('')
                        setWalletToDelete(wallet)
                      }}
                    >
                      Delete
                    </button>
                  </div>
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
                <label className="write-down-confirmation">
                  <input
                    type="checkbox"
                    checked={hasConfirmedWriteDown}
                    onChange={(event) => setHasConfirmedWriteDown(event.target.checked)}
                  />
                  <span>I have written down this recovery phrase and stored it safely.</span>
                </label>
                <p className="confirmation-hint">
                  This confirms your acknowledgment only; the app does not verify your backup.
                </p>
                <div className="dialog-actions">
                  <button className="secondary-button" type="button" onClick={closeReveal}>
                    Hide now
                  </button>
                  <button
                    className="primary-button"
                    type="button"
                    disabled={!hasConfirmedWriteDown}
                    onClick={closeReveal}
                  >
                    Confirm and hide phrase
                  </button>
                </div>
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

      {walletToDelete && (
        <div
          className="modal-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeDelete()
          }}
        >
          <section
            className="recovery-dialog delete-dialog"
            role="alertdialog"
            aria-modal="true"
            aria-labelledby="delete-title"
            aria-describedby="delete-warning"
          >
            <div className="panel-heading">
              <div>
                <p className="eyebrow">IRREVERSIBLE ACTION</p>
                <h2 id="delete-title">Delete {walletToDelete.name}?</h2>
              </div>
              <button
                className="close-button"
                type="button"
                aria-label="Cancel wallet deletion"
                onClick={closeDelete}
              >
                ×
              </button>
            </div>
            <p id="delete-warning" className="delete-warning" role="alert">
              This removes the wallet from this device’s local vault without
              asking for its password. If you have forgotten that password, you
              will not be able to recover the wallet unless its recovery phrase
              is backed up elsewhere. This does not delete a Hedera network
              account or securely erase older storage copies or backups.
            </p>
            <p className="dialog-copy">
              Confirm deletion of <strong>{walletToDelete.name}</strong>. This
              action cannot be undone.
            </p>
            {deleteError && (
              <p className="error-message" role="alert">{deleteError}</p>
            )}
            <div className="dialog-actions">
              <button
                className="secondary-button"
                type="button"
                disabled={isDeletingWallet}
                onClick={closeDelete}
              >
                Cancel
              </button>
              <button
                className="danger-button"
                type="button"
                disabled={isDeletingWallet}
                onClick={() => void handleDeleteWallet()}
              >
                {isDeletingWallet ? 'Deleting...' : 'Delete wallet'}
              </button>
            </div>
          </section>
        </div>
      )}
    </main>
  )
}

export default App

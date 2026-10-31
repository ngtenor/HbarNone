import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { FormEvent, KeyboardEvent as ReactKeyboardEvent } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { generatePassphrase } from './passphrase'
import { TestnetPanel } from './TestnetPanel'
import type { KeyDetection, WalletKeys, WalletSummary } from './types'
import './App.css'

type VaultView = 'loading' | 'uninitialized' | 'legacy' | 'locked' | 'unlocked'

type VaultState = {
  status: Exclude<VaultView, 'loading'>
  wallets: WalletSummary[]
  legacyWallets: WalletSummary[]
  autoLockSeconds: number
}

const MIN_AUTO_LOCK_SECONDS = 30
const MAX_AUTO_LOCK_SECONDS = 1800
const DEFAULT_AUTO_LOCK_SECONDS = 300

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error)
}

function formatHomeBalance(tinybars: number) {
  return `${(tinybars / 100_000_000).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })} ℏ`
}

function formatAutoLock(seconds: number) {
  const minutes = Math.floor(seconds / 60)
  const remainingSeconds = seconds % 60
  if (minutes === 0) return `${remainingSeconds} seconds`
  if (remainingSeconds === 0) return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`
  return `${minutes} min ${remainingSeconds} sec`
}

function AutoLockSlider({
  seconds,
  onChange,
}: {
  seconds: number
  onChange: (seconds: number) => void
}) {
  return (
    <label className="auto-lock-setting">
      <span>
        Auto-lock after <strong>{formatAutoLock(seconds)}</strong> of inactivity
      </span>
      <input
        aria-label="Auto-lock period"
        type="range"
        min={MIN_AUTO_LOCK_SECONDS}
        max={MAX_AUTO_LOCK_SECONDS}
        step="1"
        value={seconds}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <span className="auto-lock-range">
        <span>30 sec</span>
        <span>30 min</span>
      </span>
    </label>
  )
}

const iconProps = {
  width: 26,
  height: 26,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
  'aria-hidden': true,
} as const

function PlusIcon() {
  return (
    <svg {...iconProps}>
      <path d="M12 5v14M5 12h14" />
    </svg>
  )
}

function CogIcon() {
  return (
    <svg {...iconProps}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.9.3h0a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.9v0a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </svg>
  )
}

function PadlockIcon({ locked }: { locked: boolean }) {
  return (
    <svg {...iconProps}>
      <rect x="4" y="11" width="16" height="10" rx="2" />
      <path d={locked ? 'M8 11V7a4 4 0 0 1 8 0v4' : 'M8 11V7a4 4 0 0 1 7.5-1.9'} />
    </svg>
  )
}

function App() {
  const [wallets, setWallets] = useState<WalletSummary[]>([])
  const [activeTab, setActiveTab] = useState('home')
  const [isCreateTabOpen, setIsCreateTabOpen] = useState(false)
  const [isSettingsTabOpen, setIsSettingsTabOpen] = useState(false)
  const [openWalletTabs, setOpenWalletTabs] = useState<string[]>([])
  const [legacyWallets, setLegacyWallets] = useState<WalletSummary[]>([])
  const [vaultView, setVaultView] = useState<VaultView>('loading')
  const [vaultPassword, setVaultPassword] = useState('')
  const [vaultPasswordConfirmation, setVaultPasswordConfirmation] = useState('')
  const [isVaultPasswordVisible, setIsVaultPasswordVisible] = useState(false)
  const [legacyPasswords, setLegacyPasswords] = useState<Record<string, string>>({})
  const [autoLockSeconds, setAutoLockSeconds] = useState(DEFAULT_AUTO_LOCK_SECONDS)
  const [isSubmittingVault, setIsSubmittingVault] = useState(false)
  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [newPasswordConfirmation, setNewPasswordConfirmation] = useState('')
  const [isNewPasswordVisible, setIsNewPasswordVisible] = useState(false)
  const [passwordChangeError, setPasswordChangeError] = useState('')
  const [passwordChangeSuccess, setPasswordChangeSuccess] = useState(false)
  const [isChangingPassword, setIsChangingPassword] = useState(false)
  const [backupPassword, setBackupPassword] = useState('')
  const [hasConfirmedRestore, setHasConfirmedRestore] = useState(false)
  const [backupError, setBackupError] = useState('')
  const [backupMessage, setBackupMessage] = useState('')
  const [restoreError, setRestoreError] = useState('')
  const [restoreMessage, setRestoreMessage] = useState('')
  const [isBackupBusy, setIsBackupBusy] = useState(false)
  const [isRestoreBusy, setIsRestoreBusy] = useState(false)
  const [hasConfirmedErase, setHasConfirmedErase] = useState(false)
  const [gateMode, setGateMode] = useState<'choose' | 'create' | 'restore'>('choose')
  const [eraseError, setEraseError] = useState('')
  const [isErasing, setIsErasing] = useState(false)
  const [walletMode, setWalletMode] = useState<'create' | 'import'>('create')
  const [renamingWalletId, setRenamingWalletId] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [renameError, setRenameError] = useState('')
  const [isRenaming, setIsRenaming] = useState(false)
  const [name, setName] = useState('')
  const [importMnemonic, setImportMnemonic] = useState('')
  const [importKind, setImportKind] = useState<'phrase' | 'key'>('phrase')
  const [importKeyType, setImportKeyType] = useState<'auto' | 'ed25519' | 'ecdsa'>('auto')
  const [walletKeys, setWalletKeys] = useState<{ id: string, keys: WalletKeys } | null>(null)
  const [balances, setBalances] = useState<Record<string, { account: string, tinybars: number }>>({})
  const fetchedBalances = useRef(new Map<string, string>())
  const [error, setError] = useState('')
  const [isSavingWallet, setIsSavingWallet] = useState(false)
  const [walletToReveal, setWalletToReveal] = useState<WalletSummary | null>(null)
  const [walletToDelete, setWalletToDelete] = useState<WalletSummary | null>(null)
  const [isDeletingWallet, setIsDeletingWallet] = useState(false)
  const [deleteError, setDeleteError] = useState('')
  const [mnemonic, setMnemonic] = useState('')
  const [revealError, setRevealError] = useState('')
  const [isRevealing, setIsRevealing] = useState(false)
  const [hasConfirmedWriteDown, setHasConfirmedWriteDown] = useState(false)
  const revealRequestId = useRef(0)
  const lastActivityAt = useRef(0)
  const lastActivitySyncAt = useRef(0)
  const lockRequestActive = useRef(false)
  const shellRef = useRef<HTMLElement>(null)
  const [logoState, setLogoState] = useState<'full' | 'mark' | 'hidden'>('full')
  const [isPadlockHidden, setIsPadlockHidden] = useState(false)

  // On the centred gate screens the white box can reach the logo or padlock in a small window.
  // Hidden parts keep their layout space, so the measurements stay stable.
  useLayoutEffect(() => {
    const shell = shellRef.current
    const box = shell?.querySelector('.vault-gate')
    const logo = shell?.querySelector('.brand')
    const mark = shell?.querySelector('.brand-mark')
    const padlock = shell?.querySelector('.locked-indicator')
    if ((vaultView !== 'uninitialized' && vaultView !== 'locked') || !shell || !box || !logo || !mark) {
      setLogoState('full')
      setIsPadlockHidden(false)
      return
    }
    const overlaps = (a: DOMRect, b: DOMRect) =>
      a.top < b.bottom && a.bottom > b.top && a.left < b.right && a.right > b.left
    const update = () => {
      const boxRect = box.getBoundingClientRect()
      if (!overlaps(boxRect, logo.getBoundingClientRect())) {
        setLogoState('full')
      } else if (!overlaps(boxRect, mark.getBoundingClientRect())) {
        setLogoState('mark')
      } else {
        setLogoState('hidden')
      }
      setIsPadlockHidden(padlock ? overlaps(boxRect, padlock.getBoundingClientRect()) : false)
    }
    update()
    window.addEventListener('resize', update)
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update)
    observer?.observe(box)
    return () => {
      window.removeEventListener('resize', update)
      observer?.disconnect()
    }
  }, [vaultView, gateMode, error])
  const autoLockSaveTimer = useRef<number | null>(null)

  const closeReveal = useCallback(() => {
    revealRequestId.current += 1
    setWalletToReveal(null)
    setMnemonic('')
    setRevealError('')
    setIsRevealing(false)
    setHasConfirmedWriteDown(false)
  }, [])

  const closeDelete = useCallback(() => {
    setWalletToDelete(null)
    setDeleteError('')
  }, [])

  const clearPasswordChange = useCallback(() => {
    setCurrentPassword('')
    setNewPassword('')
    setNewPasswordConfirmation('')
    setIsNewPasswordVisible(false)
    setPasswordChangeError('')
    setPasswordChangeSuccess(false)
    setBackupPassword('')
    setHasConfirmedRestore(false)
    setBackupError('')
    setBackupMessage('')
    setRestoreError('')
    setRestoreMessage('')
  }, [])

  const lockVault = useCallback(async () => {
    if (lockRequestActive.current || vaultView !== 'unlocked') return
    lockRequestActive.current = true
    closeReveal()
    closeDelete()
    clearPasswordChange()
    setHasConfirmedErase(false)
    setEraseError('')
    setRenamingWalletId(null)
    setName('')
    setImportMnemonic('')
    setImportKind('phrase')
    setImportKeyType('auto')
    setWalletMode('create')
    setWalletKeys(null)
    setBalances({})
    setError('')
    setMnemonic('')
    setVaultPassword('')
    setVaultPasswordConfirmation('')
    setWallets([])
    setActiveTab('home')
    setIsCreateTabOpen(false)
    setIsSettingsTabOpen(false)
    setOpenWalletTabs([])
    try {
      await invoke('lock_vault')
      setError('')
    } catch (lockError) {
      setError(`Vault lock failed: ${errorMessage(lockError)}`)
    } finally {
      setVaultView('locked')
      lockRequestActive.current = false
    }
  }, [clearPasswordChange, closeDelete, closeReveal, vaultView])

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        closeReveal()
        closeDelete()
      }
    }

    window.addEventListener('keydown', closeOnEscape)

    return () => {
      window.removeEventListener('keydown', closeOnEscape)
    }
  }, [closeDelete, closeReveal])

  useEffect(() => {
    const hideSensitiveDialog = () => {
      closeReveal()
      closeDelete()
      setVaultPassword('')
      setVaultPasswordConfirmation('')
      setLegacyPasswords({})
      setIsVaultPasswordVisible(false)
      clearPasswordChange()
    }
    const hideWhenBackgrounded = () => {
      if (document.visibilityState !== 'visible') hideSensitiveDialog()
    }
    window.addEventListener('blur', hideSensitiveDialog)
    document.addEventListener('visibilitychange', hideWhenBackgrounded)
    return () => {
      window.removeEventListener('blur', hideSensitiveDialog)
      document.removeEventListener('visibilitychange', hideWhenBackgrounded)
    }
  }, [clearPasswordChange, closeDelete, closeReveal])

  useEffect(() => {
    let cancelled = false

    void invoke<VaultState>('get_vault_state')
      .then((state) => {
        if (cancelled) return
        setVaultView(state.status)
        setWallets(state.wallets)
        setLegacyWallets(state.legacyWallets)
        setAutoLockSeconds(state.autoLockSeconds)
      })
      .catch((loadError: unknown) => {
        if (!cancelled) setError(errorMessage(loadError))
      })

    return () => {
      cancelled = true
    }
  }, [])

  useEffect(() => () => {
    if (autoLockSaveTimer.current !== null) {
      window.clearTimeout(autoLockSaveTimer.current)
    }
  }, [])

  useEffect(() => {
    if (vaultView !== 'unlocked') {
      fetchedBalances.current.clear()
      return
    }
    for (const item of wallets) {
      const account = item.account_id
      if (!account || fetchedBalances.current.get(item.id) === account) continue
      fetchedBalances.current.set(item.id, account)
      void invoke<{ balance_tinybars: number } | null>('get_account_overview', { walletId: item.id })
        .then((overview) => {
          if (overview && fetchedBalances.current.get(item.id) === account) {
            setBalances((existing) => ({
              ...existing,
              [item.id]: { account, tinybars: overview.balance_tinybars },
            }))
          }
        })
        .catch(() => {
          if (fetchedBalances.current.get(item.id) === account) {
            fetchedBalances.current.delete(item.id)
          }
        })
    }
  }, [vaultView, wallets])

  useEffect(() => {
    if (vaultView !== 'unlocked' || !wallets.some((wallet) => wallet.id === activeTab)) return
    let cancelled = false
    void invoke<WalletKeys>('get_wallet_keys', { walletId: activeTab })
      .then((keys) => {
        if (!cancelled) setWalletKeys({ id: activeTab, keys })
      })
      .catch(() => {
        if (!cancelled) setWalletKeys(null)
      })
    return () => {
      cancelled = true
    }
  }, [activeTab, vaultView, wallets])

  useEffect(() => {
    if (vaultView !== 'unlocked') return

    lastActivityAt.current = Date.now()
    lastActivitySyncAt.current = 0
    const syncActivity = () => {
      const now = Date.now()
      lastActivityAt.current = now
      if (now - lastActivitySyncAt.current < 1000) return
      lastActivitySyncAt.current = now
      void invoke('record_activity').catch((activityError: unknown) => {
        setError(errorMessage(activityError))
        void lockVault()
      })
    }
    const timer = window.setInterval(() => {
      if (Date.now() - lastActivityAt.current >= autoLockSeconds * 1000) {
        void lockVault()
      }
    }, 250)

    window.addEventListener('pointerdown', syncActivity)
    window.addEventListener('pointermove', syncActivity)
    window.addEventListener('keydown', syncActivity)
    window.addEventListener('scroll', syncActivity, true)
    window.addEventListener('touchstart', syncActivity, { passive: true })

    return () => {
      window.clearInterval(timer)
      window.removeEventListener('pointerdown', syncActivity)
      window.removeEventListener('pointermove', syncActivity)
      window.removeEventListener('keydown', syncActivity)
      window.removeEventListener('scroll', syncActivity, true)
      window.removeEventListener('touchstart', syncActivity)
    }
  }, [autoLockSeconds, lockVault, vaultView])

  async function handleCreateVault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (vaultPassword !== vaultPasswordConfirmation) {
      setError('Vault passwords do not match.')
      return
    }
    setError('')
    setIsSubmittingVault(true)
    try {
      const state = await invoke<VaultState>('create_vault', {
        vaultPassword,
        autoLockSeconds,
      })
      setWallets(state.wallets)
      setAutoLockSeconds(state.autoLockSeconds)
      setVaultPassword('')
      setVaultPasswordConfirmation('')
      setIsVaultPasswordVisible(false)
      setVaultView('unlocked')
    } catch (createError) {
      setError(errorMessage(createError))
    } finally {
      setIsSubmittingVault(false)
    }
  }

  async function handleUnlockVault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError('')
    setIsSubmittingVault(true)
    try {
      const state = await invoke<VaultState>('unlock_vault', {
        vaultPassword,
      })
      setWallets(state.wallets)
      setAutoLockSeconds(state.autoLockSeconds)
      setVaultPassword('')
      setIsVaultPasswordVisible(false)
      setVaultView('unlocked')
    } catch (unlockError) {
      setError(errorMessage(unlockError))
      setVaultPassword('')
    } finally {
      setIsSubmittingVault(false)
    }
  }

  async function handleMigrateVault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (vaultPassword !== vaultPasswordConfirmation) {
      setError('Vault passwords do not match.')
      return
    }
    setError('')
    setIsSubmittingVault(true)
    try {
      const state = await invoke<VaultState>('migrate_legacy_vault', {
        vaultPassword,
        autoLockSeconds,
        legacyWalletPasswords: legacyWallets.map((wallet) => ({
          id: wallet.id,
          password: legacyPasswords[wallet.id] ?? '',
        })),
      })
      setWallets(state.wallets)
      setAutoLockSeconds(state.autoLockSeconds)
      setLegacyPasswords({})
      setVaultPassword('')
      setVaultPasswordConfirmation('')
      setIsVaultPasswordVisible(false)
      setVaultView('unlocked')
    } catch (migrationError) {
      setError(errorMessage(migrationError))
      setLegacyPasswords({})
      setVaultPassword('')
      setVaultPasswordConfirmation('')
    } finally {
      setIsSubmittingVault(false)
    }
  }

  async function handleAutoLockChange(seconds: number) {
    setAutoLockSeconds(seconds)
    if (vaultView === 'uninitialized') return
    if (autoLockSaveTimer.current !== null) window.clearTimeout(autoLockSaveTimer.current)
    autoLockSaveTimer.current = window.setTimeout(() => {
      void invoke<number>('set_auto_lock_seconds', { seconds })
        .then((savedSeconds) => {
          setAutoLockSeconds(savedSeconds)
          lastActivityAt.current = Date.now()
        })
        .catch((settingsError: unknown) => setError(errorMessage(settingsError)))
    }, 350)
  }

  function balanceText(item: WalletSummary) {
    const balance = balances[item.id]
    return balance && balance.account === item.account_id ? formatHomeBalance(balance.tinybars) : ''
  }

  const knownBalances = wallets.flatMap((item) => {
    const balance = balances[item.id]
    return balance && balance.account === item.account_id ? [balance.tinybars] : []
  })
  const totalText = knownBalances.length > 0
    ? formatHomeBalance(knownBalances.reduce((sum, value) => sum + value, 0))
    : ''

  async function handleSaveWallet(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setError('')
    setIsSavingWallet(true)
    try {
      let wallet: WalletSummary
      let detectedAccounts: string[] = []
      if (walletMode === 'create') {
        wallet = await invoke<WalletSummary>('create_wallet', { name })
      } else if (importKind === 'phrase') {
        wallet = await invoke<WalletSummary>('import_wallet', {
          name,
          mnemonic: importMnemonic,
        })
      } else {
        let keyType: 'ed25519' | 'ecdsa' | null = importKeyType === 'auto' ? null : importKeyType
        if (keyType === null) {
          let detection: KeyDetection
          try {
            detection = await invoke<KeyDetection>('detect_private_key_type', {
              privateKey: importMnemonic,
            })
          } catch (detectError) {
            setError(`${errorMessage(detectError)} If you know the key type, choose it above.`)
            return
          }
          if (!detection.keyType) {
            setError(
              'Could not tell which key type this is. Choose Ed25519 or ECDSA above and try again.',
            )
            return
          }
          keyType = detection.keyType
          detectedAccounts = detection.accounts
        }
        wallet = await invoke<WalletSummary>('import_private_key', {
          name,
          privateKey: importMnemonic,
          keyType,
        })
        if (detectedAccounts.length === 1) {
          try {
            wallet = await invoke<WalletSummary>('link_wallet_account', {
              walletId: wallet.id,
              accountId: detectedAccounts[0],
            })
          } catch {
            // The wallet is saved; the account can be linked from its tab.
          }
        }
      }
      setWallets((existing) => [...existing, wallet])
      setName('')
      setImportMnemonic('')
      setActiveTab('home')
    } catch (saveError) {
      setError(errorMessage(saveError))
    } finally {
      setIsSavingWallet(false)
    }
  }

  function handleGeneratePassphrase() {
    try {
      const passphrase = generatePassphrase()
      setVaultPassword(passphrase)
      setVaultPasswordConfirmation(passphrase)
      setError('')
    } catch {
      setError('Could not generate a secure passphrase on this device.')
    }
  }

  function handleGeneratePasswordChangePassphrase() {
    try {
      const passphrase = generatePassphrase()
      setNewPassword(passphrase)
      setNewPasswordConfirmation(passphrase)
      setPasswordChangeError('')
      setPasswordChangeSuccess(false)
    } catch {
      setPasswordChangeError('Could not generate a secure passphrase on this device.')
    }
  }

  async function handleChangePassword(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setPasswordChangeSuccess(false)
    if (newPassword !== newPasswordConfirmation) {
      setPasswordChangeError('New vault passwords do not match.')
      return
    }
    setPasswordChangeError('')
    setIsChangingPassword(true)
    try {
      await invoke('change_vault_password', { currentPassword, newPassword })
      setCurrentPassword('')
      setNewPassword('')
      setNewPasswordConfirmation('')
      setPasswordChangeSuccess(true)
    } catch (changeError) {
      setPasswordChangeError(errorMessage(changeError))
    } finally {
      setIsChangingPassword(false)
    }
  }

  async function handleExportBackup() {
    setBackupError('')
    setBackupMessage('')
    setIsBackupBusy(true)
    try {
      const saved = await invoke<boolean>('export_vault_backup')
      setBackupMessage(saved
        ? 'Backup saved. Remember: it opens with your current vault password.'
        : 'Backup cancelled.')
    } catch (exportError) {
      setBackupError(errorMessage(exportError))
    } finally {
      setIsBackupBusy(false)
    }
  }

  async function handleBackupAndErase() {
    if (isErasing || !hasConfirmedErase) return
    setEraseError('')
    setIsErasing(true)
    try {
      const state = await invoke<VaultState | null>('backup_and_erase_vault')
      if (!state) {
        setEraseError('Cancelled. Nothing was erased.')
        return
      }
      closeReveal()
      closeDelete()
      clearPasswordChange()
      setHasConfirmedErase(false)
      setRenamingWalletId(null)
      setName('')
      setImportMnemonic('')
      setImportKind('phrase')
      setImportKeyType('auto')
      setWalletMode('create')
      setWalletKeys(null)
      setBalances({})
      setMnemonic('')
      setVaultPassword('')
      setVaultPasswordConfirmation('')
      setWallets([])
      setActiveTab('home')
      setIsCreateTabOpen(false)
      setIsSettingsTabOpen(false)
      setOpenWalletTabs([])
      setAutoLockSeconds(state.autoLockSeconds)
      setGateMode('choose')
      setError('')
      setVaultView(state.status)
    } catch (failure) {
      setEraseError(errorMessage(failure))
    } finally {
      setIsErasing(false)
    }
  }

  async function handleRestoreBackup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    setRestoreError('')
    setRestoreMessage('')
    setIsRestoreBusy(true)
    try {
      const state = await invoke<VaultState | null>('restore_vault_backup', { backupPassword })
      if (state) {
        setWallets(state.wallets)
        setAutoLockSeconds(state.autoLockSeconds)
        setOpenWalletTabs([])
        closeReveal()
        closeDelete()
        setVaultView('unlocked')
        setRestoreMessage(`Backup restored with ${state.wallets.length} wallet(s).`)
      } else {
        setRestoreMessage('Restore cancelled.')
      }
      setBackupPassword('')
      setHasConfirmedRestore(false)
    } catch (failure) {
      setRestoreError(errorMessage(failure))
      setBackupPassword('')
    } finally {
      setIsRestoreBusy(false)
    }
  }

  function renderEyeToggle() {
    const label = isVaultPasswordVisible ? 'Hide vault password' : 'Show vault password'
    return (
      <button
        className="eye-toggle"
        type="button"
        aria-label={label}
        title={label}
        aria-pressed={isVaultPasswordVisible}
        onClick={() => setIsVaultPasswordVisible((visible) => !visible)}
      >
        <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor"
          strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z" />
          <circle cx="12" cy="12" r="3" />
          {isVaultPasswordVisible && <path d="M3 3l18 18" />}
        </svg>
      </button>
    )
  }

  function renderRestoreForm(idPrefix: string, requireConfirmation: boolean) {
    return (
      <form onSubmit={handleRestoreBackup}>
        <label htmlFor={`${idPrefix}-backup-password`}>Password of the backup file</label>
        <input
          id={`${idPrefix}-backup-password`}
          type="password"
          autoComplete="off"
          maxLength={1024}
          required
          value={backupPassword}
          onChange={(event) => setBackupPassword(event.target.value)}
        />
        <p className="field-hint">
          Enter the vault password that was in use when this backup was
          created. It may differ from your current vault password.
        </p>
        {requireConfirmation && (
          <label className="write-down-confirmation">
            <input
              type="checkbox"
              checked={hasConfirmedRestore}
              onChange={(event) => setHasConfirmedRestore(event.target.checked)}
            />
            <span>
              I understand restoring replaces all wallets in this vault. The
              current vault file is kept as a .pre-restore copy.
            </span>
          </label>
        )}
        {restoreError && <p className="error-message" role="alert">{restoreError}</p>}
        {restoreMessage && <p className="field-hint" role="status">{restoreMessage}</p>}
        <button
          className={idPrefix === 'gate' ? 'primary-button gate-button' : 'secondary-button'}
          type="submit"
          disabled={isRestoreBusy || (requireConfirmation && !hasConfirmedRestore)}
        >
          {isRestoreBusy ? 'Restoring...' : 'Choose backup file and restore'}
        </button>
      </form>
    )
  }

  async function handleReveal() {
    if (!walletToReveal) return

    setRevealError('')
    setIsRevealing(true)
    const requestId = ++revealRequestId.current
    try {
      const recoveredMnemonic = await invoke<string>('reveal_wallet_mnemonic', {
        walletId: walletToReveal.id,
      })
      if (requestId === revealRequestId.current) {
        setMnemonic(recoveredMnemonic)
        setHasConfirmedWriteDown(false)
      }
    } catch (revealFailure) {
      if (requestId === revealRequestId.current) {
        setRevealError(errorMessage(revealFailure))
      }
    } finally {
      if (requestId === revealRequestId.current) setIsRevealing(false)
    }
  }

  async function handleRenameWallet(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!renamingWalletId || isRenaming) return
    setRenameError('')
    setIsRenaming(true)
    try {
      const updated = await invoke<WalletSummary>('rename_wallet', {
        walletId: renamingWalletId,
        name: renameValue,
      })
      setWallets((existing) => existing.map((wallet) => (
        wallet.id === updated.id ? updated : wallet
      )))
      setRenamingWalletId(null)
      setRenameValue('')
    } catch (renameFailure) {
      setRenameError(errorMessage(renameFailure))
    } finally {
      setIsRenaming(false)
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
      setOpenWalletTabs((existing) => existing.filter((id) => id !== walletId))
      if (activeTab === walletId) setActiveTab('home')
      setWalletToDelete(null)
    } catch (deleteError) {
      const message = errorMessage(deleteError)
      setDeleteError(message)
      setError(message)
    } finally {
      setIsDeletingWallet(false)
    }
  }

  function openCreateTab() {
    setIsCreateTabOpen(true)
    setActiveTab('create')
    setError('')
  }

  function openSettingsTab() {
    setIsSettingsTabOpen(true)
    setActiveTab('settings')
    setError('')
  }

  function openWalletTab(walletId: string) {
    setOpenWalletTabs((existing) => (
      existing.includes(walletId) ? existing : [...existing, walletId]
    ))
    setActiveTab(walletId)
    setError('')
    closeReveal()
    closeDelete()
  }

  function handleTabKeyDown(event: ReactKeyboardEvent<HTMLButtonElement>) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return

    const tabs = Array.from(
      event.currentTarget.closest('[role="tablist"]')?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? [],
    )
    const currentIndex = tabs.indexOf(event.currentTarget)
    if (currentIndex < 0 || tabs.length === 0) return

    event.preventDefault()
    const nextIndex = event.key === 'Home'
      ? 0
      : event.key === 'End'
        ? tabs.length - 1
        : (currentIndex + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length
    tabs[nextIndex].focus()
    tabs[nextIndex].click()
  }

  function closeCreateTab() {
    if (isSavingWallet) return
    setIsCreateTabOpen(false)
    if (activeTab === 'create') setActiveTab('home')
    setName('')
    setImportMnemonic('')
    setError('')
  }

  function closeSettingsTab() {
    setIsSettingsTabOpen(false)
    clearPasswordChange()
    if (activeTab === 'settings') setActiveTab('home')
  }

  function closeWalletTab(walletId: string) {
    setOpenWalletTabs((existing) => existing.filter((id) => id !== walletId))
    if (activeTab === walletId) setActiveTab('home')
    if (walletToReveal?.id === walletId) closeReveal()
    if (walletToDelete?.id === walletId) closeDelete()
  }

  return (
    <main
      ref={shellRef}
      className={vaultView === 'uninitialized' || vaultView === 'locked' ? 'app-shell gate-centred' : 'app-shell'}
    >
      <header className="topbar">
        <a
          className={logoState === 'hidden' ? 'brand logo-hidden' : logoState === 'mark' ? 'brand text-hidden' : 'brand'}
          href="/"
          aria-label="HederaVault home"
        >
          <span className="brand-mark" aria-hidden="true">H</span>
          <span>HederaVault</span>
        </a>
        <div className="topbar-actions">
          {vaultView === 'locked' && (
            <span
              className={isPadlockHidden ? 'topbar-icon locked-indicator logo-hidden' : 'topbar-icon locked-indicator'}
              role="img"
              aria-label="Vault locked"
            >
              <PadlockIcon locked />
            </span>
          )}
          {vaultView === 'unlocked' && (
            <>
              <button
                className="topbar-icon"
                type="button"
                aria-label="Create / import wallet"
                title="Create / import wallet"
                onClick={openCreateTab}
              >
                <PlusIcon />
              </button>
              <button
                className="topbar-icon"
                type="button"
                aria-label="Settings"
                title="Settings"
                onClick={openSettingsTab}
              >
                <CogIcon />
              </button>
              <button
                className="topbar-icon"
                type="button"
                aria-label="Lock vault"
                title="Lock vault"
                onClick={() => void lockVault()}
              >
                <PadlockIcon locked={false} />
              </button>
            </>
          )}
        </div>
      </header>

      {vaultView === 'loading' && (
        <section className="panel vault-gate">
          {error && <p className="error-message" role="alert">{error}</p>}
          <p className="empty-state">Loading your local vault...</p>
        </section>
      )}

      {vaultView === 'uninitialized' && gateMode === 'choose' && (
        <section className="panel vault-gate gate-choice" aria-label="Get started">
          <button className="primary-button gate-button" type="button" onClick={() => setGateMode('create')}>
            Create vault
          </button>
          <button className="primary-button gate-button" type="button" onClick={() => setGateMode('restore')}>
            Restore backup
          </button>
        </section>
      )}

      {vaultView === 'uninitialized' && gateMode === 'create' && (
        <section className="panel vault-gate">
          <p className="eyebrow">CREATE YOUR VAULT</p>
          <h2>One password for all your wallets</h2>
          <p className="dialog-copy">
            Your vault password protects every wallet. If it is lost, the
            wallets can only be recovered from recovery phrases you have safely
            backed up.
          </p>
          <form onSubmit={handleCreateVault}>
            <label htmlFor="create-vault-password">Vault password</label>
            <div className="password-field">
              <input
                id="create-vault-password"
                type={isVaultPasswordVisible ? 'text' : 'password'}
                autoComplete="new-password"
                maxLength={1024}
                required
                value={vaultPassword}
                onChange={(event) => setVaultPassword(event.target.value)}
              />
              {renderEyeToggle()}
            </div>
            <label htmlFor="create-vault-password-confirmation">Confirm vault password</label>
            <div className="password-field">
              <input
                id="create-vault-password-confirmation"
                type={isVaultPasswordVisible ? 'text' : 'password'}
                autoComplete="new-password"
                maxLength={1024}
                required
                value={vaultPasswordConfirmation}
                onChange={(event) => setVaultPasswordConfirmation(event.target.value)}
              />
              {renderEyeToggle()}
            </div>
            <button
              className="secondary-button gate-button generate-passphrase"
              type="button"
              disabled={isSubmittingVault}
              onClick={handleGeneratePassphrase}
            >
              Generate six-word passphrase
            </button>
            <p className="field-hint">
              The six words are from the eff.org long word list and selected
              locally using this device’s secure random generator. Write the
              passphrase down and store it safely.
            </p>
            {error && <p className="error-message" role="alert">{error}</p>}
            <button className="primary-button gate-button" type="submit" disabled={isSubmittingVault}>
              {isSubmittingVault ? 'Creating vault...' : 'Create vault'}
            </button>
            <button className="gate-back" type="button" onClick={() => setGateMode('choose')}>
              Back
            </button>
          </form>
        </section>
      )}

      {vaultView === 'uninitialized' && gateMode === 'restore' && (
        <section className="panel vault-gate">
          <p className="eyebrow">RESTORE FROM A BACKUP</p>
          {renderRestoreForm('gate', false)}
          <button className="gate-back" type="button" onClick={() => setGateMode('choose')}>
            Back
          </button>
        </section>
      )}

      {vaultView === 'locked' && (
        <section className="panel vault-gate unlock-gate" aria-label="Unlock vault">
          <form onSubmit={handleUnlockVault}>
            <label htmlFor="unlock-vault-password">Vault password</label>
            <input
              id="unlock-vault-password"
              type="password"
              autoComplete="current-password"
              maxLength={1024}
              required
              autoFocus
              value={vaultPassword}
              onChange={(event) => setVaultPassword(event.target.value)}
            />
            {error && <p className="error-message" role="alert">{error}</p>}
            <button className="primary-button unlock-button" type="submit" disabled={isSubmittingVault}>
              {isSubmittingVault ? 'Unlocking...' : 'Unlock vault'}
            </button>
          </form>
        </section>
      )}

      {vaultView === 'legacy' && (
        <section className="panel vault-gate migration-gate">
          <p className="eyebrow">VAULT UPGRADE</p>
          <h2>Move wallets into one vault</h2>
          <p className="dialog-copy">
            Your existing wallets use separate passwords. Enter each current
            wallet password once and choose a new vault password. The original
            wallet file is preserved unless every wallet is unlocked and the
            new encrypted vault is saved successfully.
          </p>
          <form onSubmit={handleMigrateVault}>
            <label htmlFor="migration-vault-password">New vault password</label>
            <div className="password-input-row">
              <input
                id="migration-vault-password"
                type={isVaultPasswordVisible ? 'text' : 'password'}
                autoComplete="new-password"
                maxLength={1024}
                required
                value={vaultPassword}
                onChange={(event) => setVaultPassword(event.target.value)}
              />
              <button
                className="password-toggle"
                type="button"
                aria-label={isVaultPasswordVisible ? 'Hide vault password' : 'Show vault password'}
                aria-pressed={isVaultPasswordVisible}
                onClick={() => setIsVaultPasswordVisible((visible) => !visible)}
              >
                {isVaultPasswordVisible ? 'Hide' : 'Show'}
              </button>
            </div>
            <label htmlFor="migration-vault-password-confirmation">Confirm new vault password</label>
            <input
              id="migration-vault-password-confirmation"
              type={isVaultPasswordVisible ? 'text' : 'password'}
              autoComplete="new-password"
              maxLength={1024}
              required
              value={vaultPasswordConfirmation}
              onChange={(event) => setVaultPasswordConfirmation(event.target.value)}
            />
            <button
              className="secondary-button generate-passphrase"
              type="button"
              disabled={isSubmittingVault}
              onClick={handleGeneratePassphrase}
            >
              Generate six-word passphrase
            </button>
            <div className="legacy-password-list">
              {legacyWallets.map((wallet) => (
                <div key={wallet.id}>
                  <label htmlFor={`legacy-password-${wallet.id}`}>
                    Current password for {wallet.name}
                  </label>
                  <input
                    id={`legacy-password-${wallet.id}`}
                    type="password"
                    autoComplete="current-password"
                    maxLength={1024}
                    required
                    value={legacyPasswords[wallet.id] ?? ''}
                    onChange={(event) => setLegacyPasswords((existing) => ({
                      ...existing,
                      [wallet.id]: event.target.value,
                    }))}
                  />
                </div>
              ))}
            </div>
            {error && <p className="error-message" role="alert">{error}</p>}
            <button className="primary-button" type="submit" disabled={isSubmittingVault}>
              {isSubmittingVault ? 'Migrating wallets...' : 'Create vault and migrate wallets'}
            </button>
          </form>
        </section>
      )}

      {vaultView === 'unlocked' && (
        <>
          <nav className="vault-tabs" role="tablist" aria-label="Vault tabs">
            <button
              className={`vault-tab${activeTab === 'home' ? ' active' : ''}`}
              id="tab-home"
              type="button"
              role="tab"
              aria-selected={activeTab === 'home'}
              aria-controls={activeTab === 'home' ? 'panel-home' : undefined}
              tabIndex={activeTab === 'home' ? 0 : -1}
              onKeyDown={handleTabKeyDown}
              onClick={() => setActiveTab('home')}
            >
              HOME
            </button>
            {isSettingsTabOpen && (
              <div className={`vault-tab-wrap${activeTab === 'settings' ? ' active' : ''}`} role="presentation">
                <button
                  className="vault-tab"
                  id="tab-settings"
                  type="button"
                  role="tab"
                  aria-selected={activeTab === 'settings'}
                  aria-controls={activeTab === 'settings' ? 'panel-settings' : undefined}
                  tabIndex={activeTab === 'settings' ? 0 : -1}
                  onKeyDown={handleTabKeyDown}
                  onClick={() => setActiveTab('settings')}
                >
                  Settings
                </button>
                <button
                  className="vault-tab-close"
                  type="button"
                  aria-label="Close settings tab"
                  onClick={closeSettingsTab}
                >
                  ×
                </button>
              </div>
            )}
            {isCreateTabOpen && (
              <div className={`vault-tab-wrap${activeTab === 'create' ? ' active' : ''}`} role="presentation">
                <button
                  className="vault-tab"
                  id="tab-create"
                  type="button"
                  role="tab"
                  aria-selected={activeTab === 'create'}
                  aria-controls={activeTab === 'create' ? 'panel-create' : undefined}
                  tabIndex={activeTab === 'create' ? 0 : -1}
                  onKeyDown={handleTabKeyDown}
                  onClick={() => setActiveTab('create')}
                >
                  Create / import
                </button>
                <button
                  className="vault-tab-close"
                  type="button"
                  aria-label="Close create or import tab"
                  disabled={isSavingWallet}
                  onClick={closeCreateTab}
                >
                  ×
                </button>
              </div>
            )}
            {openWalletTabs.map((walletId) => {
              const wallet = wallets.find((item) => item.id === walletId)
              if (!wallet) return null
              return (
                <div
                  className={`vault-tab-wrap${activeTab === walletId ? ' active' : ''}`}
                  role="presentation"
                  key={walletId}
                >
                  <button
                    className="vault-tab"
                    id={`tab-${walletId}`}
                    type="button"
                    role="tab"
                    aria-selected={activeTab === walletId}
                    aria-controls={activeTab === walletId ? `panel-${walletId}` : undefined}
                    tabIndex={activeTab === walletId ? 0 : -1}
                    onKeyDown={handleTabKeyDown}
                    onClick={() => setActiveTab(walletId)}
                  >
                    {wallet.name}
                  </button>
                  <button
                    className="vault-tab-close"
                    type="button"
                    aria-label={`Close ${wallet.name} tab`}
                    onClick={() => closeWalletTab(walletId)}
                  >
                    ×
                  </button>
                </div>
              )
            })}
          </nav>

          {activeTab === 'home' && (
            <section
              className="vault-tab-panel home-panel"
              id="panel-home"
              role="tabpanel"
              aria-labelledby="tab-home"
            >
              {error && <p className="error-message" role="alert">{error}</p>}
              <section className="home-wallets" aria-labelledby="home-wallets-heading">
                <div className="panel-heading">
                  <div>
                    <h2 id="home-wallets-heading">Wallets</h2>
                  </div>
                  <span className="wallet-count">{wallets.length}</span>
                </div>
                {wallets.length > 0 && (
                  <ul className="wallet-list">
                    {wallets.map((wallet) => (
                      <li className="wallet-item" key={wallet.id}>
                        <button
                          className="wallet-open-button"
                          type="button"
                          onClick={() => openWalletTab(wallet.id)}
                        >
                          <span className="wallet-icon" aria-hidden="true">H</span>
                          <span className="wallet-details">
                            <strong>{wallet.name}</strong>
                            <span>Added {new Date(wallet.created_at).toLocaleDateString()}</span>
                          </span>
                          <span className="wallet-balance">{balanceText(wallet)}</span>
                          <span className="wallet-open-indicator" aria-hidden="true">Open →</span>
                        </button>
                      </li>
                    ))}
                    <li className="wallet-item wallet-total">
                      <div className="wallet-open-button">
                        <span className="wallet-icon wallet-total-spacer" aria-hidden="true" />
                        <span className="wallet-details">
                          <strong>Total</strong>
                        </span>
                        <span className="wallet-balance" aria-label="Total balance">{totalText}</span>
                        <span className="wallet-open-indicator wallet-total-spacer" aria-hidden="true">Open →</span>
                      </div>
                    </li>
                  </ul>
                )}
              </section>
            </section>
          )}

          {activeTab === 'settings' && isSettingsTabOpen && (
            <section
              className="vault-tab-panel settings-tab-panel"
              id="panel-settings"
              role="tabpanel"
              aria-labelledby="tab-settings"
            >
              <div className="settings-heading">
                <p className="eyebrow">VAULT PREFERENCES</p>
                <h1>Settings</h1>
              </div>
              <section className="panel settings-panel" aria-labelledby="auto-lock-heading">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">SECURITY</p>
                    <h2 id="auto-lock-heading">Auto-lock</h2>
                  </div>
                </div>
                <p className="dialog-copy">
                  Automatically lock the vault after a period of inactivity.
                  Activity in the app resets the timer.
                </p>
                <AutoLockSlider seconds={autoLockSeconds} onChange={handleAutoLockChange} />
              </section>
              <section className="panel settings-panel create-panel" aria-labelledby="change-password-heading">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">SECURITY</p>
                    <h2 id="change-password-heading">Change vault password</h2>
                  </div>
                </div>
                <p className="dialog-copy">
                  Your wallets are not re-encrypted; only the key protecting
                  them is re-wrapped with the new password. A forgotten vault
                  password still cannot be reset.
                </p>
                <form onSubmit={handleChangePassword}>
                  <label htmlFor="current-vault-password">Current vault password</label>
                  <input
                    id="current-vault-password"
                    type="password"
                    autoComplete="current-password"
                    maxLength={1024}
                    required
                    value={currentPassword}
                    onChange={(event) => setCurrentPassword(event.target.value)}
                  />
                  <label htmlFor="new-vault-password">New vault password</label>
                  <div className="password-input-row">
                    <input
                      id="new-vault-password"
                      type={isNewPasswordVisible ? 'text' : 'password'}
                      autoComplete="new-password"
                      maxLength={1024}
                      required
                      value={newPassword}
                      onChange={(event) => setNewPassword(event.target.value)}
                    />
                    <button
                      className="password-toggle"
                      type="button"
                      aria-label={isNewPasswordVisible ? 'Hide new vault password' : 'Show new vault password'}
                      aria-pressed={isNewPasswordVisible}
                      onClick={() => setIsNewPasswordVisible((visible) => !visible)}
                    >
                      {isNewPasswordVisible ? 'Hide' : 'Show'}
                    </button>
                  </div>
                  <label htmlFor="new-vault-password-confirmation">Confirm new vault password</label>
                  <input
                    id="new-vault-password-confirmation"
                    type={isNewPasswordVisible ? 'text' : 'password'}
                    autoComplete="new-password"
                    maxLength={1024}
                    required
                    value={newPasswordConfirmation}
                    onChange={(event) => setNewPasswordConfirmation(event.target.value)}
                  />
                  <button
                    className="secondary-button generate-passphrase"
                    type="button"
                    disabled={isChangingPassword}
                    onClick={handleGeneratePasswordChangePassphrase}
                  >
                    Generate six-word passphrase
                  </button>
                  <p className="field-hint">
                    Use Show to read a generated passphrase and write it down
                    before submitting. It is cleared if you leave this window
                    or close this tab.
                  </p>
                  {passwordChangeError && (
                    <p className="error-message" role="alert">{passwordChangeError}</p>
                  )}
                  {passwordChangeSuccess && (
                    <p className="field-hint" role="status">Vault password changed.</p>
                  )}
                  <button className="primary-button" type="submit" disabled={isChangingPassword}>
                    {isChangingPassword ? 'Changing password...' : 'Change password'}
                  </button>
                </form>
              </section>

              <section className="panel create-panel" aria-labelledby="backup-heading">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">BACKUP</p>
                    <h2 id="backup-heading">Back up your vault</h2>
                  </div>
                </div>
                <p className="dialog-copy">
                  Saves a copy of your vault to a file you choose. The file is
                  encrypted and contains no readable recovery phrases. It is
                  protected by your current vault password, so no password is
                  needed here, but you will need that password to restore it.
                  Store the file somewhere safe, such as an external drive.
                </p>
                {backupError && <p className="error-message" role="alert">{backupError}</p>}
                {backupMessage && <p className="field-hint" role="status">{backupMessage}</p>}
                <button
                  className="secondary-button"
                  type="button"
                  disabled={isBackupBusy}
                  onClick={() => void handleExportBackup()}
                >
                  {isBackupBusy ? 'Saving...' : 'Choose location and save backup'}
                </button>
              </section>

              <section className="panel create-panel" aria-labelledby="restore-heading">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">RESTORE</p>
                    <h2 id="restore-heading">Restore from a backup</h2>
                  </div>
                </div>
                <p className="dialog-copy">
                  Replaces every wallet in this vault with the wallets in a
                  backup file. Your current vault password will be replaced by
                  the backup's password.
                </p>
                {renderRestoreForm('settings', true)}
              </section>

              <section className="panel create-panel erase-panel" aria-labelledby="erase-heading">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">ERASE</p>
                    <h2 id="erase-heading">Back up and erase this device</h2>
                  </div>
                </div>
                <p className="dialog-copy">
                  Saves an encrypted backup, checks that it was written correctly,
                  and then permanently deletes the vault from this computer, so the
                  app looks as if it had just been installed. You can restore the
                  wallets later from the backup file, which opens with your
                  current vault password. Without that file and password the
                  wallets cannot be recovered from here.
                </p>
                <p className="field-hint">
                  Deleting a file does not guarantee that its contents are
                  unrecoverable from the disk. Full-disk encryption is the best
                  protection.
                </p>
                <label className="write-down-confirmation">
                  <input
                    type="checkbox"
                    checked={hasConfirmedErase}
                    onChange={(event) => setHasConfirmedErase(event.target.checked)}
                  />
                  <span>
                    I understand this removes every wallet from this device and
                    that I must keep the backup file and its password safe.
                  </span>
                </label>
                {eraseError && <p className="error-message" role="alert">{eraseError}</p>}
                <button
                  className="delete-wallet-button erase-button"
                  type="button"
                  disabled={isErasing || !hasConfirmedErase}
                  onClick={() => void handleBackupAndErase()}
                >
                  {isErasing ? 'Saving backup...' : 'Choose location, save backup and erase'}
                </button>
              </section>
            </section>
          )}

          {activeTab === 'create' && isCreateTabOpen && (
            <section
              className="vault-tab-panel create-tab-panel"
              id="panel-create"
              role="tabpanel"
              aria-labelledby="tab-create"
            >
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
                  <div className="mode-switch" role="group" aria-label="What to import">
                    <button
                      className={importKind === 'phrase' ? 'mode-button active' : 'mode-button'}
                      type="button"
                      aria-pressed={importKind === 'phrase'}
                      disabled={isSavingWallet}
                      onClick={() => {
                        setImportKind('phrase')
                        setImportMnemonic('')
                        setError('')
                      }}
                    >
                      Recovery phrase
                    </button>
                    <button
                      className={importKind === 'key' ? 'mode-button active' : 'mode-button'}
                      type="button"
                      aria-pressed={importKind === 'key'}
                      disabled={isSavingWallet}
                      onClick={() => {
                        setImportKind('key')
                        setImportMnemonic('')
                        setError('')
                      }}
                    >
                      Private key
                    </button>
                  </div>
                )}

                {walletMode === 'import' && importKind === 'phrase' && (
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

                {walletMode === 'import' && importKind === 'key' && (
                  <>
                    <label htmlFor="import-private-key">Private key (hex or DER)</label>
                    <input
                      id="import-private-key"
                      type="password"
                      autoComplete="off"
                      autoCapitalize="none"
                      maxLength={1024}
                      spellCheck={false}
                      required
                      value={importMnemonic}
                      onChange={(event) => setImportMnemonic(event.target.value)}
                    />
                    <label htmlFor="import-key-type">Key type</label>
                    <select
                      id="import-key-type"
                      value={importKeyType}
                      onChange={(event) => setImportKeyType(
                        event.target.value as 'auto' | 'ed25519' | 'ecdsa',
                      )}
                    >
                      <option value="auto">Detect (looks the key up on Testnet)</option>
                      <option value="ed25519">Ed25519</option>
                      <option value="ecdsa">ECDSA (secp256k1)</option>
                    </select>
                    <p className="field-hint">
                      A DER key states its own type. For a bare hex key, Detect sends only the
                      public keys to the Testnet mirror node. The key is encrypted in this
                      device’s local vault. Do not import a key for an account holding real funds.
                    </p>
                  </>
                )}

                <label htmlFor="wallet-name">Wallet name</label>
                <input
                  id="wallet-name"
                  autoComplete="off"
                  maxLength={40}
                  placeholder="e.g. Personal"
                  required
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                />
                {error && <p className="error-message" role="alert">{error}</p>}
                <button className="primary-button" type="submit" disabled={isSavingWallet}>
                  {isSavingWallet
                    ? 'Saving securely...'
                    : walletMode === 'create' ? 'Create encrypted wallet' : 'Import encrypted wallet'}
                  {!isSavingWallet && <span aria-hidden="true">-&gt;</span>}
                </button>
              </form>
            </section>
          )}

          {activeTab !== 'home' && activeTab !== 'create' && (() => {
            const wallet = wallets.find((item) => item.id === activeTab)
            if (!wallet) return null
            return (
              <section
                className="vault-tab-panel wallet-tab-panel"
                id={`panel-${wallet.id}`}
                role="tabpanel"
                aria-labelledby={`tab-${wallet.id}`}
              >
                <div className="wallet-tab-heading">
                  <div>
                    <p className="eyebrow">WALLET</p>
                    <h1>{wallet.name}</h1>
                    <p className="wallet-date">Added {new Date(wallet.created_at).toLocaleDateString()}</p>
                  </div>
                  <span className="wallet-icon wallet-icon-large" aria-hidden="true">H</span>
                </div>
                {error && <p className="error-message" role="alert">{error}</p>}
                <dl className="wallet-key-details">
                  <dt>Key type</dt>
                  <dd>
                    {wallet.key_type === 'ecdsa' ? 'ECDSA (secp256k1)' : 'Ed25519'}
                    {wallet.secret_kind === 'privatekey' ? ', imported private key' : ', recovery phrase'}
                  </dd>
                  {walletKeys?.id === wallet.id && (
                    <>
                      <dt>Public key</dt>
                      <dd className="mono-value">{walletKeys.keys.publicKey}</dd>
                      {walletKeys.keys.evmAddress && (
                        <>
                          <dt>EVM address</dt>
                          <dd className="mono-value">{walletKeys.keys.evmAddress}</dd>
                        </>
                      )}
                    </>
                  )}
                </dl>
                {renamingWalletId === wallet.id && (
                  <form className="rename-form" onSubmit={handleRenameWallet}>
                    <label htmlFor="rename-wallet-name">New wallet name</label>
                    <input
                      id="rename-wallet-name"
                      autoComplete="off"
                      maxLength={40}
                      required
                      autoFocus
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                    />
                    <p className="field-hint">
                      Up to 40 characters. Names must be unique within the vault, ignoring capitalisation.
                    </p>
                    {renameError && <p className="error-message" role="alert">{renameError}</p>}
                    <div className="wallet-tab-actions">
                      <button className="primary-button" type="submit" disabled={isRenaming}>
                        {isRenaming ? 'Saving...' : 'Save name'}
                      </button>
                      <button
                        className="secondary-button"
                        type="button"
                        disabled={isRenaming}
                        onClick={() => {
                          setRenamingWalletId(null)
                          setRenameError('')
                        }}
                      >
                        Cancel
                      </button>
                    </div>
                  </form>
                )}
                <div className="wallet-tab-actions">
                  <button
                    className="secondary-button"
                    type="button"
                    onClick={() => {
                      setRenameValue(wallet.name)
                      setRenameError('')
                      setRenamingWalletId(wallet.id)
                    }}
                  >
                    Rename wallet
                  </button>
                  <button
                    className="reveal-button"
                    type="button"
                    onClick={() => {
                      closeReveal()
                      setWalletToDelete(null)
                      setWalletToReveal(wallet)
                    }}
                  >
                    Reveal {wallet.secret_kind === 'privatekey' ? 'private key' : 'recovery phrase'}
                  </button>
                  <button
                    className="delete-wallet-button"
                    type="button"
                    onClick={() => {
                      closeReveal()
                      setError('')
                      setDeleteError('')
                      setWalletToDelete(wallet)
                    }}
                  >
                    Delete wallet
                  </button>
                </div>
                <TestnetPanel
                  key={wallet.id}
                  wallet={wallet}
                  onBalance={(walletId, account, tinybars) => {
                    fetchedBalances.current.set(walletId, account)
                    setBalances((existing) => ({ ...existing, [walletId]: { account, tinybars } }))
                  }}
                  onWalletUpdated={(updated) => setWallets((existing) => existing.map((item) => (
                    item.id === updated.id ? updated : item
                  )))}
                />
              </section>
            )
          })()}

          <footer className="security-note">
            <span aria-hidden="true">!</span>
            A displayed phrase may be captured by malware or screenshots. Development only; do not use real funds.
          </footer>
        </>
      )}

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
                <ol
                  className="mnemonic-grid"
                  onCopy={(event) => event.preventDefault()}
                  onCut={(event) => event.preventDefault()}
                  onContextMenu={(event) => event.preventDefault()}
                  onDragStart={(event) => event.preventDefault()}
                >
                  {walletToReveal.secret_kind === 'privatekey' ? (
                    <li className="private-key-value"><strong>{mnemonic}</strong></li>
                  ) : mnemonic.split(' ').map((word, index) => (
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
              <div>
                <p className="dialog-copy">
                  The vault is unlocked. Reveal this wallet’s recovery phrase
                  only if you can keep it private from people and software
                  viewing this device.
                </p>
                {revealError && (
                  <p className="error-message" role="alert">{revealError}</p>
                )}
                <div className="dialog-actions">
                  <button className="secondary-button" type="button" onClick={closeReveal}>
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    type="button"
                    disabled={isRevealing}
                    onClick={() => void handleReveal()}
                  >
                    {isRevealing ? 'Revealing...' : 'Reveal phrase'}
                  </button>
                </div>
              </div>
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
              This permanently removes only this wallet from the local vault.
              It does not delete a Hedera network account or securely erase
              older storage copies or backups. Make sure its recovery phrase is
              backed up elsewhere before continuing.
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

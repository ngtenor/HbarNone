import type { WalletAccount, WalletSummary } from './types'

const glyph = {
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
} as const

// The wallet symbol: red when the key is hot, blue when it is cold.
// Wallets without a recorded type are hot.
export function WalletBadge({ wallet, large = false }: { wallet: WalletSummary; large?: boolean }) {
  const cold = wallet.custody === 'cold'
  const size = large ? ' wallet-badge-large' : ''
  return (
    <span
      className={`wallet-badge ${cold ? 'wallet-cold' : 'wallet-hot'}${size}`}
      role="img"
      aria-label={cold ? 'Cold wallet' : 'Hot wallet'}
      title={cold ? 'Cold wallet' : 'Hot wallet'}
    >
      <svg {...glyph} width="100%" height="100%">
        <path d="M4 7a2 2 0 0 1 2-2h11v3" />
        <rect x="4" y="7" width="16" height="13" rx="2" />
        <path d="M16 13.5h4v3h-4a1.5 1.5 0 0 1 0-3z" />
      </svg>
    </span>
  )
}

// The Planck-constant symbol (a bold italic Times h with a bar through the upper
// stem, as used for Hbar): grey for a single-signature account, green for multisig.
export function AccountBadge({ account }: { account: WalletAccount }) {
  const multi = account.signing === 'multisig'
  return (
    <span
      className={`account-badge ${multi ? 'account-multisig' : 'account-single'}`}
      role="img"
      aria-label={multi ? 'Multisig account' : 'Single-signature account'}
      title={multi ? 'Multi sig.' : 'Single sig.'}
    >
      <svg viewBox="0 0 24 24" width="100%" height="100%" aria-hidden="true">
        <text
          x="12"
          y="20"
          textAnchor="middle"
          fontFamily="'Times New Roman', Times, serif"
          fontSize="26"
          fontWeight="700"
          fontStyle="italic"
          fill="currentColor"
        >
          h
        </text>
        <path d="M6.8 8.4 14.6 6.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      </svg>
    </span>
  )
}

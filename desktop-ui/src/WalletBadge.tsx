import type { WalletSummary } from './types'

// Top-left half: hot (red H) or cold (blue C). Bottom-right half: single sig
// (gray S) or multisig (green M). Wallets without a recorded type are hot, single sig.
export function WalletBadge({ wallet, large = false }: { wallet: WalletSummary; large?: boolean }) {
  const cold = wallet.custody === 'cold'
  const multi = wallet.signing === 'multisig'
  const label = `${cold ? 'Cold' : 'Hot'} ${multi ? 'multisig' : 'single-signature'} wallet`
  return (
    <span className={large ? 'wallet-badge wallet-badge-large' : 'wallet-badge'} role="img" aria-label={label}>
      <svg viewBox="0 0 34 34" width="100%" height="100%" aria-hidden="true">
        <polygon points="0,0 34,0 0,34" fill={cold ? '#2b6cb0' : '#c0392b'} />
        <polygon points="34,0 34,34 0,34" fill={multi ? '#2f9e6b' : '#7a7f87'} />
        <g fill="#fff" fontSize="11" fontWeight="700" fontFamily="inherit" textAnchor="middle">
          <text x="9" y="13">{cold ? 'C' : 'H'}</text>
          <text x="25" y="27">{multi ? 'M' : 'S'}</text>
        </g>
      </svg>
    </span>
  )
}

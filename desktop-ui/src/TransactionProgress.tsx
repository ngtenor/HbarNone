import { useEffect, useState } from 'react'

export type TxProgress = {
  phase: 'waiting' | 'success' | 'failed' | 'timeout'
  title: string
  detail?: string
}

type Props = {
  progress: TxProgress
  onClose: () => void
}

const SUCCESS_SECONDS = 2

// Mount with a new key per transaction so the minimised state starts fresh.
export function TransactionProgress({ progress, onClose }: Props) {
  const { phase, title, detail } = progress
  const [minimised, setMinimised] = useState(false)

  useEffect(() => {
    if (phase !== 'success') return
    const timer = window.setTimeout(onClose, SUCCESS_SECONDS * 1000)
    return () => window.clearTimeout(timer)
  }, [phase, onClose])

  const problem = phase === 'failed' || phase === 'timeout'

  if (minimised && !problem) {
    return (
      <div className="progress-corner" role="status" aria-live="polite">
        {phase === 'success' ? (
          <span className="progress-thumb" aria-label="Transaction succeeded">👍</span>
        ) : (
          <div className="progress-spinner progress-spinner-small" aria-label="Waiting for Testnet" />
        )}
      </div>
    )
  }

  const dismiss = () => (problem ? onClose() : setMinimised(true))

  return (
    <div className="modal-backdrop" onClick={dismiss}>
      <section
        className="recovery-dialog progress-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="progress-title"
        aria-live="polite"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 id="progress-title" className="progress-title">{title}</h2>
        {phase === 'waiting' && (
          <>
            <div className="progress-spinner" aria-hidden="true" />
            <p className="progress-status">Waiting for Testnet to confirm...</p>
          </>
        )}
        {phase === 'success' && (
          <>
            <div className="progress-mark progress-success" aria-hidden="true">✓</div>
            <p className="progress-status progress-success-text">SUCCESS!</p>
          </>
        )}
        {phase === 'failed' && (
          <>
            <div className="progress-mark progress-failed" aria-hidden="true">✕</div>
            <p className="progress-status progress-failed-text">The transaction failed</p>
          </>
        )}
        {phase === 'timeout' && (
          <p className="progress-status">
            Testnet has not confirmed this yet. Check the recent transactions in a moment.
          </p>
        )}
        {detail && <p className="field-hint progress-detail">{detail}</p>}
        {problem && <p className="field-hint">Click outside this box to dismiss it.</p>}
      </section>
    </div>
  )
}

import { useEffect, useRef, useState } from 'react'

export type MenuEntry =
  | { kind: 'heading'; label: string }
  | { kind?: 'item'; id: string; label: string; detail?: string; disabled?: boolean; danger?: boolean }

type Props = {
  label: string
  entries: MenuEntry[]
  onSelect: (id: string) => void
  primary?: boolean
  disabled?: boolean
}

export function DropdownMenu({ label, entries, onSelect, primary = false, disabled = false }: Props) {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)

  const enabledItems = () =>
    Array.from(root.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])

  useEffect(() => {
    if (!open) return
    enabledItems()[0]?.focus()
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [open])

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.stopPropagation()
      setOpen(false)
      trigger.current?.focus()
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      const items = enabledItems()
      if (items.length === 0) return
      const index = items.indexOf(document.activeElement as HTMLButtonElement)
      const step = event.key === 'ArrowDown' ? 1 : -1
      items[(index + step + items.length) % items.length].focus()
    } else if (event.key === 'Tab') {
      setOpen(false)
    }
  }

  return (
    <div className="dropdown" ref={root} onKeyDown={handleKeyDown}>
      <button
        ref={trigger}
        className={primary ? 'primary-button dropdown-trigger' : 'secondary-button dropdown-trigger'}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
      >
        {label} <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <ul className="dropdown-list" role="menu" aria-label={label}>
          {entries.map((entry, index) =>
            entry.kind === 'heading' ? (
              <li key={`heading-${index}`} className="dropdown-heading" role="presentation">
                {entry.label}
              </li>
            ) : (
              <li key={entry.id} role="presentation">
                <button
                  className={`dropdown-item${entry.danger ? ' dropdown-danger' : ''}`}
                  type="button"
                  role="menuitem"
                  disabled={entry.disabled}
                  onClick={() => {
                    setOpen(false)
                    onSelect(entry.id)
                  }}
                >
                  <span>{entry.label}</span>
                  {entry.detail && <span className="dropdown-detail">{entry.detail}</span>}
                </button>
              </li>
            ),
          )}
        </ul>
      )}
    </div>
  )
}

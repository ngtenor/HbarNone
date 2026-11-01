// An account is always shown as its name first, then its number with checksum.
export function AccountLabel({ name, displayId }: { name?: string; displayId: string }) {
  return (
    <>
      {name && <span className="account-name">{name}</span>}
      {name && ' '}
      <span className="account-number">{displayId}</span>
    </>
  )
}

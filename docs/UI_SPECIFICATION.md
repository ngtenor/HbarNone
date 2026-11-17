# UI_SPECIFICATION.md

# HBarNone User Interface Specification

Version: 1.2
Status: Draft
Author: HBarNone Project
Classification: User Interface Standard

---

# 1. Purpose

This document defines the user-interface requirements for the HBarNone ecosystem.

The objective is to provide a consistent, understandable and security-focused user experience across:

- Forge
- Sentry
- Signer
- Wallet

The interface shall prioritize clarity, auditability and prevention of operator error.

---

# 2. Design Principles

All HBarNone interfaces shall be designed according to the following principles:

- Security before convenience
- Offline-first operation
- Clear trust indicators
- Minimal operator ambiguity
- Explicit confirmation of security-sensitive actions
- Consistent terminology across applications

---

# 3. Forge Interface

The Forge home screen shall provide:

```text
CREATE NEW WALLET
IMPORT EXISTING WALLET
VIEW CERTIFICATIONS
VIEW AUDIT STATUS
SYSTEM STATUS
EXIT
```

---

# 4. New Wallet Workflow

When creating a new wallet:

```text
CREATE WALLET
      ↓
GENERATE MNEMONIC
      ↓
DERIVE PRIVATE KEY
      ↓
DERIVE PUBLIC KEY
      ↓
REQUEST CERTIFICATION
      ↓
PRINT CERTIFICATE
```

The user shall be shown:

- Certification status
- Key origin
- Certificate identifier

before printing.

---

# 5. Import Wallet Workflow

Imported wallets shall be treated as a privileged operation.

Workflow:

```text
IMPORT WALLET
      ↓
REQUEST SENTRY AUTHORIZATION
      ↓
ENTER 24-WORD SEED PHRASE
      ↓
VALIDATE PHRASE
      ↓
DERIVE KEYS
      ↓
CERTIFY IMPORT
      ↓
PRINT CERTIFICATE
```

If import is denied:

```text
IMPORT REFUSED
SYSTEM NOT CERTIFIED
```

shall be displayed.

---

# 6. Sentry Status Screen

The Sentry user interface shall display:

- Current State
- Violation Count
- Inspection Count
- Last Inspection Time
- Import Eligibility
- Audit Status
- Certification Status

Example:

```text
STATE: CERTIFIED
VIOLATIONS: 0
INSPECTIONS: 247
IMPORT ELIGIBLE: YES
AUDIT STATUS: VALID
```

---

# 7. Certificate Preview Screen

Before printing, the Forge shall display a preview containing:

- Certificate ID
- Key Origin
- Certification Type
- Public-Key Fingerprint

without exposing the full private key.

---

# 8. Signer Main Menu

The Signer shall present:

```text
1. Sign Transaction
2. Display Public Key
3. Verify Key
4. Settings
```

---

# 9. Sign Transaction Screen

Workflow:

```text
SCAN TRANSACTION QR
      ↓
VALIDATE PREFIX
      ↓
DISPLAY SUMMARY
      ↓
CONFIRM
      ↓
SIGN
      ↓
DISPLAY SIGNATURE QR
```

The user must explicitly confirm before signing.

---

# 10. Display Public Key Screen

Workflow:

```text
DISPLAY PUBLIC KEY
      ↓
LOAD PRIVATE KEY
      ↓
DERIVE PUBLIC KEY
      ↓
DISPLAY PUBLIC KEY
      ↓
DISPLAY FINGERPRINT
```

The Signer shall never accept an externally supplied public key for display.

---

# 11. Verify Key Screen

The Verify Key function shall display:

- Key fingerprint
- Key status
- Import/generated status
- Storage status

Example:

```text
KEY STATUS: VALID
ORIGIN: GENERATED
FINGERPRINT: AB7E93F8...
```

---

# 12. Wallet Interface

The Wallet shall provide:

```text
ACCOUNTS
BALANCES
SEND
RECEIVE
SCAN SIGNATURE
SETTINGS
```

The Wallet shall never request a private key.

---

# 13. Wallet Trust Indicators

When a wallet is added from a HBarNone certificate, the Wallet shall display:

- Generated or Imported origin
- Certificate identifier
- Public-key fingerprint

Example:

```text
ORIGIN: GENERATED
CERTIFICATE: HBN-2026-000001
FINGERPRINT: AB7E93F8...
```

---

# 14. Certificate Layout Requirements

The user interface shall reflect the printed certificate structure:

Top Left:
- Private Key QR

Top Right:
- Sentry Certification Information

Bottom Left:
- Seed Phrase (3 columns x 8 words)

Bottom Right:
- Public Key Text
- Public Key Fingerprint

No public-key QR code shall be generated.

---

# 15. Color and Status Indicators

Recommended status colours:

```text
GREEN  = CERTIFIED
AMBER  = WARNING
RED    = VIOLATION_RECORDED
GREY   = UNINITIALIZED
BLACK  = LOCKED
```

Colour shall never be the sole means of conveying status.

---

# 16. Security Warnings

Imported certificates shall display:

```text
IMPORTED KEY
CERTIFICATION BEGINS AT IMPORT
```

Generated certificates shall display:

```text
GENERATED INSIDE CERTIFIED ENVIRONMENT
```

---

# 17. Accessibility Requirements

Interfaces shall:

- Support keyboard navigation
- Use high-contrast layouts
- Avoid reliance on colour alone
- Display critical warnings prominently
- Confirm all destructive operations

---

# 18. Future Enhancements

Potential future enhancements include:

- Multi-language support
- Secure operator roles
- Touch-screen optimized layouts
- Certificate authenticity verification displays
- Audit-log visualisation

---

**End of UI_SPECIFICATION.md v1.2**

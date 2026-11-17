# SIGNER_FIRMWARE_SPECIFICATION.md

# HBarNone Signer Firmware Specification

Version: 1.2
Status: Draft
Author: HBarNone Project
Classification: Firmware Standard

---

# 1. Purpose

This document defines the required behaviour of the HBarNone Signer firmware.

The Signer is an offline cryptographic appliance responsible for secure storage of private keys and generation of digital signatures.

The Signer shall never communicate directly with blockchain networks.

---

# 2. Security Objectives

The Signer SHALL:

- Store private keys securely.
- Sign authorized transactions.
- Verify QR-code data formats.
- Derive public keys from stored private keys.
- Display public-key fingerprints.
- Prevent disclosure of private-key material.
- Operate entirely offline.

---

# 3. Responsibilities

The Signer is responsible for:

- Private-key storage
- Transaction signing
- Signature generation
- Public-key derivation
- Public-key display
- Key verification

The Signer is NOT responsible for:

- Key generation
- Wallet creation
- Seed phrase creation
- Blockchain communication
- Account management

These functions belong to the Forge and Wallet components.

---

# 4. Trust Model

The authoritative hierarchy is:

```text
Seed Phrase
      ↓
Private Key
      ↓
Public Key
      ↓
Derived Metadata
```

The Signer trusts only its internally stored private key.

Externally supplied public keys shall never be treated as authoritative.

---

# 5. Main Menu

The firmware shall present:

```text
1. Sign Transaction
2. Display Public Key
3. Verify Key
4. Settings
```

---

# 6. Sign Transaction Workflow

Workflow:

```text
SCAN TRANSACTION QR
      ↓
VALIDATE PREFIX
      ↓
PARSE TRANSACTION
      ↓
DISPLAY SUMMARY
      ↓
USER CONFIRMATION
      ↓
SIGN
      ↓
DISPLAY SIGNATURE QR
```

No signature shall be generated without explicit user approval.

---

# 7. Transaction Display Requirements

Before signing, the device SHALL display:

- Transaction type
- Destination account
- Amount
- Fee
- Network identifier

The information displayed shall be sufficient for informed approval.

---

# 8. Display Public Key Function

The Signer shall support public-key display.

Workflow:

```text
DISPLAY PUBLIC KEY
      ↓
LOAD STORED PRIVATE KEY
      ↓
DERIVE PUBLIC KEY
      ↓
DISPLAY PUBLIC KEY
      ↓
DISPLAY FINGERPRINT
```

No external public-key QR code shall be required.

---

# 9. Public Key Derivation

The public key shall always be derived from the stored private key.

The Signer shall never:

- Trust externally supplied public keys
- Store public keys as the root identity
- Display externally supplied public-key data as authoritative

---

# 10. Verify Key Function

The Verify Key function shall display:

- Key status
- Fingerprint
- Storage integrity status
- Key identifier (if present)

Example:

```text
KEY STATUS: VALID
FINGERPRINT: AB7E93F8...
STORAGE STATUS: OK
```

---

# 11. QR Code Prefix Validation

All scanned QR payloads shall contain recognized prefixes.

Examples:

```text
TXN:
PRVKEY:
SIG:
```

Unrecognized prefixes shall be rejected.

Example:

```text
INVALID QR FORMAT
UNRECOGNIZED PREFIX
```

---

# 12. Private-Key Import

The Signer shall not import seed phrases.

The Signer shall not generate keys.

Private-key provisioning shall only occur through approved initialization procedures defined elsewhere in the HBarNone architecture.

---

# 13. Signature Output

Generated signatures shall be displayed as QR codes.

Mandatory prefix:

```text
SIG:
```

The signature output must contain only information required for verification.

---

# 14. Security Controls

The firmware shall:

- Clear temporary buffers after use.
- Avoid persistent storage of transaction payloads.
- Reject malformed QR codes.
- Require user confirmation before signing.
- Prevent display of private-key material.

---

# 15. Error Handling

Security-relevant failures shall be explicit.

Examples:

```text
INVALID QR FORMAT
```

```text
SIGNING FAILED
```

```text
KEY NOT AVAILABLE
```

```text
FIRMWARE ERROR
```

Firmware shall fail safely.

---

# 16. Display Requirements

The display shall provide:

- Readable transaction summaries
- Public-key display capability
- Fingerprint display capability
- Error and warning messages
- User confirmation prompts

Security-critical information shall remain visible long enough for operator verification.

---

# 17. Offline Operation

The Signer shall operate without:

- Internet connectivity
- Bluetooth connectivity
- Cloud services
- External account services

Transaction transfer shall occur exclusively through QR-code exchange.

---

# 18. Future Enhancements

Candidate future enhancements include:

- Secure-element key storage
- Multiple signing identities
- Deterministic firmware attestation
- Advanced transaction decoding
- Hardware tamper detection

---

**End of SIGNER_FIRMWARE_SPECIFICATION.md v1.2**

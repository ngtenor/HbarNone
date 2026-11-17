# ARCHITECTURE.md

# HBarNone Architecture Specification

Version: 1.2  
Status: Draft  
Author: HBarNone Project  
Classification: Architecture Standard

---

# 1. Purpose

HBarNone is an offline-first cryptographic ecosystem designed to generate, store, certify, verify and use Hedera-compatible cryptographic keys while minimizing dependence upon network-connected systems.

The ecosystem is composed of four principal components:

1. Forge
2. Sentry
3. Signer
4. Wallet

The architecture is designed around the principles of:

- User sovereignty
- Offline operation
- Cryptographic verifiability
- Auditability
- Portability
- Defence in depth

No component shall require trust in any other component beyond explicitly defined interfaces.

---

# 2. Core Philosophy

The primary ownership artifact is the seed phrase.

All other cryptographic material is derived from the seed phrase.

Specifically:

```text
Seed Phrase
    ↓
Seed
    ↓
Private Key
    ↓
Public Key
    ↓
Hedera Account
```

Users must retain the ability to migrate away from the HBarNone ecosystem without requiring HBarNone software.

Consequently, all certified key records shall contain the complete seed phrase used to derive the associated private key.

---

# 3. Ecosystem Components

## 3.1 Forge

The Forge is responsible for:

- Generating new cryptographic identities
- Importing existing cryptographic identities
- Producing certified paper records
- Creating audit trail entries
- Interacting with the Sentry

The Forge never performs transaction signing.

The Forge never communicates with public networks.

## 3.2 Sentry

The Sentry is the trust anchor for the ecosystem.

The Sentry is responsible for:

- Environment inspection
- Tamper monitoring
- Audit logging
- Certification issuance
- Import authorization
- Trust status reporting

The Sentry does not generate keys.

The Sentry does not store private keys.

The Sentry does not sign blockchain transactions.

## 3.3 Signer

The Signer is responsible for:

- Secure storage of private keys
- Transaction authorization
- Transaction signing
- Public key derivation
- Signature generation

The Signer shall never generate private keys.

The Signer shall never import seed phrases.

The Signer acts only upon private keys already present within its secure storage.

## 3.4 Wallet

The Wallet is responsible for:

- Account management
- Transaction construction
- Signature verification
- Blockchain communication
- Balance retrieval
- User interaction

The Wallet never possesses private keys.

The Wallet never constructs signatures.

All signing operations occur on the Signer.

---

# 4. Key Lifecycle Types

## 4.1 Generated Keys

Generated keys are created entirely within a certified Forge environment.

Generated keys may receive the classification:

```text
CERTIFIED_GENERATED
```

## 4.2 Imported Keys

Imported keys originate outside the HBarNone ecosystem.

Examples:

- Ledger
- Trezor
- Coldcard
- Paper wallet
- Existing mnemonic backup

Imported keys may receive the classification:

```text
CERTIFIED_IMPORTED
```

Meaning:

> HBarNone certifies the integrity of the import process only and makes no assertions regarding generation, storage or handling before import.

---

# 5. Import Security Requirements

The Forge SHALL NOT permit seed phrase, mnemonic or private-key import unless the Sentry reports a certified state and zero recorded violations.

---

# 6. Certificate Generation

The Forge produces a printable certificate that acts as a recovery document, portability document and trust document.

---

# 7. Certificate Layout

## Top Left

Private Key QR Code

Prefix:

```text
PRVKEY:
```

## Top Right

Sentry Certification Block

Including:

- Certificate ID
- Inspection Count
- Last Inspection Time
- Sentry State
- Key Origin
- Certification Type

## Bottom Left

Seed Phrase

- 24 words
- Numbered
- Three columns
- Eight words per column

## Bottom Right

Public Key Information

Displayed as:

- Public key text
- Public key fingerprint

A public-key QR code SHALL NOT be printed.

---

# 8. Key Origin Identification

Certificates shall prominently display:

- GENERATED
- IMPORTED

---

# 9. Imported Key Disclaimer

Imported certificates shall state that HBarNone certifies only the integrity of the import process and not the security of the key before import.

---

# 10. Public Key Trust Model

The seed phrase is the root authority.

The private key is authoritative.

Public keys shall be derived internally and never trusted from external scanned input.

---

# 11. Signer Public Key Display Function

Signer menu:

```text
1. Sign Transaction
2. Display Public Key
3. Verify Key
4. Settings
```

The Signer derives and displays the public key from its internally stored private key.

---

# 12. Portability Requirement

The printed certificate shall provide sufficient information for recovery outside the HBarNone ecosystem.

---

# 13. Security Hierarchy

```text
Seed Phrase
      ↓
Private Key
      ↓
Public Key
      ↓
Certificates
      ↓
Derived Metadata
```

---

# 14. Future Enhancements

- Multi-signature certification
- Hardware-backed Sentry attestation
- Certificate authenticity verification
- Air-gapped archive workflows
- Advanced forensic audit reporting

---

**End of ARCHITECTURE.md v1.2**

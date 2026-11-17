# CERTIFICATE_FORMAT.md

# HBarNone Certificate Format Standard

Version: 1.1
Status: Draft
Author: HBarNone Project
Classification: Standards Document

---

# 1. Purpose

This document defines the mandatory format of HBarNone paper certificates.

The certificate serves four purposes:

1. Recovery document
2. Ownership record
3. Portability document
4. Security certification record

A certificate must contain sufficient information for a user to recover and migrate their cryptographic identity outside the HBarNone ecosystem without requiring proprietary software.

---

# 2. Certificate Types

The ecosystem supports two certificate types.

## 2.1 Generated Certificate

Issued when a key is created entirely within a certified HBarNone Forge environment.

Classification:

```text
CERTIFIED_GENERATED
```

## 2.2 Imported Certificate

Issued when a seed phrase or private key originates outside the HBarNone ecosystem and is imported into a certified Forge.

Classification:

```text
CERTIFIED_IMPORTED
```

Imported certificates shall contain an import disclaimer.

---

# 3. Page Layout

Certificates shall be printed in landscape orientation.

The page is divided into four primary sections.

```text
+-------------------------------------------------------+
| PRIVATE KEY QR      | SENTRY CERTIFICATION            |
|                      |                                |
+-------------------------------------------------------+
| SEED PHRASE         | PUBLIC KEY INFORMATION          |
|                      |                                |
+-------------------------------------------------------+
```

---

# 4. Top Left Section

## Private Key QR Code

This section contains the recoverable private key QR code.

Mandatory prefix:

```text
PRVKEY:
```

Example:

```text
PRVKEY:302e020100300506032b657004220420...
```

The QR code shall be the only machine-readable private-key representation on the certificate.

---

# 5. Top Right Section

## Sentry Certification Block

The certification section shall contain:

- Certificate ID
- Certificate Version
- Issue Date
- Sentry Status
- Inspection Count
- Last Inspection Date
- Key Origin
- Certification Type

Example:

```text
Certificate ID: HBN-2026-000001
Version: 1.1
Issue Date: 2026-11-17
Status: CERTIFIED
Inspection Count: 247
Last Inspection: 2026-11-17
Key Origin: GENERATED
Certification: CERTIFIED_GENERATED
```

---

# 6. Bottom Left Section

## Seed Phrase

The complete 24-word mnemonic shall be printed.

Format requirements:

- 24 words
- Numbered
- Three columns
- Eight words per column

Example:

```text
 1. abandon      9. cattle      17. oval
 2. ability     10. desk        18. patrol
 3. able        11. eager       19. question
 4. about       12. future      20. road
 5. above       13. glass       21. simple
 6. absent      14. harbor      22. travel
 7. absorb      15. island      23. useful
 8. abuse       16. jungle      24. zoo
```

The seed phrase is the authoritative recovery mechanism.

---

# 7. Bottom Right Section

## Public Key Information

The public key shall be displayed as text.

Example:

```text
PUBLIC KEY

302a300506032b6570032100xxxxxxxxxxxxxxxx
```

A public-key QR code SHALL NOT be printed.

## Public Key Fingerprint

The certificate shall additionally display a fingerprint derived from the public key.

Example:

```text
FINGERPRINT

AB7E93F8C1A4D617AE44D9B312A9E5C1
```

This fingerprint may be used by wallet software and signer devices for identity confirmation.

---

# 8. Key Origin Field

A prominent key-origin field shall be displayed.

Valid values:

```text
GENERATED
```

or

```text
IMPORTED
```

The field shall be visually obvious to future holders of the certificate.

---

# 9. Imported Certificate Disclaimer

Certificates with an IMPORTED origin shall display the following notice:

> This key was imported from an external source.
>
> HBarNone certifies that the import occurred within a certified environment and that no known security violations existed at the time of import.
>
> HBarNone does not certify the generation, storage, transport, backup or protection of this key before import.

This notice is mandatory.

---

# 10. Security Classification

Possession of the certificate shall be treated as equivalent to possession of the cryptographic identity.

Because the certificate contains:

- Seed phrase
- Private key QR code

Anyone possessing the certificate may control associated funds.

Certificates shall therefore be stored securely.

---

# 11. Recovery Guarantees

A valid certificate contains sufficient information to recover a wallet using:

- HBarNone Wallet
- Hardware wallets supporting compatible derivation paths
- Compatible BIP-39 recovery software
- Future wallet implementations

No proprietary HBarNone data shall be required.

---

# 12. Future Extensions

Reserved future enhancements:

- Tamper-evident print formats
- Certificate authenticity QR verification
- Multi-signature certificate support
- Offline certificate archival workflows
- Forensic audit references

---

**End of CERTIFICATE_FORMAT.md v1.1**

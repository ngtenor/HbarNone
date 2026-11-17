# QR_FORMAT_STANDARD.md

# HBarNone QR Format Standard (HQFS) v1.0

**Status:** Draft Standard
**Version:** 1.0
**Classification:** Interoperability Standard
**Applies To:** Forge, Wallet, Signer, Sentry, Recovery Documents

---

# 1. Purpose

This document defines the mandatory QR code payload format used throughout the HBarNone ecosystem.

The goals of the standard are to:

- Eliminate ambiguity when scanning QR codes.
- Enable automatic routing of scanned data.
- Support offline operation.
- Improve user safety.
- Ensure interoperability between HBarNone components.
- Support future protocol expansion.

All HBarNone QR payloads SHALL begin with a registered HBarNone QR prefix.

---

# 2. Design Principles

## 2.1 Typed Payloads

Every QR code SHALL declare its content type.

Implementations SHALL NOT attempt to infer payload meaning from structure alone.

## 2.2 Human Readability

Prefixes SHALL be human-readable.

Example:

```text
HBN-MNEMONIC:
```

## 2.3 Vendor Independence

Payloads SHALL be plain text or UTF-8 encoded JSON and SHALL NOT depend upon proprietary formats.

---

# 3. General Format

```text
PREFIX:PAYLOAD
```

Example:

```text
HBN-ACCOUNT:0.0.123456
```

---

# 4. Registered Payload Types

## HBN-MNEMONIC

Contains a BIP-39 mnemonic.

```text
HBN-MNEMONIC:word1 word2 word3 ...
```

## HBN-PUBKEY

Contains a public key.

```text
HBN-PUBKEY:<public-key>
```

## HBN-PRIVKEY

Contains private key material.

```text
HBN-PRIVKEY:<private-key>
```

Wallets SHALL display a warning before import.

## HBN-ACCOUNT

Contains a Hedera account identifier.

```text
HBN-ACCOUNT:0.0.123456
```

## HBN-CERT

Contains certificate metadata.

Example JSON:

```json
{
  "certificate_hash":"...",
  "audit_hash":"...",
  "hkds_version":"1.0"
}
```

## HBN-TXREQ

Contains an unsigned transaction request.

## HBN-TXSIG

Contains a completed signature package.

## HBN-AUDIT

Contains audit information.

## HBN-LOG

Contains exported audit log data.

---

# 5. Wallet Requirements

The Wallet SHALL:

1. Scan QR code.
2. Read prefix.
3. Validate payload type.
4. Route payload to appropriate handler.

Unknown prefixes SHALL be rejected.

---

# 6. Signer Requirements

The Signer SHALL accept:

- HBN-TXREQ
- HBN-MNEMONIC
- HBN-PRIVKEY (where enabled)

The Signer SHALL generate:

- HBN-TXSIG

---

# 7. Forge Requirements

The Forge SHALL generate:

- HBN-MNEMONIC
- HBN-PUBKEY
- HBN-CERT

Generated certificates SHOULD include QR representations where practical.

---

# 8. Sentry Requirements

The Sentry SHALL validate:

- Prefix correctness
- Payload structure
- JSON schema compliance where applicable

The Sentry SHALL reject malformed payloads.

---

# 9. Versioning

Future payloads SHALL preserve backward compatibility where practical.

Reserved prefixes shall never be reassigned.

---

# 10. Compliance Checklist

A compliant implementation SHALL:

- Use registered prefixes.
- Reject unknown payloads.
- Validate payload structure.
- Route by prefix.
- Prevent silent type guessing.
- Support UTF-8 data.

---

# Architecture Requirement

All QR-encoded payloads within the HBarNone ecosystem SHALL begin with a registered HBarNone QR Prefix as defined by QR_FORMAT_STANDARD.md.

---

**End of QR_FORMAT_STANDARD.md**

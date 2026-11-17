# CRYPTOGRAPHIC_OPERATIONS_STANDARD.md

# HBarNone Cryptographic Operations Standard (HCOS) v1.0

**Status:** Draft Standard
**Version:** 1.0
**Classification:** Core Security Standard
**Applies To:** Forge, Wallet, Signer, Sentry, Recovery Tools

---

# 1. Purpose

This standard defines the approved cryptographic operations used throughout the HBarNone ecosystem.

The objectives are to:

- Ensure consistent transaction signing.
- Prevent implementation drift.
- Establish deterministic verification behaviour.
- Define memory-protection requirements.
- Define canonical data encoding rules.
- Support independent security auditing.

---

# 2. Approved Algorithms

## Digital Signatures

Primary algorithm:

```text
Ed25519
```

All transaction signatures SHALL use Ed25519.

## Hashing

Approved hash function:

```text
SHA-384
```

Where Hedera compatibility requires SHA-384.

Additional approved implementation utility hashes:

```text
SHA-256
SHA-512
```

## Key Derivation

Defined by:

```text
KEY_DERIVATION_STANDARD.md
```

The cryptographic operations layer SHALL NOT define alternative key derivation schemes.

---

# 3. Canonical Data Representation

Before hashing or signing, data SHALL be converted into a canonical representation.

Requirements:

- UTF-8 encoding
- Deterministic field ordering
- No duplicate keys
- No implementation-specific formatting
- No whitespace significance

Implementations SHALL produce identical byte sequences for identical logical payloads.

---

# 4. Transaction Hashing Standard

Transactions SHALL be reduced to a canonical byte representation.

Hash Flow:

```text
Transaction
  ↓
Canonical Encoding
  ↓
SHA-384
  ↓
Transaction Hash
```

The transaction hash SHALL be the value signed by the Signer.

---

# 5. Signature Generation Standard

Signing Process:

```text
Transaction
  ↓
Canonical Encoding
  ↓
SHA-384
  ↓
Ed25519 Sign
  ↓
Signature Package
```

Output SHALL include:

- Public key identifier
- Transaction hash
- Signature
- HCOS version

---

# 6. Signature Verification Standard

Verification Process:

```text
Signature Package
  ↓
Recompute Hash
  ↓
Verify Ed25519 Signature
  ↓
Pass / Fail
```

Verification SHALL fail if:

- Signature mismatch
- Hash mismatch
- Incorrect public key
- Unsupported protocol version
- Malformed payload

---

# 7. QR Signing Request Standard

QR transaction requests SHALL use:

```text
HBN-TXREQ:
```

Example payload:

```json
{
  "version":"1.0",
  "network":"hedera-mainnet",
  "operation":"transfer",
  "payload_hash":"..."
}
```

The Signer SHALL verify structure before signing.

---

# 8. QR Signature Response Standard

Signed output SHALL use:

```text
HBN-TXSIG:
```

Example:

```json
{
  "version":"1.0",
  "transaction_hash":"...",
  "signature":"...",
  "public_key":"..."
}
```

---

# 9. Memory Protection Requirements

Sensitive material includes:

- Mnemonics
- Passphrases
- Seeds
- Private keys
- Signing buffers

Implementations SHALL:

- Minimise exposure time
- Clear temporary buffers
- Clear signing buffers after use
- Avoid secret persistence

Implementations SHOULD:

- Lock memory pages where practical
- Reduce swap exposure
- Avoid diagnostic leakage

---

# 10. Secure Logging Rules

The following SHALL NEVER be logged:

- Mnemonics
- Seeds
- Private keys
- Passphrases
- Raw entropy

Permitted logging:

- Timestamp
- Operation type
- Verification result
- Audit hash
- Certificate hash

---

# 11. Signer Security Requirements

The Signer SHALL:

1. Validate input.
2. Reconstruct key material.
3. Produce signature.
4. Return result.
5. Destroy secrets.

Signer state SHALL return to a clean idle state after each operation.

---

# 12. Wallet Verification Requirements

The Wallet SHALL:

- Verify signatures before broadcast.
- Verify path consistency.
- Validate QR payload type.
- Reject malformed transaction requests.

---

# 13. Sentry Verification Requirements

The Sentry SHALL verify:

- Approved algorithms
- Approved derivation path
- Signature validity
- QR prefix compliance
- HCOS version compatibility

Verification outcomes SHALL be written to audit logs.

---

# 14. Failure Handling

Cryptographic failures SHALL result in:

```text
FAILURE
NO SIGNATURE PRODUCED
AUDIT ENTRY GENERATED
```

Automatic retries SHALL NOT occur without user action.

---

# 15. Future Extensions

Future HCOS revisions may introduce:

- Threshold signatures
- Multi-signature workflows
- Hardware secure elements
- Secure enclave support
- Post-quantum migration mechanisms

New algorithms SHALL remain backward compatible where practical.

---

# 16. Compliance Checklist

A compliant implementation SHALL:

- Use Ed25519 signatures
- Use SHA-384 transaction hashing
- Use canonical encoding
- Support HBN-TXREQ
- Support HBN-TXSIG
- Clear sensitive memory
- Prevent secret logging
- Support audit verification
- Support Sentry validation

---

**End of CRYPTOGRAPHIC_OPERATIONS_STANDARD.md**

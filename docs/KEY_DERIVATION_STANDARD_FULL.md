# KEY_DERIVATION_STANDARD.md

# HBarNone Key Derivation Standard (HKDS) v1.0

**Status:** Draft Standard
**Version:** 1.0
**Classification:** Core Cryptographic Standard
**Applies To:** Forge, Wallet, Signer, Sentry, Recovery Tools

---

# 1. Purpose

The HBarNone Key Derivation Standard (HKDS) defines the mandatory process by which cryptographic identities are created, derived, validated, recovered, and managed throughout the HBarNone ecosystem.

The objectives of this standard are to:

- Ensure deterministic key recovery.
- Ensure interoperability with established industry standards.
- Eliminate dependence upon HBarNone software for long-term access to assets.
- Provide a verifiable audit trail for the Sentry subsystem.
- Support future migration to alternative wallet implementations.
- Establish a consistent security model across all HBarNone components.

The mnemonic phrase SHALL be considered the root secret from which all subsequent cryptographic material originates.

---

# 2. Normative Language

The key words SHALL, SHALL NOT, SHOULD, SHOULD NOT and MAY are to be interpreted as described in RFC 2119.

---

# 3. Design Principles

## 3.1 Mnemonic First Principle

A mnemonic phrase SHALL always be produced before any operational private key.

Private keys SHALL NOT be created as standalone random values.

Required sequence:

```text
Entropy
 → BIP-39 Mnemonic
 → BIP-39 Seed
 → Master Key
 → Child Key Derivation
 → Operational Private Key
```

This design guarantees portability and auditability.

## 3.2 Open Standards Principle

Only publicly documented standards SHALL be used.

Approved standards:

- BIP-39
- BIP-32
- SLIP-0010
- PBKDF2-HMAC-SHA512
- Ed25519
- BIP-44 concepts where applicable

Proprietary derivation schemes are prohibited.

## 3.3 Vendor Independence Principle

Recovery of funds SHALL remain possible without HBarNone software.

A user in possession of:

- mnemonic
- optional passphrase
- derivation path

must be able to reconstruct keys using independent implementations.

---

# 4. Entropy Requirements

## 4.1 Source

Entropy SHALL originate from a cryptographically secure operating-system random source.

Examples:

- Linux getrandom()
- Linux /dev/urandom
- Python secrets module

## 4.2 Minimum Entropy

| Mnemonic | Entropy |
|----------|----------|
| 12 words | 128 bits |
| 24 words | 256 bits |

## 4.3 HBarNone Requirement

HBarNone SHALL generate only 24-word mnemonics.

Rationale:

- Higher security margin
- Long-term archival resilience
- Consistent recovery procedures

---

# 5. BIP-39 Mnemonic Standard

## 5.1 Word List

The official BIP-39 English word list SHALL be used.

## 5.2 Checksum Validation

All mnemonic phrases SHALL pass BIP-39 checksum validation.

## 5.3 User Presentation Requirements

The Forge SHALL display the mnemonic in:

- numbered format
- printable format
- exportable format

The mnemonic constitutes the master recovery secret.

---

# 6. Optional Passphrase Support

## 6.1 Support Requirement

The Forge and Wallet SHALL support the optional BIP-39 passphrase.

## 6.2 Storage Requirement

Passphrases SHALL NOT be stored.

## 6.3 User Warning

The system SHALL clearly state:

```text
Loss of passphrase = permanent loss of recovery capability.
```

---

# 7. Seed Generation Standard

Mnemonic conversion SHALL follow BIP-39 exactly.

Algorithm:

```text
PBKDF2-HMAC-SHA512
Iterations: 2048
Output Length: 512 bits
```

Salt:

```text
mnemonic + passphrase
```

Result:

```text
512-bit seed
```

Implementations SHALL produce outputs identical to official BIP-39 test vectors.

---

# 8. Master Key Generation

The 512-bit seed SHALL be converted into a master hierarchical key.

Generation SHALL include:

- Master private key
- Master chain code

Implementations SHALL remain compatible with standard deterministic wallet recovery processes.

---

# 9. Hedera Derivation Standard

## 9.1 Supported Curve

HBarNone SHALL use:

```text
Ed25519
```

## 9.2 Derivation Method

HBarNone SHALL use:

```text
SLIP-0010
```

for hierarchical derivation.

---

# 10. Standard Derivation Path

Default path:

```text
m/44'/3030'/0'/0'/0'
```

Definitions:

| Element | Meaning |
|----------|----------|
| 44' | BIP-44 Purpose |
| 3030' | Hedera Coin Type |
| 0' | Account Group |
| 0' | Wallet |
| 0' | First Key |

---

# 11. Multi-Key Expansion

Subsequent keys SHALL increment only the final index.

Examples:

```text
m/44'/3030'/0'/0'/0'
m/44'/3030'/0'/0'/1'
m/44'/3030'/0'/0'/2'
```

---

# 12. Forge Requirements

The Forge SHALL generate:

- 24-word mnemonic
- Optional passphrase support
- Recovery document
- Certificate document
- QR export capability
- Verification metadata

The Forge SHALL NEVER store:

- mnemonic after session
- passphrase
- seed
- private key

unless operating in an explicitly approved secure profile.

---

# 13. Wallet Requirements

The Wallet SHALL support:

- mnemonic import
- mnemonic validation
- passphrase support
- path override
- deterministic recovery
- offline operation

The Wallet SHALL validate:

- word count
- checksum
- derivation path syntax

---

# 14. Signer Requirements

The Signer SHALL derive keys only when required.

Operational sequence:

1. Load mnemonic.
2. Recreate seed.
3. Derive child key.
4. Sign payload.
5. Clear memory.

Persistent storage of derived secrets is discouraged and SHALL require explicit configuration.

---

# 15. Sentry Verification Requirements

The Sentry SHALL verify:

- BIP-39 compliance
- checksum validity
- approved derivation algorithm
- approved derivation path
- approved curve

Audit output SHALL include:

- timestamp
- HKDS version
- verification result
- audit hash
- operator identity where applicable

The Sentry SHALL NOT record mnemonic material.

---

# 16. Memory Sanitisation Requirements

Sensitive data includes:

- entropy
- mnemonic
- passphrase
- seed
- private key

Implementations SHOULD:

- minimise exposure time
- overwrite buffers
- avoid swap where practical
- avoid crash-dump leakage

Forbidden:

- secret logging
- debug output of key material
- telemetry transmission of secrets

---

# 17. Recovery Standard

Recovery requires only:

- mnemonic
- optional passphrase
- derivation path

No dependence shall exist on:

- HBarNone servers
- cloud services
- certificates
- audit logs

Recovery must remain possible indefinitely.

---

# 18. Conformance Test Requirements

Implementations SHALL successfully:

- validate official BIP-39 vectors
- derive deterministic child keys
- reproduce known HKDS test outputs
- verify derivation path handling
- verify memory-clearing routines where possible

---

# 19. Future Extensions

Future HKDS versions may introduce:

- SLIP-0039 Shamir Secret Sharing
- threshold signatures
- multisignature account generation
- hardware security modules
- secure element support
- post-quantum migration planning
- air-gapped QR recovery workflows

Compatibility with HKDS v1.0 SHOULD be preserved whenever practical.

---

# 20. Compliance Checklist

A compliant implementation SHALL:

- Generate 24-word BIP-39 mnemonics
- Use 256-bit entropy
- Use PBKDF2-HMAC-SHA512
- Produce a 512-bit seed
- Use Ed25519
- Use SLIP-0010
- Support optional passphrases
- Support deterministic recovery
- Use path m/44'/3030'/0'/0'/0'
- Prevent secret leakage
- Support Sentry audit verification
- Remain vendor independent

---

**End of KEY_DERIVATION_STANDARD.md**

# SENTRY_SPECIFICATION.md

# HBarNone Sentry Specification

Version: 1.2
Status: Draft
Author: HBarNone Project
Classification: Security Standard

---

# 1. Purpose

The Sentry is the trust anchor of the HBarNone ecosystem.

Its purpose is to continuously assess the integrity of the Forge environment, maintain an auditable record of inspections and security events, and provide certification services for key generation and key import operations.

The Sentry never generates keys, stores private keys or signs blockchain transactions.

---

# 2. Security Objectives

The Sentry SHALL:

- Inspect the operational environment.
- Detect tampering and policy violations.
- Maintain signed audit records.
- Authorize or deny certification.
- Authorize or deny wallet imports.
- Provide verifiable trust status information.
- Preserve evidence of security incidents.

---

# 3. Trust Model

The Sentry is the authoritative source of certification status.

The Forge may generate certificates only when authorized by the Sentry.

The Signer may display certification information supplied by the Sentry but shall not determine certification state independently.

---

# 4. Certification States

The Sentry shall expose one of the following states.

```text
UNINITIALIZED
CERTIFIED
WARNING
VIOLATION_RECORDED
LOCKED
```

## UNINITIALIZED

The Sentry has not yet completed an inspection cycle.

## CERTIFIED

All required inspections passed and no violations are present.

## WARNING

Non-critical conditions require attention.

## VIOLATION_RECORDED

A security-relevant event has occurred.

## LOCKED

Certification services are disabled pending operator review.

---

# 5. Inspection Requirements

The Sentry SHALL maintain records of:

- Inspection ID
- Timestamp
- Result
- Inspector version
- Violation count
- Certification state

Inspections shall be recorded in the audit log.

---

# 6. Import Authorization

Imported wallets represent elevated risk because the origin of the seed phrase cannot be verified by HBarNone.

The Forge SHALL request authorization from the Sentry before permitting:

- Seed phrase import
- Mnemonic import
- Private-key import

Imports shall only be permitted when:

```text
STATE = CERTIFIED
VIOLATION_COUNT = 0
```

---

# 7. Import Denial Conditions

The Sentry SHALL deny imports when any of the following conditions exist:

- Recorded tamper event
- Failed inspection
- Missing inspection history
- Missing audit records
- Unsigned audit records
- Clock rollback detection
- Non-certified state

The Forge shall display:

```text
IMPORT REFUSED
SYSTEM NOT CERTIFIED
```

No override mechanism shall exist.

---

# 8. Certificate Authorization

The Sentry authorizes two certificate classes.

## Generated Certificates

```text
CERTIFIED_GENERATED
```

Meaning:

> The key was generated entirely inside a certified HBarNone environment.

## Imported Certificates

```text
CERTIFIED_IMPORTED
```

Meaning:

> HBarNone certifies the integrity of the import process only.

---

# 9. Mandatory Imported-Key Disclaimer

For imported certificates the Sentry shall require inclusion of the following statement:

> This key was imported from an external source.
>
> HBarNone certifies that the import occurred within a certified environment and that no known security violations existed at the time of import.
>
> HBarNone does not certify the generation, storage, backup, transport or protection of this key before import.

---

# 10. Audit Log Requirements

The Sentry SHALL record:

- Inspection events
- Certification events
- Security violations
- Import approvals
- Import denials
- Configuration changes
- Operator acknowledgements

Audit records shall never be silently deleted.

---

# 11. Violation Handling

When a violation is detected, the Sentry SHALL:

1. Record the event.
2. Timestamp the event.
3. Update violation counters.
4. Preserve forensic evidence.
5. Recalculate trust status.

Historical violations remain visible even after subsequent inspections succeed.

---

# 12. User Information Display

The Sentry status view shall display:

- Current state
- Violation count
- Inspection count
- Last inspection date
- Last certification date
- Import eligibility status
- Audit-log status

This information shall be available to both operators and the Forge.

---

# 13. Certification Data Provided to Certificates

The Sentry shall provide:

- Certificate ID
- Certification type
- Key origin
- Inspection count
- Certification timestamp
- Current state

for inclusion on printed certificates.

---

# 14. Future Enhancements

Candidate future enhancements include:

- Hardware-backed attestation
- Secure element integration
- Remote audit export packages
- Multi-party inspection approval
- Cryptographic certificate verification

---

**End of SENTRY_SPECIFICATION.md v1.2**

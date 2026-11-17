# HbarNone SENTRY Threat Model

Version: 1.0
Status: Draft

## Purpose

This document defines the threat model for the HbarNone SENTRY, Forge, and Engraver ecosystem.

Primary objective:

- Generate Hedera private keys in a highly controlled offline environment.
- Detect environmental conditions that could compromise key secrecy.
- Prevent creation of permanent physical backups from uncertified environments.
- Provide auditable evidence that a key was generated in a certified environment.

## Assets To Protect

### Critical Assets

- Private Keys
- Recovery QR Codes
- Engraving Toolpaths
- Steel Backup Artefacts
- SENTRY Certificates
- Audit Logs
- Authorization Tokens

### Integrity Assets

- Public Keys
- Public Key Fingerprints
- Account IDs
- Recovery Documents

## Trust Boundaries

### Trusted Components

- SENTRY
- Forge
- Engraver
- Approved Ubuntu Image
- Certified Audit Chain

### Conditionally Trusted Components

- Printer
- USB Boot Media
- External Monitor
- Keyboard
- Mouse

### Untrusted Components

- Internet
- Network Infrastructure
- Cloud Services
- Third-party USB Storage
- External CNC Providers
- Shared Computers

## Security States

### CERTIFIED

Requirements:

- Current audit passes.
- No historical critical failures.
- Audit chain valid.
- Installation integrity intact.

Permissions:

- Forge Enabled
- Certificate Issuance Enabled
- Engraver Enabled

### TAINTED

Triggered by:

- Historical critical failure.
- Security rule violation.

Permissions:

- Forge Enabled
- Certificate Issuance Disabled
- Engraver Disabled

### REVOKED

Triggered by:

- Audit log tampering.
- Broken hash chain.
- Invalid installation state.

Permissions:

- Forge Disabled
- Certificate Issuance Disabled
- Engraver Disabled

## Threat Categories

### Network Compromise

Threat:
Private key exposure through active networking.

Examples:

- Wi-Fi enabled.
- Ethernet connected.
- Bluetooth networking active.
- Mobile broadband active.

Mitigation:

- Continuous network auditing.
- Critical failure logging.
- Environment tainting.

### Remote Access Software

Threat:
Remote observation or control.

Examples:

- SSH
- XRDP
- TeamViewer
- VNC
- AnyDesk

Mitigation:

- Process inspection.
- Service inspection.
- Immediate failure state.

### Storage Leakage

Threat:
Private keys written to unintended storage.

Examples:

- Internal SSD mounted.
- External USB drive attached.
- Automatic backup services.

Mitigation:

- Storage auditing.
- Device monitoring.
- Audit logging.

### Malware Persistence

Threat:
Compromise retained between sessions.

Mitigation:

- Historical logging.
- Installation tainting.
- Fresh-image re-certification process.

### Toolpath Manipulation

Threat:
Generated engraving pattern differs from generated key.

Mitigation:

- End-to-end round-trip verification.
- Public-key validation.
- Fingerprint comparison.

### Printer Compromise

Threat:
Recovery documents stored or transmitted by printer.

Mitigation:

- Printer classification.
- Printer advisory reporting.
- Certificates independent of printer trust.

### Human Error

Threat:
User mistakes resulting in insecure operation.

Examples:

- Booting connected.
- Printing on unsafe devices.
- Using tainted keys.

Mitigation:

- Guided workflows.
- Mandatory warnings.
- Distinct certification states.

## Audit Log Requirements

Each audit entry shall contain:

- Timestamp
- Installation ID
- SENTRY Version
- Audit Result
- Detected Hardware
- Previous Record Hash
- Current Record Hash

Audit records shall be append-only.

## Public-Key Verification

The public key is the canonical verification artefact.

Verification process:

1. Generate private key.
2. Derive public key.
3. Generate fingerprint using SHA-256(public key).
4. Record fingerprint on all recovery documents.
5. During recovery derive public key again.
6. Recalculate fingerprint.
7. Compare values.

Matching fingerprints prove recovered key integrity.

## End-to-End Verification Requirement

Certified keys must pass:

Private Key
→ Public Key
→ QR Code
→ Toolpath
→ Simulated Plate
→ Decoded QR
→ Recovered Private Key
→ Recovered Public Key

Certification may only be issued when original and recovered public keys match exactly.

## Security Principles

1. Never trust current state alone.
2. Security history matters.
3. Audit failures are permanent within an installation.
4. Forge may operate in TAINTED mode for testing.
5. Engraver may only operate in CERTIFIED mode.
6. Steel backups require certified provenance.
7. Public-key verification is preferred over private-key disclosure.

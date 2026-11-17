# AUDIT_LOG_FORMAT.md

# HbarNone Audit Log Format Specification

Version: 1.0
Status: Draft

---

## Purpose

The audit log provides a tamper-evident record of all SENTRY security inspections.

The audit log serves four purposes:

1. Establish trust history.
2. Detect security failures.
3. Detect audit log tampering.
4. Support certificate issuance decisions.

SENTRY certification decisions shall be based on both current audit status and historical audit records.

---

# Design Principles

1. Append-only.
2. Human-readable.
3. Machine-readable.
4. Hash chained.
5. Offline verifiable.
6. Immutable within an installation.
7. Survives reboot and power loss.

---

# Storage Location

Recommended location:

```text
/var/lib/hbarnone/sentry/audit/
```

Primary log file:

```text
audit_log.jsonl
```

One JSON object per line.

---

# Log Record Structure

Each audit generates exactly one audit record.

```json
{
  "record_id": "uuid",
  "timestamp": "2026-11-17T12:00:00Z",
  "installation_id": "HBN-SENTRY-7C9A21D4",
  "sentry_version": "1.0.0",
  "audit_result": "PASS",
  "environment_state": "CERTIFIED",
  "critical_failures": 0,
  "warning_count": 0,
  "previous_hash": "...",
  "current_hash": "..."
}
```

---

# Mandatory Fields

| Field | Description |
|---------|---------|
| record_id | Unique audit identifier |
| timestamp | UTC ISO-8601 timestamp |
| installation_id | Installation identifier |
| sentry_version | Executing SENTRY version |
| audit_result | PASS or FAIL |
| environment_state | UNINITIALIZED, CERTIFIED, TAINTED or REVOKED |
| critical_failures | Count of critical issues |
| warning_count | Count of warnings |
| previous_hash | Hash of previous record |
| current_hash | Hash of current record |

---

# Audit Check Results

Each record shall include detailed check results.

Example:

```json
"checks": {
  "network": "PASS",
  "storage": "PASS",
  "usb": "PASS",
  "entropy": "PASS",
  "remote_access": "PASS",
  "boot_integrity": "PASS"
}
```

---

# Hardware Snapshot

Each audit shall include a hardware snapshot.

```json
"hardware": {
  "cpu": "AMD Ryzen 5 5600U",
  "memory_gb": 16,
  "boot_device": "SanDisk Extreme 128GB",
  "wifi_devices": ["Intel AX210"],
  "bluetooth_devices": ["Intel AX210"],
  "internal_storage": ["Samsung 980 Pro"]
}
```

---

# Critical Failure Events

The following events automatically taint an installation:

- Active Wi-Fi interface
- Active Ethernet connection
- Active Bluetooth networking
- Mounted internal storage
- Active remote access service
- Unauthorised USB mass-storage device

These events shall be permanently recorded.

---

# Hash Chain Format

Each record references the previous record.

```text
Record N Hash
↓
Stored as previous_hash in Record N+1
```

Hash algorithm:

```text
SHA-256
```

The current_hash shall be calculated from the canonical JSON representation of the record excluding current_hash itself.

---

# Genesis Record

The first record created during installation initialization shall contain:

```text
previous_hash = GENESIS
```

This record establishes the root of trust for the installation.

---

# Environment State Rules

Valid states:

```text
UNINITIALIZED
CERTIFIED
TAINTED
REVOKED
```

State transitions shall be governed by STATE_MACHINE.md.

---

# Audit Frequency

Mandatory audits:

- System startup
- Before certificate issuance
- Before Forge operations
- Before Engraver operations

Recommended periodic audit:

- Every 10 minutes during active session

---

# Audit Chain Validation

SENTRY shall verify:

1. Record ordering.
2. Hash continuity.
3. Record integrity.
4. Installation ID consistency.

Failure of any validation step shall move the installation to REVOKED.

---

# Export Format

Users may export an audit report.

Recommended formats:

- JSON
- PDF
- Plain text

Exports are read-only representations and shall never replace the authoritative log.

---

# Retention Policy

Audit logs shall be retained for the life of the installation.

Audit logs shall never be automatically deleted.

---

# Design Principles

1. Trust history is security-critical.
2. Current system state alone is insufficient.
3. All critical failures must remain visible.
4. Audit tampering must be detectable.
5. Certificates depend on audit integrity.
6. Engraver trust depends on audit trust.

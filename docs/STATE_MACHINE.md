# STATE_MACHINE.md

# HbarNone State Machine Specification

Version: 1.0
Status: Draft

---

## Purpose

This document defines the valid operational states of a HbarNone SENTRY installation and the permitted transitions between those states.

The state machine determines:

- Forge permissions
- Certificate issuance permissions
- Engraver permissions
- Recovery eligibility
- Trust status

---

# States

## UNINITIALIZED

Initial state of every new installation.

Characteristics:

- No audit history
- No Installation ID
- No trust history
- No certificates may be issued

Permissions:

- Forge: Disabled
- Certificate Issuance: Disabled
- Engraver: Disabled

---

## CERTIFIED

Trusted operational state.

Requirements:

- Successful audit
- No historical critical failures
- Valid audit chain
- Valid installation integrity

Permissions:

- Forge: Enabled
- Certificate Issuance: Enabled
- Engraver: Enabled

---

## TAINTED

State entered when a critical security violation has been recorded.

A TAINTED installation may still be used for development and testing.

Permissions:

- Forge: Enabled
- Certificate Issuance: Disabled
- Engraver: Disabled

All generated recovery material must display:

NOT CERTIFIED

and include a warning that the environment may have been compromised.

---

## REVOKED

State entered when trust in the audit system itself can no longer be established.

Examples:

- Audit chain corruption
- Missing audit records
- Hash-chain failure
- Installation integrity failure

Permissions:

- Forge: Disabled
- Certificate Issuance: Disabled
- Engraver: Disabled

---

# State Transition Diagram

```text
UNINITIALIZED
      |
      v
CERTIFIED
      |
      v
TAINTED
      |
      v
REVOKED
```

---

# Valid Transitions

## UNINITIALIZED -> CERTIFIED

Conditions:

- Initial audit passes
- Audit chain established
- Installation ID generated

Actions:

- Create Genesis Record
- Create Installation ID
- Begin trust history

---

## CERTIFIED -> TAINTED

Triggered by any critical failure.

Examples:

- Active Wi-Fi
- Active Ethernet
- Active Bluetooth networking
- Mounted internal storage
- Remote access service detected
- Unauthorised USB storage detected

Actions:

- Record critical failure
- Update audit log
- Disable certification
- Disable Engraver

Transition is permanent for the installation.

---

## TAINTED -> REVOKED

Triggered by:

- Audit log corruption
- Invalid audit chain
- Installation integrity failure

Actions:

- Disable all privileged functions
- Prevent key generation
- Prevent certificate issuance

---

# Invalid Transitions

The following transitions are not permitted:

```text
TAINTED -> CERTIFIED
REVOKED -> CERTIFIED
REVOKED -> TAINTED
UNINITIALIZED -> TAINTED
```

Reason:

Trust history must remain immutable.

---

# Re-Certification Process

A TAINTED or REVOKED installation cannot regain trust.

To obtain certification again:

1. Re-image installation media.
2. Install a clean SENTRY environment.
3. Generate a new Installation ID.
4. Create a new Genesis Record.
5. Complete initial certification audit.

This creates an entirely new trust history.

---

# Permission Matrix

| State | Forge | Certificate | Engraver |
|---------|---------|---------|---------|
| UNINITIALIZED | No | No | No |
| CERTIFIED | Yes | Yes | Yes |
| TAINTED | Yes | No | No |
| REVOKED | No | No | No |

---

# Design Principles

1. Trust history is immutable.
2. Critical failures permanently taint an installation.
3. Engraver requires certified provenance.
4. Certificates require certified provenance.
5. Testing remains possible in TAINTED environments.
6. Re-certification requires a fresh installation.
7. SENTRY state changes must always be recorded in the audit log.

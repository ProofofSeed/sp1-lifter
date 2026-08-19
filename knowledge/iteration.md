# Iteration record

Per-milestone as-built record. The plan and pre-registered acceptances live in `milestones.md`; measured S0 numbers in `cycle-law.md`.

## S0 — environment and cycle probe (DONE, merged `91202c5`)

Built the workspace (statement / methods+guest / host), verified genuine hardened-child proofs in the RISC Zero executor, fitted the cycle law (see `cycle-law.md`). Adversarially reviewed; the review's S1-shaping findings are recorded in `milestones.md` ("Standing observations") and the S1 spec was amended to carry the repetition count.

## S1 — full-scale lift and reject matrix

### Design

- The guest split into two bins: `probe` (S0's cycle probe, journal carries diagnostic cycle counts that are host-provided and unattested) and `lift` (the lifter proper, journal = the lifted statement only, no cycle data).
- The lift guest asserts validity before committing — a receipt can only exist for an accepting run — and additionally commits `valid` as a defensive belt. It computes the circuit fingerprint itself (`zkboo-circuit-hash` over the circuit rebuilt from the public parameters); the fingerprint is never host-supplied.
- `LiftJournal` = version, valid, fingerprint, chain code, index, expected output, binding, **num_responses** (soundness-critical: ZKBoo accepts any repetition count, the empty proof included).
- `check_journal(journal, expectation)` in the statement crate is the reference consumer gate with typed refusal reasons (`JournalCheckError`); the S3 on-chain verifier must enforce exactly these checks.
- Tampering for refusal tests is structured (`tamper_and_msg` rebuilds a response with one AND-message bit flipped via the public `Response` constructors), not byte-level frame corruption, so a tampered input decodes cleanly and the refusal is attributable to verification, not to postcard.

### Reject matrix and check-removal validation (2026-08-19)

All six rejects pass via their named refuser, and each distinct refuser was validated by temporarily deleting the check and watching the test go red:

| case | refuser | red-on-removal |
|---|---|---|
| R1 tampered proof (AND-message bit) | guest `assert!(valid, "ZKBoo proof rejected")` | ✓ (R1 red with the assert removed) |
| R2 wrong binding | same guest assert | (shares R1's refuser) |
| R3 wrong fingerprint | `check_journal` FingerprintMismatch | ✓ |
| R4 too few responses (8 < 219) | `check_journal` TooFewResponses | ✓ |
| R5 empty proof (ZKBoo accepts it by design) | `check_journal` TooFewResponses | (shares R4's refuser) |
| R6 wrong statement (expected output) | `check_journal` StatementMismatch | ✓ |
| R7 malformed response (oversized AND-message) | replay shape check ("view replay error") | (fail-closed by construction; added post-review) |

Control case (n=8, matching expectation) accepted throughout.

### Adversarial review (2026-08-19) and remediation

Core soundness confirmed by the reviewer: the fingerprint binds chain code AND index (both enter as constant gates whose *values* the circuit hash ingests, while witness inputs contribute type only); the statement is doubly bound (fingerprint + the global Fiat-Shamir challenge check); `min_responses` as ≥ is right (soundness is monotone in the count, and extra responses cannot be junk — each must match its position in the deterministic challenge sequence); malformed responses all fail closed. Findings acted on:

- **F1 (med-high, interface):** the journal cannot identify the crypto suite or circuit family — only the guest **image id** binds those. `check_journal` now documents receipt-verification-against-the-expected-image-id as a mandatory precondition, and `check_lifted_receipt` (host lib) fuses receipt verify + decode + gate; fixture consumers must use it (F5). The on-chain verifier takes the image id as an explicit argument, consistently.
- **F2 (low):** malformed-response handling was untested — added R7 (oversized AND-message vector → view-replay shape error, fail-closed). ⚠️ Upstream note for zkboo: `Party` deserializes without mod-3 validation; out-of-range challenges reach an `unreachable!()` in the replay backend (a panic, so fail-closed, but worth validating on deserialize).
- **F3 (low):** the journal's response count cast is now a checked `try_into`.

### Full-scale positive run (2026-08-19)

n = 219 (classical-128): ZKBoo proving 104 ms (parallel, host-native), guest accepted, journal passed the consumer gate (fingerprint `33e1f70b…`, 219 responses), **30 623 354 729 user cycles**, executor wall 227.06 s. The measured total sits within ~1.5% of the S0 law's 3.016e10 extrapolation; the excess is the in-guest circuit-fingerprinting pass and frame decode, which the S0 `verify_cycles` bracket excluded. Acceptance re-run green on the final milestone tree after a host-side helper refactor.

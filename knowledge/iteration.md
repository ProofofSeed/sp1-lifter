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

## S2 — succinct receipt

### Environment findings (2026-08-19)

- **The rzup-distributed `r0vm` for aarch64-apple-darwin is CPU-only** (no Metal strings in the binary; observed 1571% CPU). An un-accelerated n=1 succinct prove (138M cycles, ~133 segments + recursion) had not finished after 55 minutes and was abandoned — the un-accelerated CPU path extrapolates to multiple days at 219 and is **infeasible**; this triggered the plan's Keccak-acceleration contingency.
- **Keccak acceleration adopted** (guest-only `tiny-keccak` patch to `risc0/tiny-keccak` tag `tiny-keccak/v2.0.2-risczero.0`): guest cycles drop 3.45× (see `cycle-law.md`). The S1 acceptance (7-way reject matrix + full-scale positive) re-ran green on the accelerated guest — same circuit fingerprint (`33e1f70b…`, the fingerprint is of the ZKBoo circuit, not the guest), new image id.
- **Metal proving requires the full-Xcode Metal shader compiler**, which is not installed (Command Line Tools only; `risc0-sys` metal build fails at `xcrun metal`). Not adopted unilaterally — a ~12 GB Xcode install is Stefano's call; the S2 acceptance (receipt verifies) does not require Metal. Flagged as the obvious speed lever if proving times matter later.
- Segment size is tunable via `ExecutorEnv::segment_limit_po2` (s2-prove's second argument); larger segments cut the per-segment lift/join recursion count, which CPU profiling suggests dominates.

### Proving measurements and the stack switch (2026-08-19)

- risc0, un-accelerated guest, n=1 (138M cycles), CPU r0vm: **>55 min, abandoned**.
- risc0, accelerated guest, n=1 (41M cycles), in-process prover (CPU segments + Metal-recursion feature compiled in; GPU observed at 0% throughout): **aborted at 45 min 27 s, unfinished** ⇒ >150 h extrapolated at 219. The `metal` feature is vestigial for segment proving in the 3.x line: `risc0-circuit-rv32im` 4.x has only `cpu.rs`/`cuda.rs` HALs (verified at source after the GPU sat idle); only `risc0-circuit-recursion` has a Metal HAL. The Xcode/Metal-toolchain install (license + `xcodebuild -downloadComponent MetalToolchain`) predated this finding.
- risc0's Groth16 wrap: x86-only rapidsnark, unavailable on this machine even via Docker.
- **SP1 6.0.1, practically verified on this M4 Max:** template fibonacci core→compress→shrink→**native Groth16 wrap** (no Docker, `native-gnark`) succeeded — real EVM proof + fixture, 21.7 min wall including first-time SDK/gnark builds and artifact downloads, peak RSS 28 GB. One dependency nit: sp1-prover 6.0.1 needs `tempfile ≥ 3.20` (`TempDir::keep`), fixed by `cargo update -p tempfile`.
- **Stefano ratified the switch to SP1** (see the amendment in `milestones.md`) with the constraint that the whole chain runs on this machine. Port record continues below.

## SP1 port

### S1 re-established on SP1 (2026-08-19)

- Instruction law (whole-program, includes decode + fingerprint): **`≈ 3.09e7 + 2.635e7 × n`** ⇒ **5.80e9 @ 219** (measured full-scale: 5 801 478 472, matching the law; executor wall 37.4 s). 35% below the risc0-accelerated figure — the SP1 Keccak precompile plus the 64-bit RISC-V target (native u64 ops for the replay).
- Seven-way reject matrix green with named refusers; all four distinct refusers re-validated by check removal on the SP1 build.
- **SP1 semantic handled explicitly:** panicked executions return `Ok` from the executor with a non-zero exit code and are *provable* under a non-success status. Refusals therefore commit a diagnostic tag naming the refuser before panicking; `execute_lift` enforces exit 0; `client.verify(..., None)` requires success; and the v6.1.0 on-chain verifier enforces `exitCode == 0` itself (`InvalidExitCode`). Success-status is part of every gate layer.

### S2 proving law (compressed, CPU, this machine, 2026-08-19)

| n | proving wall | peak RSS | proof size |
|---|---|---|---|
| 1 | 325.0 s | 107.4 GB | 1 272 754 B |
| 8 | 1 162.5 s | 109.6 GB | 1 272 754 B |

**Law: `≈ 205 s + 119.6 s × n` ⇒ full 219 ≈ 7.3 h.** Marginal rate consistent across both points (~210 kHz effective). **Memory is scale-independent** (phase-oscillating 37–110 GB band at both n) and the compressed proof is constant-size — the succinct property, delivered. The S3 Groth16 pipeline re-runs core+compress internally (~same cost) plus shrink+wrap (template-measured ~minutes at ≤28 GB); the two full-scale runs must be strictly sequential (each peaks ~110 GB of the 137 GB).

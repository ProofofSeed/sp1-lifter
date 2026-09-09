# Measured guest cycle law

> **Every figure below was measured against the withdrawn ZKBoo 2.0.0 and is superseded.**
> The move to zkboo 1.2.0 changed every proof, every circuit fingerprint and every guest verifying key.
> Kept for the method and the shape of the results, not for the numbers.
> Do not quote them, and do not rescale them by a factor seen elsewhere.
>
> **Current figures: `performance-table.md`, the "Re-measured on zkboo 1.2.0" section**, taken 2026-09-07.
> What the port cost and what is now closed: `zkboo-1.2.0-migration.md`.
> Why a superseded number may not be rescaled into a current one, measured rather than asserted: `instructions-are-not-seconds.md`.

**AMENDED (2026-08-19, Keccak acceleration):** with the guest's `tiny-keccak` patched to RISC Zero's accelerated fork (`tiny-keccak/v2.0.2-risczero.0`, the audited Keccak-precompile path; guest workspace only, digests identical), the law becomes **`verify_cycles ≈ 3.994e7 × n_responses`** (sweep 1/2/4/8, linear, negligible intercept) — a **3.45× reduction**, i.e. software Keccak (HashPRG tape regeneration + view commitments) was ~71% of the un-accelerated guest's cycles.
Extrapolations: **8.75e9 @ 219**, 1.75e10 @ 438; measured full-scale S1 run: **8.99e9 user cycles**, executor wall 71.7 s.
The Keccak permutations move to the dedicated Keccak circuit (proven via union recursion), so proving cost shifts partly there rather than vanishing — the proving throughput numbers in `iteration.md` §S2 are the ground truth.
The un-accelerated law below stands as the baseline record.

Measured 2026-08-19 on the 16-core Apple Silicon machine (137 GB), RISC Zero executor (no proving), cargo-risczero 3.0.6 / r0vm 3.0.6 / risc0 rust 1.97.0, release build.
Statement: BIP-32 hardened child (test vector 1, m/0H), Keccak-256 suite, genuine proofs generated and natively verified on the host before entering the guest.
`verify_cycles` brackets `zkboo::verifier::verify` alone (frame decode and circuit construction excluded); `total` is the summed segment **user** cycle count for the whole session (see the caveat below).
Table from the final S0 run, after the frame-version tag was added.

| n (responses) | frame bytes | verify cycles | total cycles  | executor wall |
| ------------- | ----------- | ------------- | ------------- | ------------- |
| 1             | 70 743      | 137 773 568   | 138 651 488   | 1.06 s        |
| 2             | 141 174     | 275 492 755   | 277 238 892   | 2.05 s        |
| 4             | 282 418     | 550 971 056   | 554 457 430   | 4.09 s        |
| 8             | 564 677     | 1 101 864 220 | 1 108 830 075 | 8.11 s        |

**Fitted law: `verify_cycles ≈ 4.6e4 + 1.377e8 × n_responses`** — linear to well under 0.1%, intercept negligible, and the non-verify overhead (decode + startup) is under 0.7% of the total.

Extrapolations:

- **classical-128 (219 responses): 3.016e10 cycles**
- PQ-128 (438 responses): 6.033e10 cycles

Implications:

- ~1.38e8 cycles to verify a ~58 KiB response is ~2.4k cycles/byte, which points at the software Keccak (`tiny-keccak`) in the HashPRG tape regeneration and view commitments as the dominant cost.
  RISC Zero ships a Keccak accelerator (guest syscall); evaluating it is the first optimisation lever if S2's proving times make 3e10 cycles uncomfortable.
  Measure the Keccak share before building anything.
-  These are **user** cycles: `SegmentInfo::cycles` excludes continuation and power-of-two padding overhead, and S2 proving cost per segment is `2^po2`.
   Price S2 from a measured proving run, not from these totals.
   `env::cycle_count()` is host-provided and unattested by the zkVM circuit — fine for this self-run probe, never to be committed to a journal as trusted data.
- The probe only exercises accepting runs; rejecting inputs (tampered proof, wrong binding, wrong statement, too-few-responses) are S1's acceptance, not established here.
  Note `zkboo::verify` accepts the **empty proof** as valid — the repetition count is part of the statement (see the S1 spec).

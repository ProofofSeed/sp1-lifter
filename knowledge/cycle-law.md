# Measured guest cycle law

Measured 2026-08-19 on the 16-core Apple Silicon machine (137 GB), RISC Zero executor (no proving), cargo-risczero 3.0.6 / r0vm 3.0.6 / risc0 rust 1.97.0, release build.
Statement: BIP-32 hardened child (test vector 1, m/0H), Keccak-256 suite, genuine proofs generated and natively verified on the host before entering the guest.
Guest ELF 575 224 bytes; `verify_cycles` brackets `zkboo::verifier::verify` alone (frame decode and circuit construction excluded); `total` is the summed segment cycle count for the whole session.

| n (responses) | frame bytes | verify cycles | total cycles | executor wall |
|---|---|---|---|---|
| 1 | 70 742 | 137 775 176 | 138 653 160 | 1.05 s |
| 2 | 141 173 | 275 495 951 | 277 242 152 | 2.04 s |
| 4 | 282 417 | 550 977 429 | 554 463 867 | 4.07 s |
| 8 | 564 676 | 1 101 876 946 | 1 108 842 854 | 8.15 s |

**Fitted law: `verify_cycles ≈ 4.6e4 + 1.377e8 × n_responses`** — linear to well under 0.1%, intercept negligible, and the non-verify overhead (decode + startup) is under 0.7% of the total.

Extrapolations:

- **classical-128 (219 responses): 3.016e10 cycles**
- PQ-128 (438 responses): 6.033e10 cycles

Implications:

- ~1.38e8 cycles to verify a ~58 KiB response is ~2.4k cycles/byte, which points at the software Keccak (`tiny-keccak`) in the HashPRG tape regeneration and view commitments as the dominant cost. RISC Zero ships a Keccak accelerator (guest syscall); evaluating it is the first optimisation lever if S2's proving times make 3e10 cycles uncomfortable. Measure the Keccak share before building anything.
- The probe only exercises accepting runs; rejecting inputs (tampered proof, wrong binding, wrong statement) are S1's acceptance, not established here.

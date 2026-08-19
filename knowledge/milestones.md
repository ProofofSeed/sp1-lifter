# Milestone plan

Ratified by Stefano on 2026-08-19, together with the technology choice: **RISC Zero zkVM**, guest = the genuine `zkboo` verifier, universal Groth16 wrap, on-chain verification via RISC Zero's audited BN254 verifier contract.
Each milestone has a pre-registered binary acceptance and is only done when that acceptance passes; milestones are never silently split.
Work happens on a kebab-case side branch off `main`, merged `--no-ff` when the acceptance is green.

## Security frame (ratified 2026-08-19)

This track targets **classical** security only; post-quantum resistance belongs to the companion STARK lifter.

- **N_ε = 219** is the default repetition count: (2/3)^219 ≤ 2^-128, which is classical 128-bit in the forgery-work-ratio sense (a T-query Fiat-Shamir attacker forges with probability ≤ T·(2/3)^N, so work/success ≥ 2^128). Stricter definitions would rate it lower — an attacker investing 2^100 work succeeds with probability ~2^-28 — and every write-up must carry that caveat. 438 remains the PQ-128 figure (project convention: a PQ target needs 2× classical bits on every axis).
- N_ε is a runtime parameter throughout; nothing pins 219.
- **The end-to-end ceiling is BN254**: post-Kim–Barbulescu estimates put the pairing security of the Groth16 layer around ~100 bits, and every Groth16 verifier on L1 shares it. On this track the ZKBoo repetition count is not the weakest link, and claims must say so.

## S0 — Environment and cycle probe

Toolchain installed (cargo-risczero 3.0.6, r0vm 3.0.6, risc0 rust 1.97.0; crates pinned at 3.0.6); a guest that is a thin shim around `zkboo::verifier::verify` runs a genuine hardened-child proof (BIP-32 test vector 1, m/0H, Keccak-256 suite) inside the RISC Zero executor.

**Acceptance:** the real proof verifies in-guest at a sweep of repetition counts, and a fitted linear cycle law extrapolates the verification cost at 219 and 438 responses.

## S1 — Full-scale guest verification and statement exposure

The guest verifies a full N_ε = 219 hardened-child proof and commits to the journal: the circuit fingerprint (`zkboo-circuit-hash`), the public circuit parameters, the expected output, the binding, and the verdict.

**Acceptance:** the journal matches host-computed values, and each of tampered-proof / wrong-binding / wrong-fingerprint is rejected, with every refusal test validated by removing the corresponding check and watching the test go red.

## S2 — Succinct receipt, locally

Real STARK proving of the S1 guest run on this machine (Apple Silicon, Metal), with time and memory measured.

**Acceptance:** the succinct receipt for a full-scale S1 run verifies via the risc0 host-side verifier.

## S3 — Groth16 wrap and on-chain gas

The Docker question for the local STARK→Groth16 wrap is resolved (accept Docker for that one step, use the Bonsai/prover network, or another route — Stefano decides when the options are priced); a Foundry test verifies the wrapped proof against RISC Zero's audited verifier contract.

**Acceptance:** a real proof-of-seed lift verifies on-chain (forge) at measured gas in the expected ~250–300k range, plus a reject matrix (wrong journal, wrong image id, corrupted seal).

## Later, separately probed

- The secp256k1 elephant statements (~42 MB/response ⇒ ~9 GB of proof data through the guest at 219): probe guest cycles on a single response before committing to anything.
- The BIP-39 seed statement (58 KiB × 2048 rounds per response).
- A minimal ERC-4337 / EIP-7702 account contract consuming the lift, with binding = userOp hash.
- Keccak acceleration in-guest (RISC Zero's keccak syscall) if the cycle law makes it worthwhile.

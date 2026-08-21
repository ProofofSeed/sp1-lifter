# Milestone plan

Ratified by Stefano on 2026-08-19, together with the technology choice: **RISC Zero zkVM**, guest = the genuine `zkboo` verifier, universal Groth16 wrap, on-chain verification via RISC Zero's audited BN254 verifier contract.

## ⚠️ AMENDMENT (ratified by Stefano, 2026-08-19, same day): the stack is now SP1

**The zkVM is SP1 (sdk 6.0.1), not RISC Zero.** The operative constraint is that everything must run on this machine (M4 Max, 137 GB; the x86 server is not available), and under it the original choice fails on measured facts:

- The Metal GPU proving that tie-broke for RISC Zero **does not exist in its 3.x line**: `risc0-circuit-rv32im` 4.x ships CPU and CUDA segment HALs only (Metal survives only for the recursion circuit). The claim came from the 1.x-era circuit and was verified at feature-flag level, not implementation level — a recorded process failure.
- Measured on this machine: a single accelerated response (41M cycles) had not finished succinct-proving after **45 min 27 s** (aborted) ⇒ >150 h at 219. The un-accelerated attempt was earlier abandoned at >55 min.
- RISC Zero's Groth16 wrap is **architecturally impossible here** (x86-only rapidsnark, "even via Docker").
- SP1, practically verified on this machine the same day: **the native Groth16 wrap works locally** (`native-gnark`, pure-Go gnark, no Docker) — the template's full core→compress→shrink→wrap chain produced a real EVM proof + Foundry fixture in 21.7 min *including* first-time builds and artifact downloads, peak RSS 28 GB. `sp1-patches/tiny-keccak` exists at `patch-2.0.2-sp1-6.0.0`, the same drop-in Keccak-precompile mechanism already validated with the zkboo hasher on risc0.

Everything stack-agnostic carries over unchanged: the statement crate (`LiftInput`/`LiftJournal`/`check_journal`), the security frame, the acceptance definitions, and the reject-matrix discipline. The image-id precondition of the journal gate maps to SP1's **verifying-key** check. S1's acceptance re-runs on the SP1 guest; S2's "succinct receipt" is SP1's **compressed** proof; S3's wrap-venue decision gate is **resolved**: local native-gnark wrap on this machine — no Docker, no external service. The Metal parenthetical in S2 is void for both stacks and is struck.

The RISC Zero implementation (through the Keccak-accelerated guest and the S1 re-acceptance on it) remains in git history for reference; its measured numbers stand in `cycle-law.md` and `iteration.md` as the comparison record.
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

The guest verifies a full N_ε = 219 hardened-child proof and commits to the journal: the circuit fingerprint (`zkboo-circuit-hash`), the public circuit parameters, the expected output, the binding, **the repetition count**, and the verdict.

⚠️ The repetition count is soundness-critical, not bookkeeping: `zkboo::verify` accepts a proof with any number of responses — the empty proof verifies, and a k-response proof has soundness only (2/3)^k — so a journal without it would attest "a proof for this statement verified" at prover-chosen soundness (adversarial review finding F1, 2026-08-19). The verifier of the journal must check the count against its required N_ε (or the guest must enforce a minimum committed in the journal).

**Acceptance:** the journal matches host-computed values, and each of tampered-proof / wrong-binding / wrong-fingerprint / **too-few-responses (including the empty proof)** is rejected, with every refusal test validated by removing the corresponding check and watching the test go red.

## S2 — Succinct receipt, locally

Real STARK proving of the S1 guest run on this machine (Apple Silicon, Metal), with time and memory measured.

**Acceptance:** the succinct receipt for a full-scale S1 run verifies via the risc0 host-side verifier.

## S3 — Groth16 wrap and on-chain gas

The Docker question for the local STARK→Groth16 wrap is resolved (accept Docker for that one step, use the Bonsai/prover network, or another route — Stefano decides when the options are priced); a Foundry test verifies the wrapped proof against RISC Zero's audited verifier contract.

**Acceptance:** a real proof-of-seed lift verifies on-chain (forge) at measured gas in the expected ~250–300k range, plus a reject matrix (wrong journal, wrong image id, corrupted seal).

## S4 — One-pass proving and a measured prover configuration

The first full-scale chain cost 12 h 48 m for 6 h 26 m of work: `s2-prove` and `s3-wrap` were run as separate acceptances, and the wrap driver re-proved core and compression from scratch rather than consuming the compressed proof the previous leg had just produced.
The wrap proper is ~171 s of that (gnark itself ~26 s); everything else is the one proving pass.
This milestone makes the single pass the only path, and replaces the assumption that SP1's stock prover configuration is the right one with a measurement.

Scope: a single `lift` driver that proves once and emits both the proof and the Foundry fixture; SP1's tracing subscriber installed so runs report their phase structure; the prover knobs the SP1 code actually reads (`SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD`, `TRACE_CHUNK_SLOTS` — all read by `SP1CoreOpts::default()` from the environment, hence tunable without a rebuild) swept at a fixed statement size; and the chosen configuration recorded alongside every timing it produced.

⚠️ Two knobs that look like levers are not: `full_size_shards` is referenced only in `opts.rs` and never read on the CPU path, and `drop_ldes`/`recompute_gkr_trace` are hardcoded `false` and trade memory *for* recompute.

**Acceptance:** a single-process `lift 219` produces a Groth16 proof that passes the full relying-party gate and verifies under `forge` at recorded gas; its log records the prover configuration it ran under; and `knowledge/` carries the sweep record — wall time and peak RSS for every configuration tried — with the configuration used for the full run justified by that measurement rather than by assumption.

⚠️ The full-scale run must not be made against an uncommitted `<zkboo>` working tree: the guest ELF depends on it, so the run's verifying key and fixture would not be attributable to any recorded revision.

## Later, separately probed

- The secp256k1 elephant statements (~42 MB/response ⇒ ~9 GB of proof data through the guest at 219): probe guest cycles on a single response before committing to anything.
- The BIP-39 seed statement (58 KiB × 2048 rounds per response).
- A minimal ERC-4337 / EIP-7702 account contract consuming the lift, with binding = userOp hash.
- Keccak acceleration in-guest (RISC Zero's keccak syscall) if the cycle law makes it worthwhile.

## Standing observations from the S0 adversarial review (2026-08-19)

- The `LiftInput` frame layout depends on cargo feature unification of zkboo's word features, resolved independently in the host workspace and the guest's own workspace. The frame now carries a version tag, and the word features are pinned explicitly in `statement/Cargo.toml`; any feature change must bump `LIFT_FRAME_VERSION`.
- `SegmentInfo::cycles` is user cycles without continuation/po2-padding overhead; S2 proving cost per segment is `2^po2`, so S2 pricing must not reuse the S0 totals as-is. `env::cycle_count()` is host-provided and unattested — `verify_cycles` must never appear in a journal as trusted data.
- zkboo encoding malleability (no soundness impact): a response's short `and_msg` vector is zero-filled by `ResponseReplaySource::next_and_msg` without failing `check_consumed`, so distinct proof byte-encodings can verify identically. Any future milestone that hashes or dedupes proof bytes must canonicalise first.

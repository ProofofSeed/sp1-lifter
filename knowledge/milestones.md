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

⚠️ Three knobs look like levers and are not: `full_size_shards`, `drop_ldes` and `recompute_gkr_trace` are each referenced only in `opts.rs` and never consulted anywhere in the 6.4.0 prover. (An earlier draft of this line credited the latter two with trading memory for recompute, which is what their docstrings imply and what nothing in the code does — the Metal lesson, repeated one file away from where it was being applied.)

**Acceptance:** a single-process `lift 219` produces a Groth16 proof that passes the full relying-party gate and verifies under `forge` at recorded gas; its log records the prover configuration it ran under; and `knowledge/` carries the sweep record — wall time and peak RSS for every configuration tried — with the configuration used for the full run justified by that measurement rather than by assumption.

⚠️ The full-scale run must not be made against an uncommitted `<zkboo>` working tree: the guest ELF depends on it, so the run's verifying key and fixture would not be attributable to any recorded revision.

**DONE (2026-08-21).** A single-process `lift 219 groth16` on the released zkboo 1.0.0 tree proved the full lift in **23 320.2 s (6 h 28 m 40 s)**, peak RSS 112.2 GB, and its 356-byte proof verified under `forge` at **270 441 gas, 5/5**, having passed the full relying-party gate and every fixture guard. Against the previous two-run chain of 46 079 s that is a factor of **1.98**, and all of it comes from proving once instead of twice.

The acceptance's final clause — that the configuration be justified by measurement rather than assumption — **failed on its own terms, and the failure is the milestone's most useful output.** The configuration was chosen from an n=8 sweep showing 9.5%; at n=219 it returned −0.84%, and a second knob tested directly at full scale returned 0.54%. Both are noise. A short run has around a dozen shards against a full run's ~280, so the scheduling knobs were measured on a workload that does not resemble the one they were chosen for. **Small runs screen candidates; they do not size them** — and no prover-configuration claim should be believed here until it has been measured at a shard count within reach of the real one.

## S5 — The released ZKBoo, GPU proving, and the measurements that were owed

Ratified 2026-08-27. `knowledge/upgrade-and-metal-plan.md` is the plan as written; this is its
acceptance.

Three things changed underneath the lifter at once, and each carried a measurement the repository
had promised and not taken. ZKBoo released 2.0.0, a breaking release that invalidates every proof
1.x made and shrinks the elliptic-curve statements almost fourfold. A Metal backend for SP1
Hypercube appeared, which is the first GPU proving available on this machine. And the split
probe's timings had been discarded for having been taken on a loaded machine, leaving `root(N)`
unmeasured at the count that decides whether the split is worth building.

**Acceptance, all of it:**

1. Every ZKBoo crate resolves from crates.io at the required version, with the build refusing
   anything else, and both arms of that refusal demonstrated red.
2. On zkboo 2.0.0: the reject matrix 8/8 with a refusal validated by check-removal; the cross-leaf
   gate matrix 9/9 with its control accepting; the circuit fingerprint shown to have moved; and a
   lift verifying under `forge` at recorded gas.
3. The GPU path verified rather than assumed — every comparable phase run both ways and compared
   element for element, and the verifying key and public values byte-identical across arms — and its
   speedup quoted against the *published* prover, not only against its own CPU arm.
4. `root(N)` measured at two counts on a quiet machine, one of them large enough that reaching the
   classical-128 count is an extrapolation of a few times rather than a few tens.
5. A full-scale `lift 219 groth16` passing the relying-party gate and `forge`, at recorded gas,
   under the new verifying key.
6. The elephant instruction law refitted against the current circuit, from a sweep rather than from
   a single point.

**DONE (2026-08-28), with one finding that changes what the middle of it is worth.**

The upgrade cost no source change at all: the assertion word reaches the expected output through
`exec`, and every version tag whose contract was not breached stayed where it was. The fingerprint
moved to `d4825905…`, a full-scale execution costs 4,775,013,465 instructions against 4,773,338,656
before — a thirtieth of a percent — and on-chain verification is unchanged at 270,441 gas.

**The GPU is 4.7× faster than this machine's CPU prover, and cannot be left running unattended.**
It produces a proof that fails its own verification once every twenty-eight, then parks the run
for ever; nothing is at risk but the run finishing, and nothing is printed unless `RUST_LOG` is on.
The fault was narrowed to the one accelerated phase with no comparison mode, and leaving that phase
on the CPU has run 115 consecutive proofs clean at 1.92×. `knowledge/gpu-corruption.md` records the
narrowing and what a fix would need; every long run here now uses the safe setting.

`root(N)` is `184.2 s + 5.36 s × N` on the CPU and `102.0 s + 2.55 s × N` on the safe GPU, fitted
over 4 to 64. The discarded fit's slope was right to 1.3%, which is recorded and is not a reason to
have believed it: it was a 27× extrapolation from a machine under load.

## S6 — An elephant leaf and root, measured

On the slate 2026-08-28. The split has only ever run the hardened-child statement, and the two
figures that matter most about the Ethereum-address statement are projections from it: ~1.5 h for a
leaf and ~4.6 h of wall clock given enough machines. Both come from applying a refitted instruction
law and a proving rate measured on a different statement. One leaf and one root would make them
measurements.

**No device is needed.** The lifter generates its own ZKBoo proofs natively from a test private key;
the Flex never enters the loop.

Scope: the leaf and root programs are compiled against `HardenedChildCircuit`, and the elephant
statement has a different shape — no public parameters, a 20-byte address as output — so it needs its
own leaf and root guests, its own frame and journal types, and a statement digest over its own
public data. Kept outside the production statement crate, as the elephant probe already is, so that
probing cannot move the pinned verifying key.

**Acceptance:** the cross-leaf gate matrix passes on the elephant statement with its control
accepting; one elephant leaf and one elephant root over a small number of leaves are proved and
verified, with wall clock and peak RSS recorded; and the root's journal passes the same
relying-party gate the monolithic lift's does.

Expect roughly 1.5 h per leaf and ~3 h for the root's fingerprint pass, so about six hours of runs
for a leaf plus a small-N root — and the fixed term is the thing to watch, since on this statement
it is two thirds of a single-response run and it is paid once by the root rather than once per leaf.

**✅ ACCEPTED, 2026-08-29.** Every clause of the acceptance met, on the safe-GPU configuration:

| acceptance clause | result |
|---|---|
| cross-leaf gate matrix on the elephant, control accepting | **PASS** — nine refusal cases at 24 leaves, control accepted |
| one elephant leaf proved and verified, wall clock and peak RSS | **4,207.74 s (1 h 10 m 08 s), 73.38 GB** |
| one elephant root over a small N proved and verified | **15,589.86 s (4 h 19 m 50 s), 74.37 GB**, gate green over 2 |
| root journal passes the relying-party gate | **PASS** — `check_elephant_journal`, fingerprint `4d6c3501…` |

Logs: `fixtures/elephant-gate.summary.log`, `elephant-leaf-retime.summary.log`,
`elephant-root-n2.summary.log`, `elephant-execute-gate.summary.log`.

**The projections it replaced were wrong in both directions** — the leaf 1.26× under, the root 1.40×
over — which is the argument for having run it rather than scaling the hardened child. The fixed term
was indeed the thing to watch, and it is worse than expected: **the root is 79% of the wall clock**,
so the next lever on this statement is the fingerprint pass, not more machines. See
`knowledge/elephant-cost.md`.

⚠️ **Two leaf timings taken during the campaign are retracted** — a sibling session was proving on
the same machine — and one of them agreed with the projection to 3% while doing so. The gate matrix
was also extended from five refusal cases to the full nine while accepting this milestone; five met
the acceptance as worded, but "the gate matrix passes" reads stronger than five cases earn.

## 🔵 TODO — reinvestigate removing Poseidon

**Ruled 2026-08-28 (Stefano): keep Poseidon for now.** Recorded here so the question is not lost.

The investigation is `knowledge/poseidon-exposure.md`. In short: none of the exposure is ours — ZKBoo
and the device are Keccak-256 and untouched — and all of it is SP1's, where Poseidon2 is the trace
commitment, the sponge, the Fiat-Shamir challenger, the deferred-proof chain the split rests on, and
a second instantiation over BN254 for the wrap. A break would not leak a seed; it would let someone
forge a lift.

Swapping it is not a configuration change. The `IopCtx` abstraction is the right shape and the
instantiation is a one-line alias, but the recursion layer verifies the hash *in circuit*, with
Poseidon2 in the recursion VM's instruction set as chips of its own. And a bit-oriented replacement is
out on measured grounds: **63,192 trace cells per Keccak-f permutation against 179 for Poseidon2 —
353× per permutation, 83× per absorbed byte** (`fixtures/probe-air-width.log`).

**What to do when this is picked up again, in order:**

1. **Get the actual claim, with a reference.** Four readings — Poseidon v1 rather than Poseidon2,
   particular parameter sets, the BN254 instantiation only, or algebraic hashes in general — lead to
   four different answers, and three of them make replacing the hash the wrong move. Nothing should
   be scoped before this.
2. Only then choose between re-parameterising, a different algebraic hash (none ships in the `slop`
   tree, so it would have to be written), or a change of proving stack.
3. Whatever is decided applies to the **STARK lifter** too, which ratified Poseidon2-inner and has no
   BN254 layer beneath which part of the question is moot.

## Later, separately probed

- ~~The secp256k1 elephant statements.~~ **Probed 2026-08-22, refitted 2026-08-28 — see `elephant-cost.md`.** The instruction law is now `5.1004e9 + 2.4627e9 × n`, fitted over four response counts to within 0.002%, against `2.007e10 + 1.042e10 × n` before. **The ruling that this statement is not liftable is withdrawn**: ≈130 days became ≈13.6, of which 4.23× is zkboo's circuit and 2.27× is GPU proving, and the two terms of the law did not scale together — which is why the note said to refit rather than divide. The gap is one order of magnitude, not two, and the split that divides it across machines is built and gated but has never been run on this statement. Building an elephant leaf and root is the next thing worth doing.
- The BIP-39 seed statement (58 KiB × 2048 rounds per response).
- A minimal ERC-4337 / EIP-7702 account contract consuming the lift, with binding = userOp hash.
- Keccak acceleration in-guest (RISC Zero's keccak syscall) if the cycle law makes it worthwhile.

## Standing observations from the S0 adversarial review (2026-08-19)

- The `LiftInput` frame layout depends on cargo feature unification of zkboo's word features, resolved independently in the host workspace and the guest's own workspace. The frame now carries a version tag, and the word features are pinned explicitly in `statement/Cargo.toml`; any feature change must bump `LIFT_FRAME_VERSION`.
- `SegmentInfo::cycles` is user cycles without continuation/po2-padding overhead; S2 proving cost per segment is `2^po2`, so S2 pricing must not reuse the S0 totals as-is. `env::cycle_count()` is host-provided and unattested — `verify_cycles` must never appear in a journal as trusted data.
- zkboo encoding malleability (no soundness impact): a response's short `and_msg` vector is zero-filled by `ResponseReplaySource::next_and_msg` without failing `check_consumed`, so distinct proof byte-encodings can verify identically. Any future milestone that hashes or dedupes proof bytes must canonicalise first.

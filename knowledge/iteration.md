# Iteration record

> 🚨 **Every figure below was measured against the withdrawn ZKBoo 2.0.0 and is superseded.**
> The move to zkboo 1.2.0 changed every proof, every circuit fingerprint and every guest
> verifying key. Kept for the method and the shape of the results, not for the numbers. Do not
> quote them, and do not rescale them by a factor seen elsewhere.
>
> ➡️ **Current figures: `performance-table.md`, the "Re-measured on zkboo 1.2.0" section**, taken
> 2026-09-07. What the port cost and what is now closed: `zkboo-1.2.0-migration.md`. Why a
> superseded number may not be rescaled into a current one, measured rather than asserted:
> `instructions-are-not-seconds.md`.

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
- (RISC Zero era, superseded) Segment size was tunable via `ExecutorEnv::segment_limit_po2`; larger segments cut the per-segment lift/join recursion count. Neither the API nor that driver exists under SP1 — the equivalent knobs are `ELEMENT_THRESHOLD`/`HEIGHT_THRESHOLD`, swept below, and `lift`'s second argument is the proof mode.

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

**Standing constraint (Stefano, 2026-08-19): use at most ~75% of the machine's CPU** — all proving/build invocations run with `RAYON_NUM_THREADS=12` and `GOMAXPROCS=12` (16-core machine; observed holding at ~1120% of the ~1200% ceiling). Scales the full-scale estimates by ~4/3: **~9.8 h per full 219 run, ~20 h for the sequential S2+S3 chain.**

### Adversarial review of the SP1 port (2026-08-19): NO soundness defects

The reviewer verified in SDK/executor/contract **source** (not doc comments): `verify(..., None)` defaults to requiring SUCCESS and the exit code is pairing-bound public input, not a claim; refusal tags cannot postcard-decode as journals (version byte 0x52≠1, and the `valid` bool byte is strict); the program has no commit-then-continue path; the workspace-wide tiny-keccak fork is byte-identical to upstream on host targets (genuine Keccak-256 for native verify + fingerprints); vendored contracts match sp1-contracts v6.1.0 verbatim except the one flattened import; sdk 6.4.0 ⇔ circuit v6.1.0 ⇔ fixture selector all self-consistent. Six low/info findings; acted on immediately (verifier-side only, keeping the program ELF/vkey frozen while the full-scale chain runs):

- **Fixed — vacuous skip:** an explicitly set `LIFT_FIXTURE` that is missing now hard-fails; only the implicit default skips. Acceptance invocations always pass the fixture explicitly.
- **Fixed — "verifyProof is not an authorisation":** the Foundry suite header now states the relying-contract obligation (decode journal + full `check_journal` gate, count included).

**Deferred to the post-chain close-out** (info-level; touching them now would dirty the cargo graph mid-run): exact-length postcard decoding (`take_from_bytes` + empty remainder) in `decode_journal` and any future Solidity decoder; never migrate the `exit_code != 0` gate to an equality test (shard exit codes merge by OR); remove the risc0-era `ProbeJournal` and stale frame-version rationale from the statement crate; comment that refusal-tag needles are satisfied by both tag and panic message. NOT-CHECKED by the review: the ~500-line Groth16 pairing assembly body (VK constants diffed only), SP1's own attested circuit surface, and no re-execution of anything (machine constraint).

### S2 full-scale run and the overnight dependency drift (2026-08-19 → 20)

The detached full-scale chain's first leg **passed**: `s2-prove 219` proved the full classical-128 lift compressed in **30 358.8 s wall (8 h 26 m) at the 12-thread cap**, peak RSS 114.2 GB, compressed proof 1 272 755 bytes, relying-party gate green ("compressed proving run: PASS", preserved in `fixtures/s2-full-2026-08-19.log`). That beats the 4/3-scaled ~9.8 h estimate — the cap only binds during the fully parallel phases.

The S3 leg then **died at build time**: the shared `<zkboo>` checkout had been advanced by the parallel multi-chain session while the chain ran. `zkboo-bip32` now depends on `zkboo-ripemd160` (unpublished), which the patch table did not cover; deeper, the stack itself changed — the core crate adopted the **ZKB++ view commitment** (dropping linear-wire hashing) and halved wide-multiplication AND cost, and `zkboo-sha2` gained midstate-resumable compression and single-AND ch/maj. Response to the drift:

- `zkboo-ripemd160` added to the patch table; the guest ELF necessarily rebuilt (`27b87bc1…` → `4736f08e…`, new vkey).
- Since the ELF freeze was broken anyway, the four deferred review items were applied (exact-length `take_from_bytes` journal decode with trailing-byte refusal; the exit-code gate commented against ever becoming an equality test — shard exit codes merge by OR; risc0-era `ProbeJournal` and the stale independent-workspace frame rationale removed; the dual tag/panic refusal-needle behaviour documented).
- **S1 acceptance re-run green on the rebuilt tree** (7-way reject matrix with named refusers + full-scale positive against the canonical BIP-32 vector). The protocol changes show up as expected: circuit fingerprint `33e1f70b…` → **`e23ddb6a…`**, whole-program instructions **4 773 338 580** at n=219 (−17.7% vs 5 801 478 472), executor wall 30.4 s.
- Following S1's precedent (acceptance green on the final milestone tree) and because the old proving law no longer describes the new circuit, the **full S2 → S3 chain re-runs on the final tree**; the first-pass S2 PASS stands as evidence for the old tree only and its numbers must not be quoted for the final circuit.

zkboo crate revisions pinned at chain launch (all working trees clean): zkboo `ffe5807`, zkboo-sha2 `f02972d`, zkboo-hmac `0fe9c6f`, zkboo-modular `6db8a17`, zkboo-ecc `bc67c07`, zkboo-keccak `70a2506`, zkboo-circuit-hash `d2a98dc`, zkboo-bip32 `8315f30`, zkboo-ripemd160 `6de26ac`.

### S2 + S3 acceptances on the final tree (2026-08-20): ALL GREEN

The relaunched chain (prebuilt binaries, 12-thread cap, pinned zkboo revs above) completed both full-scale legs on the post-drift circuit:

- **S2 (compressed prove, n=219): PASS** — proving **22 954.4 s**, whole leg **22 963.6 s (6 h 23 m)**, peak RSS 113.9 GB, compressed proof the constant 1 272 755 bytes, relying-party gate green. 24% faster than the first-pass run on the pre-drift circuit (30 358.8 s), tracking the −17.7% instruction count. New capped law point: ≈ 105 s/response.
- **S3 (Groth16 wrap, n=219): PASS** — full core→compress→shrink→native-gnark pipeline **23 125.0 s (6 h 25 m)**, leg wall 23 134.2 s, peak RSS 115.3 GB; wrapped proof 356 bytes, public values 186 bytes (the n=1 smoke's 185 plus one varint byte for the count); relying-party gate green on 219 responses; fixture `verifier/fixtures/lift-219.onchain.json`.
- **On-chain acceptance (forge, audited SP1VerifierGroth16 v6.1.0): 5/5 PASS, `verifyProof` = 270 441 gas** — identical to the smoke figure, as expected for a constant-size proof. Rejects: corrupted proof / wrong vkey / wrong public values → `ProofInvalid`; wrong selector → `WrongVerifierSelector`. (The corrupted-proof case's reported gas remains the known cheatcode accounting artifact of the reverted pairing frame.)

Total sequential chain wall: 12 h 48 m at the ~75% CPU cap. Headline caveats for any write-up: 219 responses = classical 128-bit in the forgery-work-ratio sense only; the BN254 pairing (~100-bit) is the track's true classical ceiling; `verifyProof` is not an authorisation (see the Foundry suite header); the ZKB++/single-AND protocol optimisations the final circuit rests on are the zkboo stack's own reviewed work, consumed here at the pinned revisions.

### S3 smoke, end-to-end on a real proof (n=1, 2026-08-19)

The full local chain works: `s3-wrap 1` (core→compress→shrink→**native Groth16 wrap**, 512 s at 12 cores, peak 104 GB) produced a 356-byte EVM proof + 185-byte public values (the postcard journal), passing the relying-party gate; the Foundry suite on that fixture: **5/5 PASS, `verifyProof` = 270 441 gas**, rejects green (corrupted proof / wrong vkey / wrong public values → `ProofInvalid`; wrong selector → `WrongVerifierSelector`; the corrupted-proof case's reported gas is a cheatcode accounting artifact of the reverted pairing frame, not a real cost). The wrapped proof is constant-size and the journal layout is n-independent, so the full-scale gas will match this figure up to a few bytes of varint.

## S4 — proving cost: what it is actually made of (2026-08-21)

### The 12 h 48 m was half harness

`s3-wrap` calls `client.prove(...).groth16()`, which runs core → compress → shrink → wrap from scratch; it never consumed the compressed proof the S2 leg had just produced.
Running S2 and S3 as separate acceptances therefore paid for core proving and compression twice.
Measured from the two logs: S2 22 954.4 s, S3 23 125.0 s, difference **171 s** — that is the entire shrink+wrap stage, of which gnark itself is ~26 s (R1CS read 8.0 s, proving-key read 1.0 s, prove 16.2 s, verify 0.7 s).
**The real end-to-end cost of the full-scale lift was 6 h 26 m, not 12 h 48 m.**
`lift` replaces both drivers with a single pass; `compressed` mode survives only as a throughput probe.

### The remaining cost is intrinsic, and the guest is not the problem

Executor at n=1: 51 926 665 instructions; at n=219: 4 773 338 580. (Both **pre-upgrade**, i.e. before the journal gained `proof_format_version`; the current figure is 4 773 338 656, recorded below. The law below is fitted on the pre-upgrade pair, and the 76-instruction difference does not move it.)
Fitting the pair gives **21.66M instructions per response + ~30M fixed** (frame decode plus the in-guest circuit-fingerprint pass) — the fixed part is 0.6% at full scale.

Two suspicions were checked and both are dead ends:

- **Word width.** The `u32`/`u64` zkboo features are enabled in `statement/` and `program/`, so SHA-512 runs on native 64-bit words. There is no 8× byte-serial penalty to reclaim.
- **Replay-backend overhead.** At ~370 instructions per response byte (vs ~2 400/byte for the software-Keccak risc0 baseline) the SP1 Keccak precompile is plainly firing, and `zkboo-bip32/README.md` puts native verification of a hardened-child response at ~2.1 ms — order 10–25M instructions on this CPU, against the guest's 21.66M. **The guest runs at approximately native efficiency**; the refcounted `WordRef`/`MemoryManager`/`WordPairPool` machinery built for a 30 KB secure element is not measurably dead weight inside the zkVM. (Caveat: that README figure is a parallel measurement, so the single-core comparison is uncertain by perhaps 2×; it bounds the lever, it does not price it exactly.)

What remains is arithmetic: ~4.77e9 instructions of genuine verification work proved at **~209k instructions/s** on 12 CPU threads, i.e. ~24 000× slower than native execution. That is the cost of CPU zkVM proving, and it is not a defect to be optimised away.

### Prover configuration sweep (n=8, prebuilt binaries, 12-thread cap)

`SP1CoreOpts::default()` reads `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD` and `TRACE_CHUNK_SLOTS` from the environment, so these are tunable **without a rebuild** — which is why the sweep could be run against the accepted tree's own binaries, with no exposure to zkboo drift.
The full-scale runs to date used stock defaults; none of these knobs had ever been set.

| configuration | wall | CPU time | peak RSS |
|---|---:|---:|---:|
| default (`trace_chunk_slots=5`) | 1 141.6 s | 11 979.8 s | 112.4 GB |
| `TRACE_CHUNK_SLOTS=3` | **1 032.9 s** | 11 035.6 s | 110.2 GB |
| `TRACE_CHUNK_SLOTS=2` | 1 034.4 s | 11 087.6 s | 111.3 GB |
| `slots=3` + halved shard thresholds | 1 287.9 s | 14 035.4 s | **63.5 GB** |

Fewer chunk slots is **9.5% faster and 7.9% less CPU work** — a genuine reduction in work, not a scheduling artefact — and the gain **saturates at 3**: dropping to 2 gives back nothing (1 034.4 s, inside the noise).
Peak memory barely moves across all three, so the ~110 GB is not held by the chunk slots; it is also essentially workload-independent (112.4 GB at n=8 against 113.9 GB at n=219), i.e. a fixed prover allocation rather than a function of the job.
That matters for the next lever: bigger shards mean fewer recursion leaves but cost memory, and on a 137 GB machine already peaking at 110 GB the fast direction may simply be unavailable — which is exactly the constraint a 400 GB machine would not have.

Halving `ELEMENT_THRESHOLD`/`HEIGHT_THRESHOLD` costs **24.7% in time** and returns **42% of the memory**, which identifies what the ~110 GB is made of: **peak RSS scales with shard size**. The trade is clean and monotone — bigger shards mean fewer recursion leaves and less time, at proportionally more memory. Extrapolating the same ratio, the next doubling (805M elements) would want ~190 GB, so on a 137 GB machine **the fast direction exists but is memory-blocked**; the chosen configuration sits at the largest shard the machine can hold. A 400 GB machine has no such ceiling, which is a measured reason to expect it to beat this one by more than its core count alone.

⚠️ Three knobs read like levers and are not: `full_size_shards`, `drop_ldes` and `recompute_gkr_trace` are each referenced only in `opts.rs` and never consulted anywhere in the 6.4.0 prover. Checking that at the source, rather than trusting the field's existence and its docstring, is the standing lesson from the Metal episode.

**Chosen configuration: `TRACE_CHUNK_SLOTS=3`, stock shard thresholds, 12 threads** — the measured optimum at n=8, inside this machine's memory envelope.

🚨 **This gain did not survive contact with full scale. See the full-scale result below: at n=219 the same setting came in 0.84% *slower* than the default configuration, not 9.5% faster.** The n=8 measurement was not noise — two independent runs at `slots=2` and `slots=3` both landed near 1 033 s against 1 141.6 s, and total CPU time fell with it — so the effect is real at n=8 and absent at n=219. A 219-response run has roughly 280 shards against roughly 12 at n=8; whatever the chunk slots relieve at a dozen shards is already saturated at a few hundred. **Treat every number in the sweep table above as a statement about n=8 only.**

### Scaling constants for planning

From the n=8 and n=219 points at the 12-thread cap on the default configuration: **103.4 s per response + ~315 s fixed** per proving run. The fixed part is 27% of an n=8 probe and 1.4% of a full-scale run, so small-n probes understate marginal improvements and must not be extrapolated naively.

## zkboo 1.0.0 (2026-08-21)

The zkboo stack finished its development line and moved to **1.0.0**; all ten crates are committed and clean, and the lifter's manifests now require `1.0.0`.
The `[patch.crates-io]` table stays for now because 1.0.0 is not yet published; it should be dropped once it is, so that a released artefact builds against released crates rather than local working trees.

Three changes landed after the revisions this repo had pinned (`zkboo` `ffe5807` → `08e732d`), and each was assessed before adoption:

- **`PROOF_FORMAT_VERSION` (= 2)** names the response-byte layout together with the view-commitment scheme it is checked against. Its rationale matters to us directly: **a circuit fingerprint cannot see a commitment-scheme change** — adopting the ZKB++ commitment changed every commitment, and hence every Fiat-Shamir challenge, while leaving circuits and protocol parameters untouched. The lifter's journal committed only the fingerprint, so two incompatible proof formats would have produced indistinguishable journals. The verifying key does bind the format (it is compiled into the program), but only implicitly. The journal now commits `proof_format_version` and `check_journal` enforces it (`ProofFormatMismatch`, reject-matrix case R8); `LIFT_JOURNAL_VERSION` is bumped to 2. Our pinned revision was already format 2, so this records a property that already held rather than changing one.
- **Ranged response generation** (`ProofBuilder::skip_iters`) lets a prover emit any half-open range of the response sequence. This looks like a soundness hazard for a count-based gate: commit to N ≫ 219 iterations, see which survive the challenge (~2/3 of them), and emit only 219 winners. **It is not one.** The challenge entropy is hashed over the view commitments of exactly the responses *present in the proof*, and the regenerated challenge sequence is compared position by position; dropping responses changes the entropy, hence every challenge, so the retained responses stop matching. The relevant path is the **sequential** `verify`, which is what the guest compiles — `program/src/main.rs` imports it by name, so no feature flag can change that. Both it and `par_verify` live in `zkboo/src/verifier/functions.rs` (at `verify` and `par_verify` respectively) and carry the same derivation. Selection is self-defeating and a partial range does not verify standalone — it must be reassembled. `skip_iters` is a prover-side convenience (resumable, retriable, splittable proving), not a verifier-side relaxation, and `num_responses ≥ 219` remains exactly the right gate.
- **No change to the serialised `Proof`/`Response` shape** (the diff touches `lib.rs`, `params.rs`, `builder.rs` and tests only), so the `LiftInput` wire format is unchanged and `LIFT_FRAME_VERSION` stays at 1.

### Acceptance evidence on the zkboo 1.0.0 tree (2026-08-21)

Measured on the rebuilt tree, so the upgrade's neutrality is recorded rather than asserted:

- **Reject matrix: 8/8**, every refusal naming its refuser (R1/R2 tampered proof and wrong binding → refused by the program; R3/R4/R5/R6/R8 → refused by `check_journal`; R7 malformed response → view replay error).
- **R8 validated by check-removal**: deleting the format check makes it fail with `expected Err(ProofFormatMismatch), got Ok(())`, and restoring it returns the matrix to green.
- **Full-scale positive (n=219)**: circuit fingerprint **`e23ddb6a…`** — unchanged from the pre-upgrade pin — and **4 773 338 656** whole-program instructions against 4 773 338 580 before, a difference of **76**. Two changes landed together — the new journal field and the zkboo revision bump — so attributing the whole delta to the field is the plausible reading rather than an isolated measurement; what is established is that the upgrade moved the instruction count by 76 in 4.77e9 and left the fingerprint identical. Executor wall 30.23 s.
- **Single-pass driver end to end (n=1, groth16)** — note this ran on the driver as it stood before the review fixes, so its log predates the `backend=`/`memory_limit=` fields; the differences are a `println!`, an `assert_ne!`, and `expectation_for` sourcing the format version from the statement crate instead of the linked `zkboo` (numerically identical). The statement crate itself also changed, which relinks the guest — and it is the guest that determines the verifying key now pinned in the on-chain suite *from this very fixture*. The added item is a `const`, which emits nothing into the loadable image, and SP1 derives the key from the program's instructions and memory image rather than from raw ELF bytes, so the key should not have moved; the full-scale fixture is what will settle it. The milestone's acceptance is that run, not this one: 459.5 s against 512 s for the superseded two-driver path at the same count, 356-byte proof, 186-byte public values, gate green; `forge` 5/5 with `verifyProof` at **270 441 gas**, unchanged by the extra journal field as expected for a constant-size proof.

⚠️ Provenance of the sweep numbers above: those four probes were run with the *previous* binaries, which predate the configuration line this milestone adds, so their logs carry the old banner and no `prover config:` record — the mapping from setting to measurement rests on the filenames. The numbers still transfer, because `SP1CoreOpts::default()` already read the environment before this work and `from_env_with_opts(Some(SP1CoreOpts::default()))` is a behavioural no-op against the previous `from_env()` on the CPU path. Runs from here on record their own configuration.

⚠️ Fixtures are generated artefacts that outlive the program that produced them, and a fixture from an older program stays internally consistent — `verifyProof` accepts it, so the Foundry suite goes green against a program that no longer exists. This actually happened: after the upgrade, `verifier/fixtures/lift-219.onchain.json` still held a journal at version 1 under the superseded verifying key while the suite's default path pointed at it. The stale artefacts were deleted, and the suite now refuses a fixture it should not accept, on every count it can reach, each validated by staging a corrupted fixture and watching it fail with its own message: the journal's **version byte** must match the statement crate (a layout change), the **fingerprint** at `publicValues[2..34]` must match the circuit (a circuit change — this is the drift that actually happened once, when the ZKB++ commitment moved `33e1f70b…` to `e23ddb6a…` without touching the journal shape), the **verifying key** must match the program (a program change that alters neither of the above; without this the suite attests nothing about which program ran, since `vkey` is read from the fixture and handed straight to `verifyProof`), the version must be **single-byte**, since every offset below it shifts silently once the version reaches 128; the **proof format** must match; and — for the default fixture, which is the acceptance artefact — the journal must attest **219 responses**, without which a smoke fixture copied to the acceptance path passes at identical gas while attesting a single response, the same accident class as the stale fixture above. A suite that skips for want of a fixture now says so through `vm.skip`'s reason string: `console2` logs from a skipped `setUp` are discarded by forge at every verbosity, so a logged warning there is dead code.

### Full-scale run on the released tree (launched 2026-08-21)

Launched with prebuilt binaries — no cargo inside the run, so it is immune to any further movement in the dependency trees — at the swept configuration `TRACE_CHUNK_SLOTS=3`, `RAYON_NUM_THREADS=12`, `GOMAXPROCS=12`, stock shard thresholds. Single pass: `lift 219 groth16` runs core → compress → shrink → wrap once and emits both the proof and the Foundry fixture.

zkboo revisions pinned at launch, every crate **pushed and clean** (0 unpushed, 0 dirty), so the resulting verifying key and fixture are attributable to published revisions rather than to a local working tree: zkboo `ca156da`, zkboo-sha2 `41ef52c`, zkboo-hmac `d8b12cd`, zkboo-modular `9f7624e`, zkboo-ecc `4ebff06`, zkboo-keccak `642fca6`, zkboo-circuit-hash `a775387`, zkboo-bip32 `02d2d4b`, zkboo-ripemd160 `5c367b4` — all at 1.0.0. The acceptance above was measured on this same tree: the `Release 1.0.0` commit was already in the working tree when those binaries were built, which is why rebuilding against the pushed revisions recompiled nothing. (The earlier pin of `08e732d` and its siblings names the *pre-upgrade* tree, not the tree the acceptance ran on.)

Projection to be checked against the outcome: ~5 h 50 m, from the marginal law (93.6 s/response after the 9.5% configuration gain) plus ~171 s for the wrap.

### The partial-proof question, tested (2026-08-21)

Reasoning from the source said a partial proof cannot verify. Two independently written probes tested it, and both agree; the ranged cases correspond to zkboo's own committed `tests/test_ranged_proof.rs`.

On a toy circuit, 23 of 23 cases behaved as predicted: an honest proof built at any `num_iters` verifies; the full range `[0,9)` verifies; and a **truncated prefix** (each of 1..8 of 9), an arbitrary **sub-range** (`[0,k)`, `[4,9)`), a **sparse selection of 9 responses out of 30 iterations**, a **reversal**, a **duplication** and a valid proof with foreign responses appended all fail. On the real `HardenedChildCircuit` — the lifted statement itself — a 12-response proof verifies while its 8-response prefix, a subset with the middle four dropped, and a reordering all fail.

The empty proof remains the single degenerate case that verifies, because it has no challenges to check; `s1-acceptance` R5 confirms the count gate is what refuses it.

⚠️ Scope of the evidence: both probes drive the **sequential** `verify`, the path the guest compiles. That `par_verify` shares the derivation was established by reading both functions, not by probing the parallel path. An earlier note claiming the result held "on both verifier paths" overstated what was run.

### Planned: the worker-pool sweep (designed 2026-08-21, pre-registered)

`SP1CoreOpts` is not the only environment surface. `sp1-prover`'s worker pool is configured by a family of `SP1_WORKER_*` variables (`worker/config.rs`), every one of them env-read, sitting on hard-coded defaults beside a literal `TODO: base default values on system information`:

| knob | default |
|---|---:|
| `SP1_WORKER_NUM_CORE_WORKERS` | 4 |
| `SP1_WORKER_NUM_RECURSION_PROVER_WORKERS` | 8 |
| `SP1_WORKER_NUM_RECURSION_EXECUTOR_WORKERS` | 4 |
| `SP1_WORKER_NUM_PREPARE_REDUCE_WORKERS` | 4 |
| `SP1_WORKER_NUM_SETUP_WORKERS` / `_SPLICING_WORKERS` | 2 / 2 |
| `SP1_WORKER_MAX_REDUCE_ARITY` / `_MAX_COMPOSE_ARITY` | 4 / 4 |
| `SP1_WORKER_VERIFY_INTERMEDIATES` | **true** |

Three hypotheses, to be measured at n=8 against the 1 032.9 s baseline, one variable at a time:

1. **`VERIFY_INTERMEDIATES=false`.** The default spawns a blocking `core_verifier(...).verify(...)` for **every shard proof as it is produced** (`worker/prover/core.rs:618-630`); at roughly 280 shards for a full run that is 280 verifications contending for the same cores. It is a prover-side self-check, and removing it is **soundness-neutral for a relying party**: the final proof still passes `check_lifted_proof` and the audited on-chain verifier, so this cannot make an invalid proof verify — it can only delay the discovery of a prover bug from the shard that produced it to the run that fails at the end. Expected to be the largest of the three.
2. **`MAX_REDUCE_ARITY=8`.** Fewer recursion levels over the same leaves. ⚠️ Arity tables have misled this project before — recursion has cost more than the thing it recursed over — so this is a measurement, not a prediction.
3. **Worker counts rebalanced to the 12-thread cap.** The defaults total 28 logical workers; whether raising the core and recursion-prover counts helps or simply adds contention is unknown.

Each probe uses prebuilt binaries and changes exactly one variable, so a result is attributable. Nothing here can affect what a proof means — every one of these knobs is a scheduling or self-check decision on the prover side, and the verifier is unmoved.

### Full-scale result on the released tree (2026-08-21): ACCEPTANCE MET, projection MISSED

`lift 219 groth16`, single pass, prebuilt binary, `TRACE_CHUNK_SLOTS=3`, 12 threads, zkboo 1.0.0 at the pinned revisions.

| | measured |
|---|---:|
| zkVM groth16 pipeline (core → compress → shrink → wrap) | **23 320.2 s (6 h 28 m 40 s)** |
| whole run, wall | 23 329.3 s |
| peak RSS | 112.2 GB |
| gnark stage inside that | 25.4 s (R1CS read 7.9, pk read 1.0, prove 16.2, verify 0.7) |
| EVM proof / public values | 356 B / 187 B (one varint byte more than n=1, the response count) |
| relying-party gate | green, 219 responses |
| **on-chain (`forge`, all seven fixture guards + 4 rejects)** | **5/5, `verifyProof` = 270 441 gas** |

**The pre-registered acceptance is met**: a single-process `lift 219` produced a Groth16 proof that passed the full gate and verified under `forge` at recorded gas, its log records the configuration it ran under, and `knowledge/` carries the sweep record. The acceptance's last clause — that the configuration be *justified by measurement rather than assumption* — is where this run indicts its own method: the justification was an n=8 measurement, and it did not hold.

**What actually improved, and what did not:**

- **The harness fix is real and confirmed at full scale: 46 079 s → 23 320 s, a factor of 1.98.** Proving once instead of twice is the entire gain.
- **The configuration change bought nothing**: 23 320.2 s against the previous full-scale groth16 pass at stock settings, 23 125.0 s — **0.84% slower**. Within run-to-run noise, and in the wrong direction. (Two short review agents did millisecond-scale `forge` and file reads during the first ninety minutes; that cannot account for a percent.)
- **The projection was wrong by 11.1%**: 20 984 s predicted, 23 320 s measured. It was built by scaling the n=8 marginal law by the n=8 configuration gain, so it inherited the error twice over.

**The methodological lesson, which is the durable output of this run:** a cheap probe at n=8 answered a different question than the one asked. Shard count is the variable the prover's scheduling knobs act on, and it differs by more than an order of magnitude between the probe and the real workload. This repeats a lesson already recorded on the STARK track — behaviour that is invisible at four queries can dominate at three hundred — in the opposite direction: a gain visible at a dozen shards evaporates at a few hundred. **Any future prover-configuration claim must be validated at a shard count within reach of the real one before it is believed, and small-n probes are for screening candidates, not for sizing them.**

Note this cuts the other way for the next lever: `SP1_WORKER_VERIFY_INTERMEDIATES` verifies *per shard*, so its cost grows with shard count and an n=8 probe would **understate** it — the mirror image of the error above.

### The worker-pool sweep, abandoned on two full-scale nulls (2026-08-22)

Hypothesis 1 was tested where the previous mistake said it had to be — at full scale, one variable changed against the 23 320.2 s run on the same tree.

| n=219, same tree, `TRACE_CHUNK_SLOTS=3` | pipeline | CPU time | peak RSS |
|---|---:|---:|---:|
| `VERIFY_INTERMEDIATES` default (true) | 23 320.2 s | 274 679 s | 112.2 GB |
| `VERIFY_INTERMEDIATES=false` | **23 194.7 s** | 273 641 s | 116.4 GB |

**0.54% — inside run-to-run noise.** Removing ~280 shard verifications from a full run is worth almost nothing, and in hindsight the reason is arithmetic rather than scheduling: verifying a shard proof is orders of magnitude cheaper than producing one, and the verifications run on a blocking pool that overlaps with proving anyway. The expectation that this would be the largest of the three knobs was wrong.

**The sweep is abandoned here, and the reason generalises past these knobs.** Two full-scale interventions have now returned nothing, and the profile says why. This run's own trace attributes only 1.45% (337 s over 1.74M invocations) to Keccak trace generation and 25.6 s to the wrap; the top-level proving spans carry no busy time at `info`, so the work sits below them. The sp1-metal probe suite profiled this very workload — *"the ProofofSeed snark-lifter's compressed proof at n=8, 1007 s wall"* — and found **GKR 46.8%, PCS opening 19.0%, zerocheck 15.0%, commit 13.7%**: about 95% of the run is field arithmetic inside sumcheck-shaped phases. None of the remaining knobs (`MAX_REDUCE_ARITY`, worker counts) touches that; they schedule it. With `user/real` already at 11.8 against a 12-thread cap, there is no idleness for a scheduler to reclaim.

**What this leaves.** Proving cost is now bounded by two things and neither is configuration: the instruction count (zkboo's territory, 17.7% already banked) and the per-instruction proving rate (~209k/s). On the second, the measured, non-GPU opportunity is that SP1's CPU prover does its field arithmetic on scalar values — 0 packed-type mentions across `slop-sumcheck`, `slop-multilinear`, `slop-jagged`, `slop-basefold-prover`, `slop-dft`, and 1 of 65 files in `sp1-hypercube` — where packed NEON measures 3.26× on the GKR round and 2.31–4.65× on the folds. Against a ~70% share that is roughly **1.9× end to end**, on the hardware already here, in upstream code that a `[patch.crates-io]` fork can reach and that is **vkey-neutral and soundness-neutral** to change: the verifying key is a function of the program, not the prover, so a packed prover produces proofs the same audited verifier accepts, with no fixture regeneration — and a mistake in it cannot forge, only fail to verify.

## The elephant probe (2026-08-22): the Ethereum-address statement, measured

The statement that actually binds a proof of seed to an Ethereum account is
`address = Keccak256(x_be ‖ y_be)[12:]` where `Q = (x,y) = d·G` on secp256k1. Everything the
lifter had measured until now was on the hash-only side of the proof-of-seed suite; this is the
expensive side, probed at one, two and four responses rather than by committing months of machine
time to find out.

The probe lives in its own crate and its own guest (`elephant/`, `program-elephant/`), deliberately
outside the production statement crate: that crate's shape fixes the guest ELF and hence the
verifying key the on-chain suite pins, and a probe must not move it. Verified after building: the
Foundry suite still passes 5/5 at the same key.

### Measured, at the circuit

> 🔴 **STALE INPUT, 2026-08-26 — the elephant shrank 3.8× and this note's arithmetic has not been redone.**
> The device now measures **2,995,675 B** (public key), **2,955,830 B** (normal child) and **2,907,372 B**
> (Ethereum address) per response, against the **11.28 MB** this note is built on, after zkboo 2.0.0's
> prover advice, in-circuit assertions and affine comb. All three were proved *and verified* on hardware
> (`ledger-app/docs/benchmarks.md`, `whitepaper/on-device-measurements.md`).
>
> Guest instruction count scales with the ZKBoo trace the lifter replays, so the lift cost plausibly falls
> by a similar factor — but **that has not been measured**, and nothing here should be scaled by hand.
> The hash-statement response is likewise **51,297–51,691 B measured**, not ~58 KiB.
> **On the re-measurement list: re-run the cycle-law probe against the current circuits.**


| | hardened child | Ethereum address | ratio |
|---|---:|---:|---:|
| response size | ~58 KiB | **11.28 MB** | 190× |
| full proof at 219 responses | 12.7 MB | **2.47 GB** | — |
| circuit execution | — | 269 ms | — |
| native prove, 1 response | ~5 ms | 1.16 s | — |
| native verify, 1 response | ~2.1 ms | **511 ms** | 243× |

⚠️ `zkboo-bip32/README.md` still lists 42.3 MB per response for this statement. That table predates
the optimisation round; the measured figure is **11.28 MB**, 3.75× smaller, and the full-scale proof
data is **2.47 GB rather than the ~9.3 GB** quoted from the stale number.

### Measured, in the zkVM

| n | instructions | executor wall | peak RSS |
|---:|---:|---:|---:|
| 1 | 30 495 877 378 | 185.2 s | 15.1 GB |
| 2 | 40 917 254 932 | 248.8 s | 15.7 GB |
| 4 | 61 760 022 190 | 378.0 s | 16.5 GB |

**Fitted law: `instructions = 2.007e10 + 1.042e10 × n`.** The two independent marginal estimates
(n=1→2 and n=2→4) agree to **0.0001%**, so the law is trustworthy in a way a single point never is —
and the single point would have misled badly: the **fixed term is 66% of the n=1 total**, being the
in-guest circuit-fingerprint pass over a secp256k1 circuit plus the decode of a 13.4 MB frame.
Extrapolating from n=1 alone overstates the full-scale cost by **2.90×**. At 219 responses that same
fixed term is 0.87% of the total and can be ignored.

Note the shape of the cost: **481× the hardened child per response against a 190× response size**, so
the elephant is ~2.5× more expensive per byte. The cost is replaying scalar-multiplication gates,
which no precompile relieves — SP1's Keccak accelerator does nothing for field arithmetic. Note also
that the zkVM needs roughly 6–12× the instructions the native verifier uses here, where on the hash
statement it sat at rough parity: 256-bit field arithmetic compiles far less favourably to RISC-V
than hashing does.

### What it would cost

At 219 responses: **2.302e12 instructions**, and at this machine's measured proving rate of
204 687 instructions/s that is **130 days** — **482× the 6 h 28 m the hardened-child lift takes**.

⚠️ The rate is the hardened-child statement's, assumed to carry over. It has **not** been measured on
an elephant workload, whose instruction mix is entirely different, and no elephant response has been
proved — only executed. Treat 130 days as an order-of-magnitude statement, not a figure.

Scaled onto other hardware, each factor an extrapolation stacked on the last: ~35 days on valkyrie's
64 cores, ~13 days with one of its GPUs at sp1-metal's projected 10×, ~3 days with four or five cards
busy on one proof. **None of these is a route to lifting this statement as a single proof.**

### The structural obstacle, and why the obvious fix is unsound

219 responses look like 219 independent verification tasks that a cluster could divide. **They are
not, and this repository has already proved they are not.** The Fiat-Shamir challenge is derived from
the view commitments of exactly the responses present in the proof, so a subset re-derives different
challenges and fails — truncations, sub-ranges, sparse selections and reorderings were all built and
all rejected (recorded above). Splitting the work therefore cannot mean splitting the proof: any
division has to keep a global challenge check over all 219 responses while distributing the replay,
which is a design in its own right — the same global-Fiat-Shamir gate the STARK track identified as
the correctness condition for any lifter.

**The honest conclusion of this probe: the Ethereum-address statement is not liftable by this route
at classical-128, on any hardware currently available to the project, and the gap is ~2 orders of
magnitude rather than something a better configuration closes.** What the probe has bought is that
this is now measured rather than assumed, and that the two cheap levers are visible: the per-response
instruction count (zkboo's circuit, where 3.75× has already been found once) and the proving rate
(where packing the CPU prover's scalar field arithmetic is worth ~1.9× and costs no soundness).

## S5 — the released ZKBoo, GPU proving, and the owed measurements (2026-08-28)

### The upgrade cost no source change, and that is a finding about the version tags

zkboo 2.0.0 is breaking — every circuit's outputs gain a trailing assertion word, so no 1.x proof
verifies and `PROOF_FORMAT_VERSION` is 3 — and the lifter needed exactly one constant changed. The
expected output comes from `exec`, so it grew the word on its own; the frame's layout did not change,
because the assertion word is a `u8` among bytes the outputs already carried and the enabled word
features are untouched.

`LIFT_FRAME_VERSION` and `LIFT_JOURNAL_VERSION` therefore stayed where they were. Each guards a
layout, no layout moved, and what catches a proof from the old format is `proof_format_version`,
which the journal already commits and `check_journal` already enforces — the mechanism built for this
exact case when the ZKB++ commitment changed. Bumping a tag whose contract was not breached would
teach the next reader that the contracts are decorative.

| | before (zkboo 1.0.0) | after (zkboo 2.0.0) |
|---|---|---|
| circuit fingerprint | `e23ddb6a…` | **`d4825905…`** |
| whole-program instructions at n=219 | 4 773 338 656 | **4 775 013 465** (+0.035%) |
| public values | 186 B | 187 B |
| on-chain gas | 270 441 | **270 441** |

The instruction count is the useful one: the assertion word costs a thirtieth of a percent on this
statement, which is what it should cost for a circuit that asserts nothing and emits a constant.

### Provenance is now the lockfile's problem, not a working tree's

The manifests asked for `1.0.0` and patched it to local paths that had since become 2.0.0. **A patch
whose local version stops satisfying the requirement is ignored by cargo without a word**, and the
published crate is resolved instead. The ledger-app line lost a round of measurements to precisely
this: proofs that were well-formed and did not verify, with a provenance field that read `git
describe` on the working tree and so named a version the binary did not contain.

The patches are gone, the requirements are exact, and `host/build.rs` refuses to build unless every
zkboo crate resolves from a registry at the required version with a checksum. Both arms of the
refusal were demonstrated by making the tree violate them. Every driver prints what it linked.

### GPU proving, and the defect that bounds it

Full account in `knowledge/gpu-proving.md` and `knowledge/gpu-corruption.md`.

Compressed proving of a one-response lift, three arms, two runs each, on an idle machine:

| arm | median | peak RSS |
|---|---:|---:|
| published SP1 6.4.0 | 274.9 s | 98.0–101.1 GB |
| the fork, GPU forced off | 276.4 s | 87.8–89.7 GB |
| the fork, GPU on | 58.3 s | 40.5–43.6 GB |

**4.74×, and the third arm is why that number means anything**: with the patches in place both of the
obvious arms run through the fork, so a slowed CPU path would be indistinguishable from a fast GPU.
The fork's CPU path lands within 0.8% of the published prover.

**And it produces a wrong proof about once in twenty-eight, then parks the run for ever.** An intermittent
Merkle root mismatch, caught by SP1's own shard verification — so nothing invalid is accepted and the
risk is availability, not soundness — but retried silently and never returned from. Leaving the
Merkle tree on the CPU has run 115 consecutive proofs clean, at 1.92× instead of 4.72×. The number to
plan against here is 1.9× until it is fixed upstream.

### `root(N)`, measured where it was owed

| | CPU | GPU, safe |
|---|---:|---:|
| leaf (one response) | 109.72 s | 57.00 s |
| root over 4 leaves | 205.61 s | 112.18 s |
| root over 64 leaves | 527.20 s | 264.95 s |
| fitted | 184.2 + 5.36·N | 102.0 + 2.55·N |

Admitting one more leaf costs **4.5% of a leaf**. The discarded n=4/n=8 fit had the slope right to
1.3% — recorded, and not a reason to have believed it: it was a 27× extrapolation taken on a machine
under load, and this repository had already been burned by a small-n result that did not survive full
scale.

### The full-scale lift on the new stack (2026-08-28)

`lift 219 groth16`, safe GPU setting, prebuilt binaries, released zkboo 2.0.0 from crates.io:

| | zkboo 1.0.0, CPU (2026-08-21) | zkboo 2.0.0, GPU safe (2026-08-28) |
|---|---:|---:|
| one proving pass, core → compress → shrink → wrap | 23 320.2 s (6 h 28 m 40 s) | **10 296.11 s (2 h 51 m 36 s)** |
| peak RSS | 112.2 GB | **77.5 GB** |
| proof | 356 B | 356 B |
| `forge` | 5/5 at 270 441 gas | **5/5 at 270 441 gas** |

**2.27×**, and essentially all of it is the GPU: the zkboo upgrade moved the guest by 0.035% of its
instructions, so the two changes on this branch do not have to be disentangled by argument.

Marginal cost falls from 103.4 s per response to about 47 s, where a run at n = 8 had predicted
2.50× rather than 2.27× — that n = 8 CPU point being the August one on zkboo 1.0.0, as is the
full-scale baseline, so both ratios are against measurements taken before this branch rather than
against a CPU arm re-run tonight. That is the same shape as the configuration sweep that did not survive full
scale, in the mild direction rather than the catastrophic one: a short run still flatters, by a tenth
instead of by everything.

The run completed with no shard failure, over a body of shards far larger than the leaf proofs the
GPU corruption was characterised on. That is the strongest single piece of evidence that leaving the
Merkle tree on the CPU is enough.

It is also the measurement `sp1-metal`'s own verdict names as the one that would change it — wall
clock to a proof on this project's classical-128 lift, the workload its bar is written against and
had never been run on.

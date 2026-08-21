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

**Chosen configuration: `TRACE_CHUNK_SLOTS=3`, stock shard thresholds, 12 threads** — measured optimum inside this machine's memory envelope, worth 9.5% on every run for a one-line environment change.

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

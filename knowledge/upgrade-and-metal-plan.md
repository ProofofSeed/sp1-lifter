# Plan: zkboo 2.0.0 and GPU proving

> **Written for the withdrawn ZKBoo 2.0.0, and superseded twice over.**
> Its figures are stale for the reasons in `zkboo-1.2.0-migration.md` and `zkboo-next-delta.md`, and the elephant cycle law it quotes was already refitted before that.
> Kept for the method and for the phase structure, which the relaunch still follows.
> Do not quote a number from it.
>
> One item is now moot rather than merely stale: profiling the Bitcoin and Taproot statements off-device.
> Upstream re-measured them after restoring the hint-backed cost of the affine conversion, and neither statement is lifted here in any case.

Written 2026-08-27, after reading the zkboo 2.0.0 changelogs, the `sp1-metal` release documents, and the whitepaper and ledger-app notes taken during the on-device round of 2026-08-26/27.
It supersedes the quiet-machine relaunch recipe recorded in `split-and-streaming.md`, which it absorbs as one phase rather than replacing.

## What changed underneath this repository

### zkboo 2.0.0: breaking changes

Eleven crates are published on crates.io at 2.0.0 and all twelve repositories are tagged `v2.0.0`; the local trees at `<zkboo>` sit exactly on those tags, clean, with nothing unpushed.

- Every circuit's outputs now end with an **assertion word** that the backend emits, so an expected output gains a trailing word and **no proof produced by 1.x verifies**.
  `PROOF_FORMAT_VERSION` is **3**.
- New machinery: `Backend::hint` (prover advice, replayed by the verifier as an input and constraining nothing by itself), `Backend::assert_true` (accumulated by conjunction into that final output word), and `WordRef::wide_square`.
- `zkboo-circuit-hash` fingerprints advice under an opcode of its own, so **the circuit fingerprint moves** — which also moves the guest ELF and therefore **the verifying key**.
- The elliptic-curve statements are **3.9× smaller on the device path**: `ethereum_address` is 368,123 nonlinear AND-messages and 2,907,372 B per response, measured on hardware, against the 11.28 MB this repository's cost note is built on.

### Provenance trap in the manifests

Every manifest here requires `zkboo = "1.0.0"` and redirects it through `[patch.crates-io]` to a local path that now holds 2.0.0.
**A patch whose local version no longer satisfies the requirement is silently ignored**, and cargo resolves the published crate instead with no warning.
This exact failure already happened once during the device round: one app build linked pre-optimisation code while the host verifier used the local tree, every proof it made was well-formed and failed to verify, and nothing in the recorded row revealed it — proof size is bimodal and so could not separate them, and the benchmark's own provenance field ran `git describe` on the working tree and reported the local tag for a binary that did not contain it.
**Provenance must come from the lockfile.**

### The figure to plan against

`<sp1-metal>`, tag `v0.1.0`, clean and pushed; the consumer path is six `[patch.crates-io]` entries and a four-entry `metal` feature, against `sp1-sdk = "=6.4.0"`, which is what we already resolve.

- Core shard proving reaches 6.0× on a large synthetic workload, and the microbenchmark envelope is 1.94×–6.01×.
- **Compressed proving — which is what the lift is — measures 2.92×–3.08×**, three guests, measured twice on an idle machine, agreeing to within 0.02×.
  Recursion roughly halves the backend's advantage.
- Eight whole Bitcoin and Ethereum statements measure a median of 2.98×, about a fifth below the microbenchmarks taken on the same machine the same day.
  That is the figure someone deploying would get.
- The Groth16 wrap is gnark — pure Go, on the CPU — and gains nothing.
  Budget it as the constant ~171 s it has always been.
- **The guest ELFs are untouched by these patches, so the verifying key is identical across arms.**
  CPU and Metal are a clean control on one binary.

`sp1-metal`'s own verdict records that its D7 bar — wall clock from execute to compressed proof, on this project's classical-128 lift — has **never been measured on the workload it names**.
Phase 5 below is that measurement.

### Device figures

Streaming is 3,234–3,500 B/s across 39 verified rows, flat in statement, witness size, response count and proof size; a response arrives every ~29–30 s of measured device work.
The `~1.9 KB/s` this repository designed against is retracted — its timer spanned the consent prompt.
So in the real-time-keeping argument the device rate is now measured and **the Metal leaf time is the estimate**.

### Two smaller corrections

The split-and-aggregate probe is **committed** at `25b137b`, working tree clean; the memory that says otherwise is stale.
The rustup default is now `nightly-2025-12-05`, set for the ledger app; every recorded snark-lifter baseline was taken on stable 1.95.0, so this repository needs an explicit override or its timings are not comparable to their own history.

## The plan

Six phases.
Each has a pre-registered acceptance and is not done until that acceptance passes.
Phases 0–2 are prerequisites and are not timed measurements; phases 3–5 are, and run **strictly serially on an otherwise idle machine** — the previous campaign was discarded because it was not.

### P0 — Provenance and toolchain

- Pin this repository to stable 1.95.0 explicitly, so tonight's numbers are comparable to the recorded ones.
- Raise every zkboo requirement to `=2.0.0` and **drop the zkboo entries from `[patch.crates-io]`**, building against the published releases; keep the `tiny-keccak` patch, which is the SP1 Keccak precompile and unrelated.
  Pin every `sp1-*` requirement at `=6.4.0`, which is what `sp1-metal` tracks exactly.
- Add a provenance guard that reads the resolved source rather than the working tree, and print it in every driver's banner beside the prover configuration.

**Acceptance:** every zkboo crate resolves from the registry at 2.0.0 with a lockfile checksum; the guard is demonstrated **red** by pointing it at a tree it must refuse.

### P1 — The lifter on zkboo 2.0.0

The assertion word reaches `expected_output` on its own, because that comes from `exec`; what does not follow on its own is every version tag and every pinned artefact.

- `REQUIRED_PROOF_FORMAT_VERSION` 2 → 3; bump `LIFT_FRAME_VERSION`, `LIFT_JOURNAL_VERSION`, and the leaf and root frame and journal versions in `crates/split/`.
- Delete every fixture.
  A fixture from an older program stays internally consistent, `verifyProof` accepts it, and the Foundry suite goes green against a program that no longer exists — this has happened here once already.

**Acceptance, all of it before anything is timed:**

1. `s1-acceptance` reject matrix 8/8, each refusal naming its refuser, R8 re-validated by removing the check and watching it go red.
2. `split-probe 4 gate` 9/9.
   **The control accepting is the load-bearing case**: it is what says `aggregate` still reproduces zkboo 2.0.0's challenge derivation byte for byte — the framed binding, the absorption order, the two-bits-at-a-time party generation with `0b11` rejected.
3. The new circuit fingerprint recorded **and shown to differ** from `e23ddb6a…`.
   A fingerprint that did not move would mean the assertion word never reached the circuit, and the upgrade would be a fiction.
4. `lift 1 groth16` end to end, `forge` 5/5 against the regenerated fixture.

### P2 — sp1-metal wired in, correctness before speed

- Patch by **git tag `v0.1.0`, not by path.**
  The checkout is shared, and a path patch lets a concurrent edit move what was measured after it was measured.
- Warm the shader cache once at `SP1_METAL_COMPILE_BUDGET=1800`; macOS caches a compiled kernel across processes, and a cold cache is worth up to a factor on the first run of a workload.
- Run one small lift with `SP1_METAL_ZEROCHECK=check`, which runs both paths on every call and compares element for element.

**Acceptance:** `lift 1 groth16` under Metal passes the relying-party gate, its verifying key and public values are byte-identical to the CPU arm's, and `forge` is 5/5 — both arms through one binary, which is the only comparison in which nothing but the path under test differs.

**Gate before P5:** peak RSS on a mid-size run.
The CPU arm peaks at 112–114 GB of 137, unified memory means the GPU draws from the same pool, and the trade for relief is known and monotone — halving the shard thresholds costs 24.7% in time and returns 42% of the memory.
If the mid-size run is above roughly 120 GB, P5 is re-planned rather than launched hopefully.

### P3 — The cheap measurements that carry the most information

These are hours of machine time and they move the two most consequential stale figures in the project.

- **Re-fit the elephant cycle law.**
  Executor only, at n ∈ {1, 2, 4}, against the current circuit at the **device comb window width**.
  The existing law, `instructions = 2.007e10 + 1.042e10 × n`, is fitted against an 11.28 MB response and is what ruled the Ethereum-address statement unliftable at ≈130 days.
  **Do not divide it by 3.9**: the fixed term is 66% of an n=1 run and the two terms need not scale together, and the note itself warns that n=1 overstates the marginal cost by 2.9×.
  -  The host window width is 1.74× cheaper again and **is not available to us**: the lifted circuit must be the one the device can prove, and the device path stops at width 5.
- **Re-measure the streamed-verification table** through `zkboo-harness`'s pipe on current proofs.
  The whitepaper's `@table-streaming` scales its wall clock from pre-optimisation proofs by a factor that is now wrong; its peak-memory column is size-independent by construction and is unaffected.
- **Profile the Bitcoin and Taproot address statements** off-device, which closes a stale row in the multi-chain plan for the cost of minutes.
  Lowest priority of the three; drop it first if the night runs short.

### P4 — The split campaign, relaunched

The previous plan, updated for 2.0.0 and given a GPU arm.
Strictly serial, under `/usr/bin/time -l`, at the 12-thread cap, with the machine's state recorded next to each figure.

Order: `4 gate`, `4 forge-count`, `4 forge-order`, `4 forge-vk`, `4 leaves`, `4 root`, `64 leaves`, `64 root`.

- **Clear the leaf cache between response counts.**
  A leaf of an n=4 proof is not a leaf of an n=64 proof: the responses themselves depend on the global challenge, which depends on how many there are.
- **The three forge negatives have never been run.**
  Their first form proved nothing — it corrupted the leaf verifying key, and SP1 looks that up as an artifact name before it checks anything cryptographic, so the run failed at `artifact not found` and never reached the binding it was meant to test.
  `forge-count` and `forge-order` keep a genuine key precisely so that lookup succeeds and SP1's deferred-proof digest is what refuses them, and each asserts that the failure was not the lookup.
- Take the leaf and the root on both arms at n=4.
  That ratio is the Metal leaf time, which is the estimate the streaming argument currently rests on.
- Produce the 64 leaves on the faster arm and time a sample: leaf artefacts are inputs to the root, so only a few of them need timing.
- **`root(64)` is the decisive number.**
  The recorded `root(N) ≈ 204.8 + 5.43N` is a 27× extrapolation from two points that were themselves taken on a loaded machine, and this repository has already been burned by exactly this move once — a configuration gain that was real at n=8 and absent at n=219, because a short run has around a dozen shards against a full run's several hundred.

### P5 — The full-scale lift on the new tree

`lift 219 groth16` under Metal, from prebuilt binaries with no cargo inside the run, against pinned and recorded zkboo revisions.

**Acceptance:** the relying-party gate passes, `forge` is 5/5 at recorded gas, and the journal attests 219 responses at proof format 3 under the new verifying key.

It produces two things at once: this project's headline numbers on the current stack, and **`sp1-metal`'s D7 measured as written for the first time** — which its own verdict names as the thing that would change it.

### P6 — Write-up, commit, review

Figures written into the repository **as they are taken**, flushed per line, with provenance and with the gaps named; several device measurements have already nearly died in scrollback.
Then the knowledge notes here, the whitepaper's re-measurement list and the tables that tonight makes measurable, and the D7 result fed back to `sp1-metal`.
Milestone branch merged `--no-ff`, natural-language commit messages, and an adversarial review of the completed chunk before it is presented.

## Out of scope

- **Anything on the Ledger Flex.**
  The device has been destroyed four times, several statements need a human to approve them, and the pre-registered validation ladder is daylight work.
- **The ERC-4337 account contract and the Solidity journal gate.**
  Still deliberately unstarted; it deserves its own milestone rather than the tail of a measurement night.
- **Pushing.**
  This repository has no remote and nothing here is pushed without being asked.

## Risks, and what is done about each

1. **Memory on the Metal full-scale run.**
   Gated at a mid-size run before launch, with a known and monotone lever if the gate fails.
2. **A Metal-arm proof that does not verify.**
   Two such defects were found and closed on 2026-08-26/27 and *both presented as hangs*, because SP1 retries a failed shard silently and leaves every thread parked — `RUST_LOG=error` is what says otherwise.
   The acceptance everywhere is a proof that verifies, never a proof that was produced.
3. **The elephant re-fit may still say no.**
   That is a finding rather than a failure, and it is the cheapest item on the list.
4. **`RUST_LOG=info` makes 261 MB of log on a full run.**
   Keep it, write it to a file, and keep a separate summary log that a human can read.

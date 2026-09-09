# What everything costs, on this machine

Two rounds sit in this file. The **1.2.0 round** below is current. Everything under "The 2.0.0
round" was measured against the withdrawn ZKBoo 2.0.0 and is kept for the rows the 1.2.0 round has
not re-taken — read those for the method and the shape of the results, never for the numbers, and
never rescale one by a factor observed elsewhere. `instructions-are-not-seconds.md` is this
project's shortest illustration of why that last rule exists.

## Re-measured on zkboo 1.2.0 — 2026-09-07

Same machine (M4 Max, 16 cores / 137 GB), same discipline: `RAYON_NUM_THREADS=12 GOMAXPROCS=12`,
one run at a time on a quiet machine.

⚠️ **Two backend configurations appear below and every row says which.** Runs up to 2026-09-07
used `SP1_METAL_MERKLE=0`, the workaround for a defect in the Metal backend. That defect is fixed
upstream and the workaround was retired on 2026-09-08, so later rows are on the **full** path and
are roughly 2.5–2.9× faster. A figure from one configuration must never be compared with one from
the other without saying so. The full-soundness lift delivered
`(user+sys)/real = 11.88` of 12 requested threads, which is what "quiet" means here — a contended
run reads about 9 and its timings are retracted on sight.

### On-chain

🔴 **The figures this project published before 2026-09-07 measured the test harness as well as
the verifier**, and are corrected here. The Foundry suites hold the fixture in `bytes internal`
fields; Forge resets the EVM access list per test, so reading them inside the measured region
charges a cold SLOAD per 32-byte word of journal and proof. That is ~46,700 gas on the pairing
path and ~34,700 on the gate — a fifth and two fifths of the respective figures — and a relying
contract, which receives both in calldata, pays none of it.

What a relying contract pays for the call:

| step | gas |
| ------------------------------------------------------------- | ------: |
| `SP1VerifierGroth16.verifyProof` (the pairing check) | 225,975 |
| `LiftJournalGate.check` (decode + the full relying-party gate) | 54,615 |
| **together** | **280,590** |

⚠️ The transaction additionally pays intrinsic calldata for the 219-byte journal and 356-byte
proof — about 9,000 gas at 16/byte — plus the 21,000 base. That is outside both figures.

The gate is 24% on top of the proof verification, and it is not optional: `verifyProof` alone
establishes only that some program committed some public values.

For continuity with what was published before, the same calls measured the old way, with the
values read from the test contract's cold storage:

| arrangement | 188-byte journal (2.0.0) | 219-byte journal (1.2.0) | delta |
| ---------------------- | -----------------------: | -----------------------: | -----: |
| memory-resident | 225,838 | 225,917 | +79 |
| warm storage | 228,388 | 228,576 | +188 |
| **cold storage (as published)** | **270,441** | **272,635** | +2,194 |

Both published figures reproduce to the gas in the cold-storage arrangement, which is what
identifies it. The memory-resident row reads 225,917 here and 225,975 in the fixed suite: the two
harnesses reach the call with different memory already allocated, and 58 gas is that difference,
not a discrepancy. Quote the suite's figure — it is the one an unchanged `forge test` prints. The whole 2,194-gas "increase across the release" is one extra cold storage slot:
the journal crossed from six 32-byte words to seven. On the values a caller actually holds, those
31 bytes cost 79 gas. 🔑 An instrument that must hold its own fixture will charge the subject for
holding it — ask where the data lives before believing the meter.

### SP1 zkVM proving, GPU-safe arm

All four rows here are on the **safe** path, both columns, so the comparison is like for like:

| workload | 2.0.0 | 1.2.0 | change |
| ---------------------------------------- | ------------------: | ------------------: | -----: |
| **lift, 219 responses, groth16** | 10,296.1 s / 77.5 GB | **9,927.5 s / 76.5 GB** | −3.58% |
| split: 4 leaves, summed | — | 234.6 s | — |
| split: root over 4 leaves | 112.2 s | 114.5 s | +2.1% |
| split: monolithic 4-response baseline | — | 286.1 s | — |

The root over four leaves is unchanged within noise, which is the prediction the elephant law's
fixed term makes: the root is dominated by the one-off secp256k1 fingerprint traversal, and that
term moved 5.1004e9 → 5.0986e9, i.e. −0.035%. RSS was not taken for the split runs.

### The elephant statement, split

On the **safe** path (`SP1_METAL_MERKLE=0`), which is what both rounds below were measured on:

| | 2.0.0 | 1.2.0 | change |
| ------------------- | -----------: | -----------: | -----: |
| one leaf | 4,207.74 s | 3,919.77 s | −6.84% |
| root over two leaves | 15,589.86 s | 15,476.43 s | −0.73% |
| two leaves + root | 24,005.34 s | 23,315.97 s | −2.87% |

The root is unchanged, as its fixed term predicts. Two 2.0.0-era root runs differ from each other
by 0.62%, so the 0.73% is inside the noise floor the baseline itself sets. The leaf saved 6.84% of
seconds for 11.70% of instructions — the elephant leaf got 5.50% slower per instruction across the
release where the hardened child got 4.74%.

On the **full** path, 2026-09-08, which is what this stack now runs:

| | safe | full | speedup |
| ------------------- | -----------: | ----------: | ------: |
| one leaf | 3,919.77 s | **1,333.82 s** | 2.939× |
| root over two leaves | 15,476.43 s | **5,911.38 s** | 2.618× |
| (hardened-child leaf, for contrast) | 58.35 s | 23.13 s | 2.523× |

🪤 **Three workloads, three speedups, a 16% spread** — one machine, one binary, one release. A
backend speedup belongs to a workload, not to a machine, exactly as a proving rate does.

### The root's cost law — measured, not assumed

**`root instructions = 5.099954e9 + 122,869 × n`**, fitted over eight counts from n=2 to n=24 by
executing the root guest over natively-replayed leaf outputs. Worst residual **0.000011%**.

The marginal is **0.0024% of the fixed term**: at n=219 the root's guest work is +0.523% over n=2.
So the root is a constant to within half a per cent, which is what the projection had been
assuming on an analogy rather than a measurement. 🪤 The 2.0.0 round instead borrowed the
*hardened child's* root marginal (2.55 s per leaf) for the elephant; against the measured law that
was wrong by an order of magnitude.

### Peak memory, with the release separated from the backend

Measured 2026-09-08/09 by re-running both phases on the *older* backend configuration, so that
only the release differs. Per-phase peaks, one process each, n=2:

| | leaf phase | root phase | |
| ------------ | ---------: | ---------: | --- |
| 2.0.0, safe | 76.05 GB | 74.37 GB | baseline |
| 1.2.0, safe | **81.71 GB** | **80.17 GB** | release **+7.43% / +7.79%** |
| 1.2.0, full | 57.40 GB | 53.03 GB | backend **−29.74% / −33.85%** |

🔑 **zkboo 1.2.0 costs about 7.5% more peak memory, on both phases.** It was invisible until the
two variables were separated, because the only comparison available crossed the backend change as
well — and that change is four times larger and points the other way. The confounded figures
(−24.5%, −28.7%) would have credited the release with an improvement it did not make.

The two phases share almost nothing — a view replay against a one-off curve fingerprint traversal
— and agree to within four tenths of a percent, against a baseline whose own repeat spread is
0.43%. That is what makes it a systematic effect rather than scatter.

🪤 The confounded reading flattered us, which is the direction least likely to be interrogated.
Before this run the entry here read "not measured" rather than a number, and that was the right
call: the only pair available could not tell a 7.5% regression from a 30% improvement.

Root *time* on the safe path moved −1.58% to −2.19% across the release, against 1.2.0's own
run-to-run scatter of 1.47% — so "the root is unchanged" holds, now with a measured noise floor
under it.

At full classical-128 soundness:

| | 2.0.0 (safe) | 1.2.0 (safe) | 1.2.0 (full) |
| ----------------------------------- | -------: | -------: | -------: |
| 219 leaves + root, one machine | 11.07 d | 10.12 d | **3.450 d** |
| critical path, one machine per leaf | 5.65 h | 5.41 h | **2.021 h** |
| root's share of the critical path | 79% | 79.8% | **81.7%** |

Every term of the last column is measured or fitted: the leaf from two proofs, the root at n=2
from one, and the root's growth from the eight-point fit above. The only thing never run is a root
over more than two leaves at proving scale.

🔑 The root is still four fifths of the critical path — more so than before, at 81.7% — so more
machines buys nothing. The one-off secp256k1 fingerprint traversal is the only lever left on this
statement, and that is now a measured statement rather than an inferred one.

Split arithmetic at n = 4: split on one machine 349.1 s against a 286.1 s monolith — an overhead of
1.22× — with a critical path of 174.2 s, a 1.64× speedup given four machines, and the root at 65.7%
of that path. 🔑 The root's share is why more machines is not the next lever.

### Guest instruction laws, refitted

| statement | law | marginal change | fixed change |
| ---------------- | -------------------------- | --------------: | -----------: |
| Ethereum address | `5.0986e9 + 2.1148e9 × n` | −14.13% | −0.035% |
| hardened child | `3.0274e7 + 1.9889e7 × n` | −8.18% | −3.8% |
| elephant leaf | 2,114,787,163 (measured) | −11.70% | n/a |

🪤 Three savings for three things, and none predicts another. In particular the elephant leaf was
*predicted* at −14.5% by deriving it from the refitted monolithic law, and that was wrong by more
than the width of the band given; it is measured here, in fifteen seconds, instead. And none of
them is a time saving — see `instructions-are-not-seconds.md`.

At the hardened child's measured rate of 441,791 instructions/second, a monolithic elephant lift at
219 responses is 4.6825e11 instructions ⇒ **12.3 days**. ⚠️ That rate is the hardened child's,
carried across statements, which this project has measured to be wrong by 1.23× in one direction
before. Order of magnitude only.

### Artefacts

| | 2.0.0 | 1.2.0 |
| ----------------------- | ----: | ----: |
| Groth16 proof | 356 B | 356 B |
| journal (public values) | 188 B | 219 B |
| proof file on disk | 1,882 B | 1,913 B |

vkey `0x00f1810f…`, circuit fingerprint `0xbd121f6f…`, proof format id `0x4be5d4e6…`. Evidence:
`fixtures/lift-219-2026-09-07.summary.log`, `fixtures/split-probe-2026-09-07.summary.log`,
`fixtures/elephant-probe-2026-09-07b.summary.log`, `fixtures/relaunch-2026-09-07.summary.log`.

## The 2.0.0 round

> 🚨 **Every figure below was measured against the withdrawn ZKBoo 2.0.0.** Where the section
> above re-takes a row, that row is the one to quote. The rest is the previous round, kept because
> re-taking it costs hours and its method and shape carry.

Every figure measured 2026-08-28 unless a row says otherwise, on an M4 Max (16 cores / 137 GB) at
`RAYON_NUM_THREADS=12 GOMAXPROCS=12`, one run at a time, against the released ZKBoo 2.0.0.
RSS is `/usr/bin/time -l` maximum resident set size for the whole process.

Three arms appear throughout:

- **CPU** — the same binary with every `SP1_METAL_*` switch forced off (`fixtures/cpu-arm-switches.txt`).
  It measures within 0.8% of the published SP1 6.4.0 prover, which was checked separately.
- **GPU safe** — the Metal backend with `SP1_METAL_MERKLE=0`. It *was* the only setting fit for an
  unattended run; the defect that made it so is ✅ fixed upstream and the setting was retired on
  2026-09-08. See `gpu-corruption.md` and `gpu-proving.md`.
- **GPU all** — every phase accelerated. Faster, and produces a proof that fails its own
  verification about once every twenty-eight, then parks the run for ever.

## On-chain

| step | gas |
|---|---:|
| `SP1VerifierGroth16.verifyProof` (the pairing check) | 270,441 |
| `LiftJournalGate.check` (decode + the full relying-party gate) | **85,287** |
| **together** | **~356,000** |

The gate is 32% on top of the proof verification, and it is not optional: `verifyProof` alone
establishes only that some program committed some public values. Measured 2026-08-29,
`verifier/test/LiftJournalGate.t.sol`.

## SP1 zkVM proving

| workload | CPU | GPU safe | GPU all |
|---|---|---|---|
| lift, 1 response, compressed | 276.4 s / 89.7 GB | — | **58.3 s / 43.6 GB** |
| lift, 1 response, groth16 | 423.6 s / — ⁽ᶜ⁾ | — | **208.7 s / 41.5 GB** |
| lift, 8 responses, compressed | 1 141.6 s / 112.4 GB ⁽ᵃ⁾ | **457.2 s / 79.5 GB** | — |
| **lift, 219 responses, groth16** | **23 320.2 s / 112.2 GB** ⁽ᵃ⁾ | **10 296.1 s / 77.5 GB** | — |
| split: one leaf (1 response) | 109.7 s / 82.2 GB | **57.0 s / 58.9 GB** | 23.3 s / 33.9 GB |
| split: root over 4 leaves | 205.6 s / 86.2 GB | 112.2 s / 68.8 GB | 47.9 s / 41.4 GB |
| split: root over 64 leaves | 527.2 s / 96.6 GB | **265.0 s / 80.1 GB** | — |

⁽ᶜ⁾ This cell is the **published SP1 6.4.0** prover, not the switches-off binary the CPU column otherwise means (`fixtures/lift-1-cpu.summary.log`, which carries no configuration banner — it begins at gnark's output). The two arms agree within 0.8% where both were measured, which is why it was placed here, but the column header claims an arm this figure was not taken on.

⁽ᵃ⁾ Measured 2026-08-21 on **zkboo 1.0.0**, not re-run since. The upgrade moved the guest by 0.035%
of its instructions, so they are fair baselines, but they are not from this round.

For reference, the published SP1 6.4.0 prover — no fork at all — on the one-response compressed lift:
**274.9 s / 101.1 GB**. That third arm is why the GPU ratios mean anything: without it a slowed CPU
path would be indistinguishable from a fast GPU.

### The ratios

| | GPU all vs CPU | GPU safe vs CPU |
|---|---:|---:|
| lift, 1 response, compressed | **4.74×** | — |
| lift, 8 responses | — | 2.50× |
| lift, 219 responses | — | **2.27×** |
| one leaf | 4.72× | 1.92× |
| root over 4 | 4.30× | 1.83× |
| root over 64 | — | 1.99× |

**The safe setting costs about 2.4× of the 4.7×.** The Merkle tree is where most of the GPU advantage
lived, and it is the phase that corrupts.

## Two things the table says that no single row does

**Memory tracks the arm, not the workload.** The GPU arm peaks at roughly half the CPU arm's figure
everywhere. And peak barely moves from one response to 219 — 77.5 GB against 79.5 GB — which
confirms what the August sweep found: SP1's peak is a fixed prover allocation, not a function of the
job. That is why the memory ceiling, not the clock, was what put a larger shard size out of reach.

**The split's root is near-fixed, and the GPU does not change that.** `184.2 + 5.36·N` on the CPU,
`102.0 + 2.55·N` on the safe GPU, **interpolated between two points, 4 and 64**. Admitting one more
leaf costs 4.5% of a leaf on the GPU and 4.9% on the CPU.

⚠️ Two points per arm is a line through two measurements, not a fit: it has no residual and says
nothing about linearity *between* them. The supporting evidence is that a third, discarded point (the
loaded-machine n=8 campaign) had a slope within 1.3% — which is corroboration, not a degree of
freedom. Anywhere this note or another says "fitted over 4 to 64", read "the line joining N=4 and
N=64".

## Streaming orchestrator (safe GPU)

8 responses at a modelled 30 s device cadence, one prover: leaves 555.6 s (69.5 s each), root 184.9 s,
**end to end 770.7 s against 980.5 s** for producing everything and then lifting it — 1.27×, at
69.9 GB (⚠️ **unsourced** — `stream-lift-n8-cadence30.summary.log` records no maximum resident set
size at all, so this figure cannot be traced to a log and must be re-measured before it is quoted).
Proving is the bottleneck at that cadence, so a deployment needs two to three machines.

## Native verification, no zkVM (all CPU)

The streamed verifier, at the paper's `N_ε = 438`. This is the ZKBoo layer alone — no SP1 anywhere —
and its memory is in **megabytes**, not gigabytes.

| statement | full proof | sequential | 12 concurrent | peak RSS |
|---|---:|---:|---:|---:|
| Hardened child | 22.6 MB | 1.84 s | **0.26 s** | 3.3 MB |
| Public key `d·G` | 1.31 GB | 137.6 s | **17.9 s** | 6.6 MB |
| Ethereum address | 1.27 GB | 141.1 s | **17.4 s** | 7.0 MB |
| Normal child key | 1.29 GB | 150.0 s | **18.1 s** | 8.0 MB |

Memory grows with responses in flight, not with proof size: 3.5 MB at one, 6.6–8.0 MB at twelve,
across a 58× range of proof size.

## The elephant in the zkVM executor (CPU, execution only — nothing proved)

| responses | instructions | executor wall |
|---:|---:|---:|
| 1 | 7 562 995 674 | 53.4 s |
| 2 | 10 025 871 402 | 69.8 s |
| 4 | 14 950 873 523 | 102.8 s |
| 8 | 24 801 729 491 | 168.7 s |

Peak RSS for the whole four-point sweep: 15.1 GB. Fitted `5.1004e9 + 2.4627e9 × n`, every point within
0.002%. **No elephant response has ever been proved** — the campaign that changes that is running.

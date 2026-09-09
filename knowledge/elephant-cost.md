# The cost of lifting the Ethereum-address statement

> **Every figure below was measured against the withdrawn ZKBoo 2.0.0 and is superseded.**
> The move to zkboo 1.2.0 changed every proof, every circuit fingerprint and every guest verifying key.
> Kept for the method and the shape of the results, not for the numbers.
> Do not quote them, and do not rescale them by a factor seen elsewhere.
>
> **Current figures: `performance-table.md`, the "Re-measured on zkboo 1.2.0" section**, taken 2026-09-07.
> What the port cost and what is now closed: `zkboo-1.2.0-migration.md`.
> Why a superseded number may not be rescaled into a current one, measured rather than asserted: `instructions-are-not-seconds.md`.

Re-measured 2026-08-28 on the 16-core Apple Silicon machine (137 GB), SP1 sdk 6.4.0, **zkboo 2.0.0**.
Everything below is measured on this machine unless it is labelled an extrapolation or a projection.

The first version of this note was written against zkboo 1.0.0 and concluded that this statement was not liftable on any hardware available to the project.
**That conclusion no longer follows**, and the section saying so is kept below rather than deleted.

## Why this statement

The proof-of-seed suite divides in two, and the division is not a matter of degree.

The **hash-only** statements — master key, hardened child, BIP-39 seed — are HMAC-SHA512 chains.
They cost tens of kilobytes per response, and the lifter proves a full classical-128 lift of one in **2 h 51 m 36 s** on the current stack. (The 6 h 28 m this note used to quote is the CPU / zkboo-1.0.0 figure, superseded on both axes — see `performance-table.md`.)

The **secp256k1** statements — public key `d·G`, normal child, and the Ethereum address — put an in-circuit scalar multiplication in front of that, and cost megabytes per response.
Only these bind a proof of seed to an actual Ethereum account:

```
address = Keccak256(x_be ‖ y_be)[12:]   where   Q = (x, y) = d·G
```

This matters for the deployment story rather than the mechanism.
An ERC-4337 account that accepts a lifted proof as authorisation must know *which* legacy account the prover is proving for, and only the address statement establishes that.
A lift of the hardened-child statement demonstrates the machinery end to end; it does not, on its own, migrate anybody's account.

## What was measured

The probe lives in `crates/elephant/` (the circuit and its wire format) and `programs/program-elephant/` (its zkVM guest), deliberately outside the production statement crate: that crate's shape determines the guest ELF and hence the verifying key pinned by the on-chain suite, and a probe must not move it.
The suite was re-run after building and still passes 5/5 at the same key.

### At the circuit

Re-measured 2026-08-28 against zkboo 2.0.0.
The previous figures were taken against an 11.28 MB response and are superseded throughout.

|                             | hardened child | Ethereum address | ratio |
| --------------------------- | -------------: | ---------------: | ----: |
| nonlinear AND-messages      |          6,454 |      **368,123** |   57× |
| response size               |       51,691 B |  **2,909,068 B** |   56× |
| full proof at 219 responses |        11.3 MB |      **0.64 GB** |     — |
| circuit execution           |              — |          63.9 ms |     — |
| native prove, one response  |        ~1.2 ms |           302 ms |     — |
| native verify, one response |          ~2 ms |           126 ms |     — |

The response is 3.88× smaller than the 11.28 MB this note was built on, and the device agrees: 2,907,372 B measured on a Ledger Flex, one input share away, which is what agreement looks like for a quantity with two values.

### In the zkVM, refitted

Four points rather than one, because the fixed term is two thirds of a single-response run and a single point cannot separate it from the marginal one.

| responses |   instructions | executor wall |
| --------: | -------------: | ------------: |
|         1 |  7 562 995 674 |        53.4 s |
|         2 | 10 025 871 402 |        69.8 s |
|         4 | 14 950 873 523 |       102.8 s |
|         8 | 24 801 729 491 |       168.7 s |

**`instructions = 5.1004e9 + 2.4627e9 × n`** — every point within **0.002%** of the fit.

### Term scaling

The superseded law was `2.007e10 + 1.042e10 × n`.
Against it:

|               |   before |         after |     ratio |
| ------------- | -------: | ------------: | --------: |
| fixed term    | 2.007e10 |      5.1004e9 | **3.93×** |
| marginal term | 1.042e10 |      2.4627e9 | **4.23×** |
| total at 219  | 2.302e12 | **5.4443e11** | **4.23×** |

The instruction to re-fit rather than divide by the 3.8× the response size had moved by was therefore worth following.
Dividing the old law by 3.8 would have overestimated the fixed term by **3.6%** and the marginal one by **11.3%** — a factor of three apart, and both in the **same** direction, since the true reductions (3.93× and 4.23×) both exceed 3.8.
The fixed term is still 67% of a single-response run, so a one-point estimate would still overstate full scale by **3.04×** at n = 219 (3.07× asymptotically).

An earlier version of this paragraph said "wrong by 8% on the fixed term and 11% on the marginal one, **in opposite directions**".
Both halves were wrong: no divisor produces 8%, and no divisor puts the two errors on opposite sides.
The conclusion — refit, never divide — is unaffected, which is exactly why the wrong arithmetic survived a reading: it pointed at the right answer.

## What a full lift would cost, now

At 219 responses — classical-128 in the forgery-work-ratio sense, with the usual caveat that stricter definitions rate it lower, and with BN254's ~100-bit pairing security the real ceiling of this track either way:

- **5.4443e11 instructions**, against 2.302e12 before
- **0.64 GB of proof data through the guest**, against 2.47 GB
- **114× the hardened-child statement per response**, against 482×

| at this machine's proving rate                                             |      days |
| -------------------------------------------------------------------------- | --------: |
| CPU, as measured in August (204 687 instr/s)                               |      30.8 |
| GPU with the Merkle tree on the CPU, hardened-child rate (464 000 instr/s) |      13.6 |
| **the same, at the elephant's own measured rate (569 221 instr/s)**        | **11.07** |

**130 days became 11.07.**
Of the 11.7×, **4.23× is the circuit** — zkboo's advice, in-circuit assertions and affine comb — **2.27× is the GPU**, and the remaining **1.23×** is that this statement proves faster per instruction than the one the rate was borrowed from.

The caveat this table used to carry — *"the rate is the hardened-child statement's, assumed to carry … and no elephant response has ever been proved, only executed"* — **is discharged.**
Elephant responses have now been proved, and the rate is measured on one.
It went the favourable way, which is not the way to bet: the same borrowing was 26% pessimistic here and could as easily have been optimistic.

## The conclusion this note used to draw no longer follows

It said: *"not liftable by this route at classical-128 on any hardware currently available to the project.
The gap is about two orders of magnitude."*
**Both halves of that are now wrong.**

The gap is one order of magnitude rather than two, and it is not the whole story, because the second thing that changed is that the responses can be divided.
The split-and-aggregate architecture is built, its cross-leaf Fiat-Shamir gate passes a nine-case refusal matrix with the control accepting, and `root(N)` is measured — see `split-and-streaming.md`.
A leaf is an independent `prove` call, so the leaves distribute over machines and the root is near-fixed.

### Elephant leaf and root

**Measured 2026-08-28**, safe-GPU configuration (`SP1_METAL_MERKLE=0`), 12 Rayon threads.
Leaf: `fixtures/elephant-leaf-retime.summary.log`.
Root: `fixtures/elephant-root-n2.summary.log`, and the re-run `fixtures/elephant-root-n2-rerun.summary.log`.

### The root was run twice, and the second run settles both its repeatability and its provenance

The first root's log records `backend=cpu` and nothing more: the config line did not report the GPU configuration until 2026-08-28 23:31, so **every run before that is in the same position**, GPU and CPU alike, with the arm resting on a filename and on what the operator remembers.
That is not a record.
The line was fixed, and the root was then re-run under it.

|                           |   first run | re-run (banner-confirmed safe GPU) |                              |
| ------------------------- | ----------: | ---------------------------------: | ---------------------------: |
| wall                      | 15,589.86 s |                    **15,493.61 s** |              **0.62% apart** |
| peak RSS                  |    74.37 GB |                           74.70 GB |                         0.4% |
| host instructions retired | 3.049863e15 |                    **3.047954e15** |             **0.063% apart** |
| gate                      |       green |                              green | same fingerprint `4d6c3501…` |

**The instruction counts settle the first run's arm retroactively.**
They agree to 0.063%, and a CPU arm retires about 2.2× more host instructions for the same guest work (the 219-response lift: 4.49e15 on CPU against 2.01e15 on the safe GPU).
A CPU root would therefore have retired roughly 6.7e15, not 3.05e15.
The first run was on the safe GPU.

Host instruction *rate* was tried as the forensic first and does **not** discriminate: every run on this machine retires ~195e9 host instructions per second, CPU and GPU alike, because that is the machine's throughput and not the workload's.
The discriminating quantity is total instructions for *the same work*, which is why it needed a second run of the same thing rather than a comparison across runs.

A 7-second `cargo build` ran during the re-run.
Against 4 h 18 m on a 16-core machine with 12 threads in use it is not material, but it is disclosed rather than omitted.

|                                    | projected |                                     **measured** |                 |
| ---------------------------------- | --------: | -----------------------------------------------: | --------------: |
| **one elephant leaf**              |    ~1.5 h |                   **1 h 10 m 08 s** (4,207.74 s) | **1.26× under** |
| leaf peak RSS                      |         — |                                     **73.38 GB** |                 |
| **an elephant root over 2 leaves** |    ~3.1 h | **4 h 19 m 02 s** (15,541.7 s, mean of two runs) |  **1.39× over** |
| root peak RSS                      |         — |                               **74.4 – 74.7 GB** |                 |

Both **passed**, gate green, root fingerprint `4d6c3501…`.
So the architecture runs on the statement it was built for, which is the thing that was in doubt.
The nine-case cross-leaf refusal matrix runs on the elephant statement too, at 24 leaves, with the control accepting.

### Contaminated leaf timings

Two leaf timings taken alongside the root — 6,193.81 s and 5,469.39 s — were taken while another session was proving on the same machine.
**Both are retracted.**
The clean re-run on an idle machine is 4,207.74 s, so contamination cost **1.47×** and **1.30×**.

The one worth dwelling on is the second.
At 5,469.39 s it sat within 3% of the 5,310 s projection, and that looked like corroboration.
It was two errors cancelling: a measurement 30% too slow against a projection 26% too high.
**An agreement between a contaminated measurement and a projection corroborates neither**, and the tell was available at the time — the run delivered 8.98 of 12 cores against the clean run's 11.91.

### The proving rate does not carry between statements

The 13.6-day estimate applied a rate measured on the hardened-child statement to a workload whose instruction mix is entirely different, and said so.
It does not carry:

|                               |      instructions |       wall |                rate |
| ----------------------------- | ----------------: | ---------: | ------------------: |
| hardened-child lift (assumed) |                 — |          — |     464,000 instr/s |
| **elephant leaf (measured)**  | **2,395,132,641** | 4,207.74 s | **569,221 instr/s** |

**1.23× faster per instruction**, on the same machine and the same safe-GPU configuration.

**The two sides are not the same pipeline**, and the gap is not all attributable to the statement.
The leaf figure is core→compress; the 464,000 figure is a monolithic core→compress→shrink→wrap run whose wrap is a fixed ~171 s this repository measured separately.
Excluding the wrap the borrowed rate is ~471,600 instr/s and the ratio is **1.21×**.
What remains also mixes in program *shape* — a few shards against roughly 280, with per-call overheads amortised differently — which no measurement here separates from the statement's instruction mix.
The honest claim is that a rate borrowed across both statement and pipeline was ~21% pessimistic, not that this statement is 1.23× more efficient.

The leaf's instruction count is measured rather than assumed to be the law's marginal term (`elephant-split 1 execute`): at 2.3951e9 it is **0.973×** of 2.4627e9, the difference being that a leaf parses its own frame and encodes a journal where a marginal response in the monolithic program does neither.

### Projecting the root to 219

`root(2)` alone cannot separate a fixed term from a per-leaf one.
It does not have to, because the per-leaf term was already measured on the hardened-child statement — `root(N) = 102.0 + 2.55·N` on the safe GPU, from the line joining N=4 and N=64 (two points, not a fit — see `performance-table.md`) — and that term is taken to be **statement-independent.**

That is not an assumption; it is visible in the artefacts.
A leaf's compressed proof is:

| statement                   |   compressed leaf proof |
| --------------------------- | ----------------------: |
| Ethereum address (elephant) |             1,272,772 B |
| hardened child              | 1,272,919 – 1,272,923 B |

**0.012% apart** (the hardened-child leaves in the n=64 cache run to 1,272,925 B), and the difference is public-values serialisation.

**This is weaker evidence than the sentence it replaced claimed.**
What is *observed* is that the leaf proofs have the same shape.
That equal shape implies equal per-leaf recursion cost is an inference about how the recursion layer works — the same source-reading category this repository is careful to label elsewhere.
Checking it properly needs an elephant root at two different leaf counts, which does not exist: the marginal term is ~2.55 s against a ~15,537 s fixed term, so separating them needs a lever arm of tens of leaves, at over an hour each.
Taking the inference:

- elephant root fixed term ≈ **15,537 s** (4 h 19 m, mean of two runs) — the secp256k1 fingerprint pass, paid once
- **root(219) ≈ 15,537 + 219 × 2.55 ≈ 16,095 s ≈ 4 h 28 m**

The leaf-admission term is 3.5% of the root at full scale.
**Admitting all 219 leaves costs less than 4% of what the root pays before it admits any of them.**

### What a full split lift costs, revised

|                                     | projected |             **measured** |
| ----------------------------------- | --------: | -----------------------: |
| one leaf                            |    ~1.5 h |               **1.17 h** |
| the root at 219 leaves              |    ~3.1 h |               **4.47 h** |
| total work, 219 leaves and a root   |    ~326 h | **260.4 h (10.85 days)** |
| **wall clock with enough machines** |    ~4.6 h |               **5.64 h** |
| a single machine, monolithic        | 13.6 days |           **11.07 days** |

Only the last row still rests on an inference — the monolithic program's instruction total divided by the leaf's measured rate.
That is a far shorter step than the 13.6-day figure took, since the leaf guest and the monolithic guest do the same replay work; the other rows are arithmetic over two measurements.

### The root is now the wall clock, and more machines will not touch it

**The root is 79% of the wall clock** — 4.47 h against a 1.17 h leaf — and it is 3.71 leaves' *wall clock* for a job that admits leaves in 2.55 s each.

**That it is the fingerprint traversal is measured, not inferred** (`elephant-split 2 execute-root`):

|                                 |      instructions |             |
| ------------------------------- | ----------------: | ----------: |
| the root guest, over 2 leaves   | **5,101,775,808** |             |
| the monolithic law's fixed term |          5.1004e9 | **1.0003×** |

The root's guest work is the fixed term of the instruction law to within **0.03%**.
Admitting the leaves costs so little that two of them do not move the third significant figure.
This was an inference from the monolithic law until the root was executed; it is now a measurement, and it landed almost exactly on the prediction.

It also exposes something the wall clock alone hid: the root proves at **327,250 instr/s** against the leaf's 569,221 — **0.575×**.
Guest instructions are not fungible between the two workloads any more than they are between statements, so the root is not "3.71 leaves of work"; it is 2.13 leaves of guest work proved at 57% of the rate.
Anyone projecting a root from a leaf's rate would be 1.75× out.

That inverts where the effort belongs.
Adding machines drives the leaf half toward zero and leaves 4.47 h standing.
**The lever is the fingerprint pass, not the fleet.**
Whether that traversal can be made cheaper — hoisted into a preprocessed constant pinned by the verifying key, computed incrementally, or replaced by a commitment the leaves already establish — is the open question this measurement raises, and it was not visible before the root was run.

So the honest statement is no longer "not liftable", and it is no longer a projection either: **a single machine needs about eleven days; the architecture that divides it is built, gated on nine refusal cases, and now measured on this statement, and it brings the wall clock to 5.64 hours — of which four and a half are one circuit traversal that no amount of hardware divides.**

## Why the cost is shaped this way

The elephant costs **114× per response against a 56× response size** — about **2.0× more per byte** than the hash statements.
The shape has survived the optimisation round: both figures fell by roughly four, and their ratio moved from 2.5× to 2.0×.
The work is replaying scalar-multiplication gates, and no precompile relieves it: SP1's Keccak accelerator, which does most of the heavy lifting for the hash statements, does nothing for field arithmetic.

The old note also said the zkVM needs 6–12× the instructions the native verifier uses here, against rough parity on the hash statement.
**That comparison has not been re-measured** and is not restated: native instruction counts are not separable from the probe's own, and both sides of the ratio moved.

## The structural obstacle, and the gate that removes it

219 responses look like 219 independent verification tasks that a cluster could divide between machines.
**They are not, and this repository has already demonstrated that they are not.**

The Fiat-Shamir challenge is derived from the view commitments of exactly the responses present in the proof, and the regenerated challenge sequence is compared position by position.
Remove any response and every challenge changes, so the remainder verifies as nothing.
Truncated prefixes, arbitrary sub-ranges, a sparse selection of 9 responses out of 30 iterations, reorderings and duplications were each built and each rejected. (That same property is what makes zkboo's ranged proving safe, and what makes the response count a load-bearing part of the statement.)

So distributing the work cannot mean distributing the proof.
Any division must preserve a **global** challenge check across all 219 responses while splitting the replay between provers — a design in its own right, and the same global-Fiat-Shamir gate the STARK track identified as the correctness condition for any lifter.

**That gate is built.**
`crates/split/`, `programs/program-leaf/` and `programs/program-root/` implement it: a leaf proof per response and a root that re-derives the challenge sequence over the commitments of every response present and requires it to match what each response declared.
Nine refusal cases, each check validated by deleting it, and the control accepting — which is what says the gate reproduces zkboo's own derivation byte for byte.
See `split-and-streaming.md`.
This paragraph's "cannot" is about the naive division and remains true of it; it is no longer true of the problem.

## What the levers are now

1. **Instructions per response** (2.4627e9).
   This is zkboo's circuit, where reductions of 3.75× and then 4.23× have each been found once.
   It is still the larger lever and the one with precedent.
2. **The proving rate** (464 000 instr/s).
   Two things sit here: the GPU's Merkle phase, worth a further ~2.5× on this machine once its intermittent corruption is fixed, and SP1's unpacked CPU prover, worth ~1.9× and costing no soundness.
3. **Dividing the work**, which is new since this note was written and is the only one of the three that changes the wall clock by more than a small factor.

A fourth option is not a lever but a change of target: a statement that binds an account without an in-circuit scalar multiplication would sidestep the whole cost.
Whether one exists is a question for the protocol, not for the lifter.

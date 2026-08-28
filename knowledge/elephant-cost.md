# The cost of lifting the Ethereum-address statement

Re-measured 2026-08-28 on the 16-core Apple Silicon machine (137 GB), SP1 sdk 6.4.0, **zkboo 2.0.0**.
Everything below is measured on this machine unless it is labelled an extrapolation or a projection.

The first version of this note was written against zkboo 1.0.0 and concluded that this statement was
not liftable on any hardware available to the project. **That conclusion no longer follows**, and the
section saying so is kept below rather than deleted.

## Why this statement

The proof-of-seed suite divides in two, and the division is not a matter of degree.

The **hash-only** statements — master key, hardened child, BIP-39 seed — are HMAC-SHA512 chains.
They cost tens of kilobytes per response, and the lifter proves a full classical-128 lift of one in
6 h 28 m.

The **secp256k1** statements — public key `d·G`, normal child, and the Ethereum address — put an
in-circuit scalar multiplication in front of that, and cost megabytes per response. Only these bind
a proof of seed to an actual Ethereum account:

```
address = Keccak256(x_be ‖ y_be)[12:]   where   Q = (x, y) = d·G
```

This matters for the deployment story rather than the mechanism. An ERC-4337 account that accepts a
lifted proof as authorisation must know *which* legacy account the prover is proving for, and only
the address statement establishes that. A lift of the hardened-child statement demonstrates the
machinery end to end; it does not, on its own, migrate anybody's account.

## What was measured

The probe lives in `elephant/` (the circuit and its wire format) and `program-elephant/` (its zkVM
guest), deliberately outside the production statement crate: that crate's shape determines the guest
ELF and hence the verifying key pinned by the on-chain suite, and a probe must not move it. The
suite was re-run after building and still passes 5/5 at the same key.

### At the circuit

Re-measured 2026-08-28 against zkboo 2.0.0. The previous figures were taken against an 11.28 MB
response and are superseded throughout.

| | hardened child | Ethereum address | ratio |
|---|---:|---:|---:|
| nonlinear AND-messages | 6,454 | **368,123** | 57× |
| response size | 51,691 B | **2,909,068 B** | 56× |
| full proof at 219 responses | 11.3 MB | **0.64 GB** | — |
| circuit execution | — | 63.9 ms | — |
| native prove, one response | ~1.2 ms | 302 ms | — |
| native verify, one response | ~2 ms | 126 ms | — |

The response is 3.88× smaller than the 11.28 MB this note was built on, and the device agrees:
2,907,372 B measured on a Ledger Flex, one input share away, which is what agreement looks like for
a quantity with two values.

### In the zkVM, refitted

Four points rather than one, because the fixed term is two thirds of a single-response run and a
single point cannot separate it from the marginal one.

| responses | instructions | executor wall |
|---:|---:|---:|
| 1 | 7 562 995 674 | 53.4 s |
| 2 | 10 025 871 402 | 69.8 s |
| 4 | 14 950 873 523 | 102.8 s |
| 8 | 24 801 729 491 | 168.7 s |

**`instructions = 5.1004e9 + 2.4627e9 × n`** — every point within **0.002%** of the fit.

### The two terms did not scale together, which is why the note said not to divide

The superseded law was `2.007e10 + 1.042e10 × n`. Against it:

| | before | after | ratio |
|---|---:|---:|---:|
| fixed term | 2.007e10 | 5.1004e9 | **3.93×** |
| marginal term | 1.042e10 | 2.4627e9 | **4.23×** |
| total at 219 | 2.302e12 | **5.4443e11** | **4.23×** |

The instruction to re-fit rather than divide by the 3.8× the response size had moved by was
therefore worth following: it would have been wrong by 8% on the fixed term and 11% on the marginal
one, in opposite directions. The fixed term is still 67% of a single-response run, so a one-point
estimate would still overstate full scale by **3.07×**.

## What a full lift would cost, now

At 219 responses — classical-128 in the forgery-work-ratio sense, with the usual caveat that stricter
definitions rate it lower, and with BN254's ~100-bit pairing security the real ceiling of this track
either way:

- **5.4443e11 instructions**, against 2.302e12 before
- **0.64 GB of proof data through the guest**, against 2.47 GB
- **114× the hardened-child statement per response**, against 482×

| at this machine's proving rate | days |
|---|---:|
| CPU, as measured in August (204 687 instr/s) | 30.8 |
| **GPU with the Merkle tree on the CPU, measured 2026-08-28 (464 000 instr/s)** | **13.6** |

**130 days became 13.6.** Of the 9.6×, **4.23× is the circuit** — zkboo's advice, in-circuit
assertions and affine comb — and **2.27× is the GPU**.

⚠️ The rate is the hardened-child statement's, assumed to carry to a workload whose instruction mix
is entirely different, and **no elephant response has ever been proved — only executed**. Treat 13.6
days as an order of magnitude, not a figure.

## The conclusion this note used to draw no longer follows

It said: *"not liftable by this route at classical-128 on any hardware currently available to the
project. The gap is about two orders of magnitude."* **Both halves of that are now wrong.**

The gap is one order of magnitude rather than two, and it is not the whole story, because the second
thing that changed is that the responses can be divided. The split-and-aggregate architecture is
built, its cross-leaf Fiat-Shamir gate passes a nine-case refusal matrix with the control accepting,
and `root(N)` is measured — see `split-and-streaming.md`. A leaf is an independent `prove` call, so
the leaves distribute over machines and the root is near-fixed.

Projected onto the address statement, at the rate above:

| | projected |
|---|---:|
| one leaf (the marginal term; a leaf computes no fingerprint) | ~1.5 h |
| the root's own fingerprint pass over a secp256k1 circuit (paid once) | ~3.1 h |
| total work, 219 leaves and a root | ~326 h |
| **wall clock with enough machines** | **~4.6 h** |

⚠️ **Projected, not measured.** The split probe has only ever run the hardened-child statement; no
elephant leaf or elephant root has been built, and the root's leaf-admission cost is extrapolated
from a fit over 4 to 64 hardened-child leaves. What is measured is the instruction law above and the
proving rate on a different statement.

So the honest statement is no longer "not liftable". It is: **a single machine needs about two weeks,
and the architecture that removes that has been built and gated but not run on this statement.**
Building an elephant leaf and root, and measuring one of each, is the next thing worth doing here.

## Why the cost is shaped this way

The elephant costs **114× per response against a 56× response size** — about **2.0× more per byte**
than the hash statements. The shape has survived the optimisation round: both figures fell by
roughly four, and their ratio moved from 2.5× to 2.0×. The work is replaying scalar-multiplication
gates, and no precompile relieves it: SP1's Keccak accelerator, which does most of the heavy lifting
for the hash statements, does nothing for field arithmetic.

⚠️ The old note also said the zkVM needs 6–12× the instructions the native verifier uses here,
against rough parity on the hash statement. **That comparison has not been re-measured** and is not
restated: native instruction counts are not separable from the probe's own, and both sides of the
ratio moved.

## The structural obstacle, and the gate that removes it

219 responses look like 219 independent verification tasks that a cluster could divide between
machines. **They are not, and this repository has already demonstrated that they are not.**

The Fiat-Shamir challenge is derived from the view commitments of exactly the responses present in
the proof, and the regenerated challenge sequence is compared position by position. Remove any
response and every challenge changes, so the remainder verifies as nothing. Truncated prefixes,
arbitrary sub-ranges, a sparse selection of 9 responses out of 30 iterations, reorderings and
duplications were each built and each rejected. (That same property is what makes zkboo's ranged
proving safe, and what makes the response count a load-bearing part of the statement.)

So distributing the work cannot mean distributing the proof. Any division must preserve a **global**
challenge check across all 219 responses while splitting the replay between provers — a design in
its own right, and the same global-Fiat-Shamir gate the STARK track identified as the correctness
condition for any lifter.

✅ **That gate is built.** `split/`, `program-leaf/` and `program-root/` implement it: a leaf proof
per response and a root that re-derives the challenge sequence over the commitments of every response
present and requires it to match what each response declared. Nine refusal cases, each check
validated by deleting it, and the control accepting — which is what says the gate reproduces zkboo's
own derivation byte for byte. See `split-and-streaming.md`. This paragraph's "cannot" is about the
naive division and remains true of it; it is no longer true of the problem.

## What the levers are now

1. **Instructions per response** (2.4627e9). This is zkboo's circuit, where reductions of 3.75× and
   then 4.23× have each been found once. It is still the larger lever and the one with precedent.
2. **The proving rate** (464 000 instr/s). Two things sit here: the GPU's Merkle phase, worth a
   further ~2.5× on this machine once its intermittent corruption is fixed, and SP1's unpacked CPU
   prover, worth ~1.9× and costing no soundness.
3. **Dividing the work**, which is new since this note was written and is the only one of the three
   that changes the wall clock by more than a small factor.

A fourth option is not a lever but a change of target: a statement that binds an account without an
in-circuit scalar multiplication would sidestep the whole cost. Whether one exists is a question for
the protocol, not for the lifter.

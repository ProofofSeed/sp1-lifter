# GPU proving, measured on this machine

> The absolute timings here were taken against the withdrawn ZKBoo 2.0.0 (see `zkboo-1.2.0-migration.md`).
> The CPU-to-GPU *ratios* are properties of the SP1 backend and should carry; the seconds are of a workload that no longer exists.

`sp1-metal` (tag `v0.1.0`; not yet public) is a Metal backend for SP1 Hypercube 6.4.0 on Apple Silicon.
SP1's own GPU support is CUDA-only, so without it this machine's prover is the CPU.
This note records what it does to *this* workload, which is not the workload it was measured on.

All runs: M4 Max, 16 cores / 137 GB, `RAYON_NUM_THREADS=12 GOMAXPROCS=12`, stock shard thresholds, `RUST_LOG=error`, `/usr/bin/time -l`, one run at a time, nothing else of this project running.
The machine was otherwise idle apart from a hardware monitor holding about 9% of one core (0.6% of the machine) and the editor driving the runs; a previous campaign here was discarded for having been taken under real load, so what was and was not running is recorded rather than asserted.
Statement: BIP-32 hardened child, zkboo 2.0.0, proof format 3, program fingerprint `d4825905…`, verifying key `0x004290a1…`.

## The setup

Six `[patch.crates-io]` entries pinned to the tag, and a `metal` feature naming four of them; the workspace root carries the patches because that is the only place cargo honours them.
Pinned to the tag rather than to a local checkout, so that an edit there cannot move what was measured after it was measured.

**Once the patches are in place, both arms run through the fork.**
The CPU arm is reached by the `SP1_METAL_*` runtime switches, not by dropping the patch — which is the comparison `sp1-metal` recommends, because then nothing but the path under test differs.
The switch list is in `fixtures/cpu-arm-switches.txt` at commit `35ed318`.
That is *not* the same as comparing against the published prover, so the published prover was measured too, as a third arm.

## Compressed proving, one response, three arms

| arm                          |       run 1 |       run 2 |         peak RSS |
| ---------------------------- | ----------: | ----------: | ---------------: |
| published SP1 6.4.0, no fork |    273.70 s |    276.00 s |    98.0–101.1 GB |
| the fork, GPU forced off     |    275.87 s |    276.90 s |     87.8–89.7 GB |
| **the fork, GPU on**         | **57.16 s** | **59.49 s** | **40.5–43.6 GB** |

**4.74× against the fork's own CPU arm, 4.71× against the published prover.**

The third arm is the one that makes the ratio mean anything.
The fork's CPU path measures within **0.8%** of the published prover, so the speedup is the GPU doing work and not the CPU arm having been slowed down to make room for it.

**Peak memory more than halves**, 98.0 GB to 43.6 GB.
That was expected to be the risk in this integration and is instead its second result: this workload's memory ceiling was the constraint that put a larger shard size out of reach on a 137 GB machine, and the GPU arm has 94 GB of headroom where the CPU arm had 36.

### Discrepancy against sp1-metal's compressed figure

Its own figure is **2.9×–3.1×**, over three synthetic guests, measured twice on an idle machine and agreeing to 0.02×; whole Bitcoin and Ethereum statements median 2.98×.
We measure 4.74× on the lift.

The difference is not explained here, and the honest reading is that it is a workload effect rather than a discrepancy: `sp1-metal`'s own finding is that the ratio moves with the *shape* of the work rather than with what it computes, and that a proof with ordinary work around its precompiles scores better than one saturated by them.
Its verdict document names the missing measurement precisely — "execute to compressed proof, on the canonical workload" — and says the workload it was measured on was the wrong one.
This is that measurement at n = 1; the full-scale one follows.

## Groth16, one response

| arm                 |     wall | peak RSS |
| ------------------- | -------: | -------: |
| published SP1 6.4.0 | 423.55 s |        — |
| the fork, GPU on    | 208.71 s |  41.5 GB |

2.03×, and the compression of the ratio is the shrink-and-wrap stage, which gains nothing from the GPU.
That is measurable rather than asserted — subtracting each arm's compressed run at one response from its groth16 run leaves **150.4 s on the GPU and 147.2 s on the CPU, 2% apart**, so the stage costs the same whichever arm produced the proof it wraps.
It is 72% of the GPU run and 35% of the CPU one, which is the whole of why the 3.6× on compressed proving becomes 2.03× here. gnark proper is ~26 s of it.
At full scale the stage is a fixed ~171 s against hours, so the ratio recovers.

This paragraph used to say the wrap was "roughly a quarter of the run" at one response.
That matches nothing: the stage is 72%/35% depending on arm, and gnark alone is 12%/6%.

## Correctness

Not taken on trust, in this order:

- **Seven phases run both ways and compared element for element on every call** — `ZEROCHECK`, `GKR_ROUND`, `ENCODE`, `FIRST_LAYER`, `BATCH`, `JAGGED_VALUES`, `JAGGED_ASSIST` all set to `check` for a full `lift 1 compressed`.
  No disagreement, and the proof verified.
  That mode makes a disagreement fatal — `panic!` at `sum_as_poly.rs:486` — so it is a check that can fail, and it costs 16% — 198.15 s (`fixtures/metal-parity-check.summary.log` at commit `35ed318`) against 170.62 s (`fixtures/metal-warmup.summary.log` at commit `35ed318`).
- **The verifying key and the public values are byte-identical across arms.**
  The patches touch host proving only; the guest ELF, and therefore the key, is untouched.
  Confirmed by producing the same fixture on each arm and comparing.
- **`forge` 5/5 at 270,441 gas (retracted: harness-inflated; see `performance-table.md`)** on the GPU-produced fixture, at the same gas as the CPU-produced one, as a constant-size proof should be.

The Groth16 proof bytes themselves differ between arms, which is expected: gnark's proving is randomised.

**None of that was sufficient, and the next section is why.**
Every check above passed, and the backend still produced a wrong proof once the run was long enough.
What the checks establish is that the compared phases compute the right thing on this workload; what they cannot reach is the one accelerated phase that has no comparison mode.

## The fast configuration is not usable

Everything above was measured with every phase accelerated, in a configuration that at the time **corrupted a proof about once every twenty-eight**: an intermittent Merkle root mismatch that fails SP1's own shard verification and then parks the run forever.
Full account in `gpu-corruption.md`.

No invalid proof was ever accepted — the failure is a proof being refused, not one getting through — so the risk was availability rather than soundness.
But an unattended run of any length did not finish, and the workaround (`SP1_METAL_MERKLE=0`, the Merkle tree left on the CPU) cost most of the advantage.

## Fixed upstream, and the workaround is retired — 2026-09-08

The defect was found, root-caused and repaired in `sp1-metal` weeks ago, on a revision this repository was not building; the patch now points at `rev = cab9ba36db31`.
It was never the Merkle kernel — it was a buffer hand-off table keyed on an address whose allocation it did not keep alive, on the route into that kernel.
`SP1_METAL_REUSE` now defaults off, removing the hazard class.

**64 leaf proofs under `SP1_METAL_MERKLE=check`, which builds both trees and compares every digest of every layer: no disagreement.**
Then the plain timing, on this machine, on the real programs:

|                                 |      CPU | GPU, Merkle off |    GPU, all on |
| ------------------------------- | -------: | --------------: | -------------: |
| hardened-child leaf             | 109.72 s |   57.00–59.71 s |    **23.13 s** |
| speedup over CPU                |    1.00× |           1.92× |      **4.74×** |
| peak RSS, 8 leaves in a process |        — |          ~59 GB |   **35.38 GB** |
| elephant leaf                   |        — |      3,919.77 s | **1,333.82 s** |
| speedup over the safe setting   |        — |           1.00× |     **2.939×** |

**So the number to plan against on this machine is 4.7× again**, and the safe setting is no longer the configuration long runs use.

**The two statements do not share a speedup.**
The hardened child gains 2.53× over the safe setting and the elephant 2.94× — a 16% difference between two workloads on one machine, one release, one binary.
A backend speedup is a property of the workload, not of the machine, as is a proving rate.
Neither figure may be carried to a third statement.

And the mechanism is legible in the numbers: check mode builds **both** trees and costs 59.53 s, the safe setting builds only the CPU tree and costs 57–60 s, and the GPU tree alone costs 23.13 s.
The CPU Merkle tree was essentially the entire 2.5× the workaround was charging.

## Effect beyond wall clock

The split-and-aggregate architecture makes device streaming and proving a *max* rather than a *sum*, and its balance condition is whether one leaf keeps pace with one response arriving.
The device delivers a response every ~29–30 s of measured work.
Until now the leaf time on the fast arm was an estimate; this is the first arm-to-arm ratio measured on the real program.
With the defect fixed, the fast arm is the one that runs: a leaf at 23.13 s against a response arriving every ~29–30 s means one leaf keeps pace with one response, with room to spare.

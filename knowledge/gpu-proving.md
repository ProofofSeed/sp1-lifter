# GPU proving, measured on this machine

`sp1-metal` (hashberg-io, tag `v0.1.0`) is a Metal backend for SP1 Hypercube 6.4.0 on Apple Silicon.
SP1's own GPU support is CUDA-only, so without it this machine's prover is the CPU.
This note records what it does to *this* workload, which is not the workload it was measured on.

All runs: M4 Max, 16 cores / 137 GB, `RAYON_NUM_THREADS=12 GOMAXPROCS=12`, stock shard thresholds, `RUST_LOG=error`, `/usr/bin/time -l`, one run at a time, nothing else of this session running.
The machine was otherwise idle apart from a hardware monitor holding about 9% of one core (0.6% of the machine) and the editor session driving the runs; a previous campaign here was discarded for having been taken under real load, so what was and was not running is recorded rather than asserted.
Statement: BIP-32 hardened child, zkboo 2.0.0, proof format 3, program fingerprint `d4825905…`, verifying key `0x004290a1…`.

## The setup

Six `[patch.crates-io]` entries pinned to the tag, and a `metal` feature naming four of them; the workspace root carries the patches because that is the only place cargo honours them.
Pinned to the tag rather than to the local checkout at `<sp1-metal>`, so that an edit there cannot move what was measured after it was measured.

⚠️ **Once the patches are in place, both arms run through the fork.** The CPU arm is reached by the `SP1_METAL_*` runtime switches, not by dropping the patch — which is the comparison `sp1-metal` recommends, because then nothing but the path under test differs. The switch list is in `fixtures/cpu-arm-switches.txt`.
That is *not* the same as comparing against the published prover, so the published prover was measured too, as a third arm.

## Compressed proving, one response, three arms

| arm | run 1 | run 2 | peak RSS |
|---|---:|---:|---:|
| published SP1 6.4.0, no fork | 273.70 s | 276.00 s | 98.0–101.1 GB |
| the fork, GPU forced off | 275.87 s | 276.90 s | 87.8–89.7 GB |
| **the fork, GPU on** | **57.16 s** | **59.49 s** | **40.5–43.6 GB** |

**4.74× against the fork's own CPU arm, 4.71× against the published prover.**

The third arm is the one that makes the ratio mean anything. The fork's CPU path measures within **0.8%** of the published prover, so the speedup is the GPU doing work and not the CPU arm having been slowed down to make room for it.

**Peak memory more than halves**, 98.0 GB to 43.6 GB. That was expected to be the risk in this integration and is instead its second result: this workload's memory ceiling was the constraint that put a larger shard size out of reach on a 137 GB machine, and the GPU arm has 94 GB of headroom where the CPU arm had 36.

### This is higher than sp1-metal measures for compressed proving

Its own figure is **2.9×–3.1×**, over three synthetic guests, measured twice on an idle machine and agreeing to 0.02×; whole Bitcoin and Ethereum statements median 2.98×.
We measure 4.74× on the lift.

The difference is not explained here, and the honest reading is that it is a workload effect rather than a discrepancy: `sp1-metal`'s own finding is that the ratio moves with the *shape* of the work rather than with what it computes, and that a proof with ordinary work around its precompiles scores better than one saturated by them.
Its verdict document names the missing measurement precisely — "execute to compressed proof, on the canonical workload" — and says the workload it was measured on was the wrong one. This is that measurement at n = 1; the full-scale one follows.

## Groth16, one response

| arm | wall | peak RSS |
|---|---:|---:|
| published SP1 6.4.0 | 423.55 s | — |
| the fork, GPU on | 208.71 s | 41.5 GB |

2.03×, and the compression of the ratio is the wrap: gnark is Go on the CPU and gains nothing, and at one response it is roughly a quarter of the run. At full scale it is a fixed ~171 s against hours.

## Correctness

Not taken on trust, in this order:

- **Seven phases run both ways and compared element for element on every call** — `ZEROCHECK`, `GKR_ROUND`, `ENCODE`, `FIRST_LAYER`, `BATCH`, `JAGGED_VALUES`, `JAGGED_ASSIST` all set to `check` for a full `lift 1 compressed`. No disagreement, and the proof verified. That mode makes a disagreement fatal — `panic!` at `sum_as_poly.rs:486` — so it is a check that can fail, and it costs 16% (198.15 s against 170.62 s).
- **The verifying key and the public values are byte-identical across arms.** The patches touch host proving only; the guest ELF, and therefore the key, is untouched. Confirmed by producing the same fixture on each arm and comparing.
- **`forge` 5/5 at 270,441 gas** on the GPU-produced fixture, at the same gas as the CPU-produced one, as a constant-size proof should be.

The Groth16 proof bytes themselves differ between arms, which is expected: gnark's proving is randomised.

## Why this matters here beyond the clock

The split-and-aggregate architecture makes device streaming and proving a *max* rather than a *sum*, and its balance condition is whether one leaf keeps pace with one response arriving.
The device delivers a response every ~29–30 s of measured work.
Until now the leaf time on the fast arm was an estimate; this is the first arm-to-arm ratio measured on the real program.

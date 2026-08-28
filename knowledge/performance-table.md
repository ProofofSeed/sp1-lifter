# What everything costs, on this machine

Every figure measured 2026-08-28 unless a row says otherwise, on an M4 Max (16 cores / 137 GB) at
`RAYON_NUM_THREADS=12 GOMAXPROCS=12`, one run at a time, against the released ZKBoo 2.0.0.
RSS is `/usr/bin/time -l` maximum resident set size for the whole process.

Three arms appear throughout:

- **CPU** — the same binary with every `SP1_METAL_*` switch forced off (`fixtures/cpu-arm-switches.txt`).
  It measures within 0.8% of the published SP1 6.4.0 prover, which was checked separately.
- **GPU safe** — the Metal backend with `SP1_METAL_MERKLE=0`. **The only setting fit for an
  unattended run**; see `gpu-corruption.md`.
- **GPU all** — every phase accelerated. Faster, and produces a proof that fails its own
  verification about once every twenty-eight, then parks the run for ever.

## SP1 zkVM proving

| workload | CPU | GPU safe | GPU all |
|---|---|---|---|
| lift, 1 response, compressed | 276.4 s / 89.7 GB | — | **58.3 s / 43.6 GB** |
| lift, 1 response, groth16 | 423.6 s / — | — | **208.7 s / 41.5 GB** |
| lift, 8 responses, compressed | 1 141.6 s / 112.4 GB ⁽ᵃ⁾ | **457.2 s / 79.5 GB** | — |
| **lift, 219 responses, groth16** | **23 320.2 s / 112.2 GB** ⁽ᵃ⁾ | **10 296.1 s / 77.5 GB** | — |
| split: one leaf (1 response) | 109.7 s / 82.2 GB | **57.0 s / 58.9 GB** | 23.3 s / 33.9 GB |
| split: root over 4 leaves | 205.6 s / 86.2 GB | 112.2 s / 68.8 GB | 47.9 s / 41.4 GB |
| split: root over 64 leaves | 527.2 s / 96.6 GB | **265.0 s / 80.1 GB** | — |

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
`102.0 + 2.55·N` on the safe GPU, fitted over 4 → 64. Admitting one more leaf costs 4.5% of a leaf on
either arm.

## Streaming orchestrator (safe GPU)

8 responses at a modelled 30 s device cadence, one prover: leaves 555.6 s (69.5 s each), root 184.9 s,
**end to end 770.7 s against 980.5 s** for producing everything and then lifting it — 1.27×, at
69.9 GB. Proving is the bottleneck at that cadence, so a deployment needs two to three machines.

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

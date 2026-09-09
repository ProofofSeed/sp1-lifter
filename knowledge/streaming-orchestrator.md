# Proving leaves as responses arrive

> **Every figure below was measured against the withdrawn ZKBoo 2.0.0 and is superseded.**
> The move to zkboo 1.2.0 changed every proof, every circuit fingerprint and every guest verifying key.
> Kept for the method and the shape of the results, not for the numbers.
> Do not quote them, and do not rescale them by a factor seen elsewhere.
>
> **Current figures: `performance-table.md`, the "Re-measured on zkboo 1.2.0" section**, taken 2026-09-07.
> What the port cost and what is now closed: `zkboo-1.2.0-migration.md`.
> Why a superseded number may not be rescaled into a current one, measured rather than asserted: `instructions-are-not-seconds.md`.

`host/src/orchestrator.rs` and the `stream-lift` driver.
Built 2026-08-28.

The monolithic lifter cannot start until the last response exists, so device time and proving time **add**.
A leaf is an independent `prove` call whose input is complete the moment its response lands, so they can **overlap** instead.
This is the orchestration that turns that structural fact into a pipeline: a producer emitting one response at a time, a pool of leaf provers, and a root fired when the last leaf lands.
It needs no change to SP1, to `zkboo`, or to the contract.

The root commits the same `LiftJournal` the monolithic lifter does, and the driver puts it through the same relying-party gate rather than trusting the pipeline that produced it.

## What is modelled, and what is not

The producer is a real ZKBoo prover: the whole challenge phase first — the Fiat-Shamir challenge is global, so no response can exist before it — and then responses one at a time, in order, by the same mechanism a device uses.

**The device's clock is not modelled by default, and without it the measurement is empty.**
This machine produces a hardened-child response in milliseconds where a Ledger Flex takes ~30 s, so the pipeline is starved and there is nothing to overlap.
`stream-lift <n> <pool> <interval>` sleeps out an arrival interval to model the cadence.

> The first version of this stamped each response's arrival time inside the channel `send`, which **blocks** once the pool is busy.
> That folded the pool's backpressure into the producer's clock and reported a 1.19× overlap saving on a run where production took 20 ms and there was nothing to overlap at all.
> The stamp is now taken when the response is finished, before the send, and the un-modelled run correctly reports **1.00×**.

## Measured (2026-08-28, safe GPU setting, one prover)

|                                   |  no interval | 30 s cadence |
| --------------------------------- | -----------: | -----------: |
| responses                         |            4 |            8 |
| challenge phase                   |       0.01 s |       0.02 s |
| last response produced            |       0.02 s |     240.02 s |
| leaf proving, summed              |     238.56 s |     555.59 s |
| root                              |     117.99 s |     184.85 s |
| **end to end**                    | **356.57 s** | **770.68 s** |
| produced-then-lifted (no overlap) |     356.57 s |     980.45 s |
| **overlap saved**                 |    **1.00×** |    **1.27×** |

**The "no interval" column has been re-run with its log kept** — `fixtures/stream-lift-n4-control.summary.log`, 2026-08-29, banner-confirmed safe GPU:

|                      |             re-run (logged) | the unlogged original |
| -------------------- | --------------------------: | --------------------: |
| leaf proving, summed | **228.34 s** (57.09 s mean) |    238.56 s (59.64 s) |
| root                 |                **111.99 s** |              117.99 s |
| end to end           |                **340.44 s** |              356.57 s |
| overlap              |                   **1.00×** |                 1.00× |
| peak RSS             |                **72.97 GB** |          not recorded |

(The control was run twice, before and after a change to the orchestrator's accounting; the two agree within 1% — 226.39 / 110.52 / 337.03 / 71.79 GB and the row above.
The log holds the second.)

The original column was real but its log was never saved, so it could not be traced to a file — the same provenance failure this project has recorded before.
The re-run is 5% faster across the board and confirms the shape, including the 1.00× that says a machine producing responses instantly has nothing to overlap.
**Quote the logged column.**

**It also resolves the root discrepancy below.**
At n=4 the orchestrator's root takes 111.99 s against the `102.0 + 2.55·N` line's 112.2 s — **0.2%** (110.52 s, 1.5%, on the first of the two runs).
So the line does describe a root the orchestrator drives, and the n=8 cadence run's 184.85 s against a predicted 122.4 s is an anomaly of that run, not a systematic property of the orchestrator's root.

**The cadence column's per-leaf cost is 69.45 s**, not the 57.00 s an isolated leaf costs and not the 59.6 s of the control — 22% and 17% above them, on a run whose own output says proving is the bottleneck.
Nothing here explains that gap; it is recorded rather than resolved.

**The root in the cadence run took 184.85 s over 8 leaves**, against the line's 122.4 s — 1.51×.
It is not setup (`root_wall` starts after `client.setup` returns) and it is not the leaf count.
Since the n=4 control matches the line to 1.5%, this is an anomaly of that single run and remains unexplained; it is not evidence that the line is wrong.

The 1.00× is the control and it is the right answer: with production free there is nothing to overlap, and a pipeline that claimed otherwise would be measuring itself.

At a 30 s cadence against a ~60–70 s leaf, **proving is the bottleneck** — the report says which side is, by comparing time provers spent idle against time finished responses spent queued.
One machine keeps pace only with an arrival interval above its leaf time, so at the device's real cadence a deployment needs **two to three machines**, which is what the split-and-streaming note already concluded from the leaf time alone.
The pipeline confirms it end to end rather than by arithmetic.

## Holding a second proving key costs 54% of every leaf

The orchestrator's first version called `client.setup` for both the leaf and the root program up front, because that reads naturally.
Leaves then proved at **91.8 s** against **57.0 s** for the identical proof in `split-probe`.

Moving the root's `setup` to after the streaming phase brings it back to **59.6 s** — measured at the same response count, in the same build, changing nothing else.

**A second SP1 proving key resident during proving costs about 54%.**
Nothing warns about it, and the natural way to write the driver is the slow way.
Anything that proves against more than one program should set up one at a time.

## The refusal matrix is only exponentially strong in the number of leaves

`split-probe 2 gate` reported **G1 accepting a leaf set it must refuse** — and that is the gate being honest rather than broken.

The challenge-based cases refuse because a re-derived party sequence stops matching the declared one.
A party is one of three, so a tampered set of `k` leaves matches **by chance with probability 3⁻ᵏ**.
At two leaves that is one in three.
At the four this probe defaulted to, each such case had roughly a **3.7%** chance of passing by luck — small enough to have never happened in any run here, and far too large for a matrix that is the split's whole soundness argument.

The matrix now runs at **24 leaves whatever the proving count** — chance pass of 3.5e-12 (3⁻²⁴) — and says so when it overrides.
It costs no proving, so the floor is free.

This is the same property that makes ZKBoo itself sound only at `N_ε` repetitions, appearing one level up.
It should have been obvious and was not until a two-leaf run made it fail.

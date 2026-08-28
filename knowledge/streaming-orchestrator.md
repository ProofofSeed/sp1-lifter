# Proving leaves as responses arrive

`host/src/orchestrator.rs` and the `stream-lift` driver. Built 2026-08-28.

The monolithic lifter cannot start until the last response exists, so device time and proving time
**add**. A leaf is an independent `prove` call whose input is complete the moment its response lands,
so they can **overlap** instead. This is the orchestration that turns that structural fact into a
pipeline: a producer emitting one response at a time, a pool of leaf provers, and a root fired when
the last leaf lands. It needs no change to SP1, to `zkboo`, or to the contract.

The root commits the same `LiftJournal` the monolithic lifter does, and the driver puts it through
the same relying-party gate rather than trusting the pipeline that produced it.

## What is modelled, and what is not

The producer is a real ZKBoo prover: the whole challenge phase first — the Fiat-Shamir challenge is
global, so no response can exist before it — and then responses one at a time, in order, by the same
mechanism a device uses.

**The device's clock is not modelled by default, and without it the measurement is empty.** This
machine produces a hardened-child response in milliseconds where a Ledger Flex takes ~30 s, so the
pipeline is starved and there is nothing to overlap. `stream-lift <n> <pool> <interval>` sleeps out
an arrival interval to model the cadence.

> 🪤 The first version of this stamped each response's arrival time inside the channel `send`, which
> **blocks** once the pool is busy. That folded the pool's backpressure into the producer's clock and
> reported a 1.19× overlap saving on a run where production took 20 ms and there was nothing to
> overlap at all. The stamp is now taken when the response is finished, before the send, and the
> un-modelled run correctly reports **1.00×**.

## Measured (2026-08-28, safe GPU setting, one prover)

| | no interval | 30 s cadence |
|---|---:|---:|
| responses | 4 | 8 |
| challenge phase | 0.01 s | 0.02 s |
| last response produced | 0.02 s | 240.02 s |
| leaf proving, summed | 238.56 s | 555.59 s |
| root | 117.99 s | 184.85 s |
| **end to end** | **356.57 s** | **770.68 s** |
| produced-then-lifted (no overlap) | 356.57 s | 980.45 s |
| **overlap saved** | **1.00×** | **1.27×** |

The 1.00× is the control and it is the right answer: with production free there is nothing to
overlap, and a pipeline that claimed otherwise would be measuring itself.

At a 30 s cadence against a ~60–70 s leaf, **proving is the bottleneck** — the report says which side
is, by comparing time provers spent idle against time finished responses spent queued. One machine
keeps pace only with an arrival interval above its leaf time, so at the device's real cadence a
deployment needs **two to three machines**, which is what the split-and-streaming note already
concluded from the leaf time alone. The pipeline confirms it end to end rather than by arithmetic.

## 🚨 Holding a second proving key costs 54% of every leaf

The orchestrator's first version called `client.setup` for both the leaf and the root program up
front, because that reads naturally. Leaves then proved at **91.8 s** against **57.0 s** for the
identical proof in `split-probe`.

Moving the root's `setup` to after the streaming phase brings it back to **59.6 s** — measured at the
same response count, in the same build, changing nothing else.

**A second SP1 proving key resident during proving costs about 54%.** Nothing warns about it, and the
natural way to write the driver is the slow way. Anything that proves against more than one program
should set up one at a time.

## 🚨 The refusal matrix is only exponentially strong in the number of leaves

`split-probe 2 gate` reported **G1 accepting a leaf set it must refuse** — and that is the gate being
honest rather than broken.

The challenge-based cases refuse because a re-derived party sequence stops matching the declared one.
A party is one of three, so a tampered set of `k` leaves matches **by chance with probability 3⁻ᵏ**.
At two leaves that is one in three. At the four this probe defaulted to, each such case had roughly a
**3.7%** chance of passing by luck — small enough to have never happened in any run here, and far too
large for a matrix that is the split's whole soundness argument.

The matrix now runs at **24 leaves whatever the proving count** — chance pass below 3e-12 — and says
so when it overrides. It costs no proving, so the floor is free.

This is the same property that makes ZKBoo itself sound only at `N_ε` repetitions, appearing one
level up. It should have been obvious and was not until a two-leaf run made it fail.

# An intermittent wrong proof on the GPU path

Found 2026-08-28 while proving leaf after leaf on the Metal backend (`sp1-metal` v0.1.0, SP1 6.4.0).
This is a defect in that backend, not in the lifter, and it is recorded here because it is what the lifter has to plan around.

## What happens

A proof fails its own verification with

```
ERROR task failed with retryable error: shard verification failed: invalid shard proof:
  invalid pcs opening proof: dense pcs verification failed: PCS error:
  Query failed to verify: root mismatch
```

and the run then **stops making progress and never returns**.
SP1 treats the failure as retryable, retries the shard, and parks; nothing is printed after the first line and no process exits.
Only `RUST_LOG=error` shows the cause at all — `sp1-metal`'s own notes warn about exactly this, that a wrong proof on this backend presents as a hang.

## What is and is not at risk

**No invalid proof is accepted.** The failure is SP1's own shard verification refusing a proof the GPU produced, before anything downstream sees it, and the driver's relying-party gate never runs. The risk here is availability, not soundness.

**Unattended runs are the casualty.** A long campaign parks silently in the middle of the night and produces nothing.

## Rate

Three occurrences over the **84** single-response leaf proofs the three runs completed, each proof
about 23 s:

| run | configuration | failed at | phase |
|---|---|---:|---|
| 1 | defaults | leaf 8 | core shard verification |
| 2 | defaults | leaf ~36 | `lift/deferred verify` (recursion) |
| 3 | `SP1_METAL_UPLOAD_FRESH=1` | leaf 40 | `lift/deferred verify` (recursion) |

**One failure per 28 proofs**, or about one per 11 minutes of proving. (This note previously said
"one per 20–25 proofs, about one per 8 minutes" a few lines above where it computes 3/84 correctly;
the denominator is 84, not "roughly a hundred", and the two statements have been reconciled.)
It is **not deterministic in the input**: leaf 8 failed in run 1 and succeeded in runs 2 and 3.
It appears in both core proving and the recursion layer, which share a commitment scheme.

## What has been excluded

- **It is not how the data reaches the device.** `SP1_METAL_UPLOAD_FRESH=1` gives every upload a private buffer, sharing nothing and outliving nothing — the backend's own diagnostic for this question, whose documentation says in as many words that a corruption surviving it is not in the upload path. It survived, at leaf 40.
- **It is not a wrong kernel of the ordinary sort.** A generated kernel that reads the wrong column or applies the wrong constant is wrong on every row and every run; this is wrong on about one run in twenty. The backend's own per-chip prefix comparison, and a full seven-phase `=check` run on this workload, both passed.

## What is suspected, and why

`MERKLE` — the trace commitment's Merkle tree.

The error is a **Merkle root mismatch**: a query opened a path that did not hash to the committed root. Of the phases that could produce that, the encode and both jagged phases have comparison modes and passed them; **the Merkle tree is the one accelerated phase with no comparison mode at all**, so it is both the best fit for the message and the one place a fault could not have been caught by the checks already run.

## The evidence for it, and the working configuration

`SP1_METAL_MERKLE=0` leaves the trace commitment's Merkle tree on the CPU and accelerates everything else.

**115 consecutive leaf proofs, no failure** — a run of 51 (stopped by its own wall-clock cap) and a
complete run of 64.

The rate to compare against comes from the arm that failed: three failures over the 84 proofs those
three runs completed, so about one in 28. Over 115 proofs that predicts 4.1 failures, and the chance
of seeing none is **1.6%**. Neither number is precise — three events is a thin basis for a rate, and
the interval around it is wide — so this is evidence at about the two-percent level rather than a
demonstration.

That is evidence and not proof. What it does not do is identify the fault inside the Merkle phase,
and the attribution rests on the phase being the one the error message names and the one place the
existing comparison modes could not have looked.

**It costs most of the speedup.** A leaf goes from 23.26 s to 57.00 s, so the backend drops from
**4.7× over this machine's CPU prover to 1.92×**. The Merkle tree is where most of the advantage was.

| | CPU | GPU, Merkle off | GPU, all on |
|---|---:|---:|---:|
| leaf (one response) | 109.72 s | 57.00 s | 23.26 s |
| speedup | 1.00× | 1.92× | 4.72× |
| survives a long run | yes | yes, 115 proofs | **no** |

Every long run in this repository uses `SP1_METAL_MERKLE=0` until this is fixed upstream.

## What a fix would need, and what to hand over

For whoever picks this up in `sp1-metal`:

- **The Merkle phase has no `=check` mode.** Seven phases have one and this does not, which is why
  the fault survived a full seven-phase checked run. Giving it one would both catch this and localise
  it, and on the evidence here it is the single most valuable thing to add.
- **It reproduces in a process that proves many times**, not in one that proves once. Every failure
  here came from a driver making dozens of sequential `prove` calls; five single-proof runs in the
  same session were clean. That points at state carried across proofs rather than at a kernel.
- **`SP1_METAL_UPLOAD_FRESH=1` does not prevent it**, so by that switch's own documentation it is not
  in the upload path — which leaves the tree's own construction or the buffers it keeps.
- **The failure parks the run.** SP1 calls it retryable, retries, and never returns or exits. A run
  left overnight produces nothing and says nothing. Even without a fix, failing loudly would turn a
  silent all-night loss into a five-minute one.

Logs: `fixtures/gpu-failure-1.summary.log` and `gpu-failure-2.summary.log` (the two default-configuration failures), `repro-gpu-fresh.summary.log` (the private-buffer run that failed at leaf 40), `repro-gpu-nomerkle.summary.log` and `split-n64-leaves-gpu.summary.log` (the 51 and 64 clean proofs with the Merkle tree on the CPU).

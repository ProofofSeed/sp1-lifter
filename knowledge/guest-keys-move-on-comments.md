# A comment moves a guest's verifying key

Established 2026-08-29 by experiment, after an adversarial review prompted a routine re-check.

## What happened

Three crates that the zkVM guests link — `statement/`, `split/`, `elephant/` — were edited to fix stale and missing security documentation.
**Every changed line began with `///`.**
No code, no constant, no dependency, no feature.

`vkeys` then reported the hardened-child **leaf** key had moved:

    0x007d48ce67ea51b89c2d168f092189989f85beeaff5e0c359bbcb697362dd8e6   before
    0x00aa247993f247909f1a9cf742f9757400edd13922ab14988dc3fcf50c92a711   after

Reverting the three files restored the old key exactly; re-applying them reproduced the new one.
So it is deterministic, and it is caused by the comments.

## What makes it worth writing down

**It is not uniform across guests, which makes it impossible to predict by reading the diff.**

| guest                       | links an edited crate                           | key moved |
| --------------------------- | ----------------------------------------------- | --------- |
| `program` (monolithic lift) | `statement`                                     | **no**    |
| `program-elephant`          | `statement`, `elephant`                         | **no**    |
| `program-leaf`              | `statement`, `split`                            | **yes**   |
| `program-root`              | `statement`, `split`, `pinned-keys`             | **no**    |
| `program-leaf-elephant`     | `statement`, `split`, `elephant`                | **no**    |
| `program-root-elephant`     | `statement`, `split`, `elephant`, `pinned-keys` | **no**    |

One guest out of six, and not the one that links the most edited crates.
The mechanism is almost certainly `file!()`/`line!()` metadata compiled into panic paths: adding comment lines shifts the line numbers of the code below them, and a guest whose inlining happens to capture a shifted panic site gets a different ELF.
Which guest that is depends on inlining decisions, so **there is no reading of the source that predicts it.**

The mechanism above is an inference.
What is *measured* is the table: comment-only edits, one key moved, revert restores it, re-apply reproduces it.

## The rule

**Run `vkeys` after any edit to a crate a guest links, including a comment-only one.**
Do not reason about whether the change "could" affect codegen; the evidence is that a doc comment can, and that the affected guest is not the one you would guess.

Consequences when a leaf key moves, all of which happened here:

1. `pinned-keys` must be updated, which moves the **root** keys (they link it).
2. Every **cached leaf proof** becomes stale.
   The drivers catch this — `client.verify` against the current key refuses them — but the cost is real: `fixtures/leaves/n4` had to be re-proved (227.96 s) and every negative that consumes it re-run.
3. Anything pinned on-chain would have to be re-deployed.
   **It was not affected here** — the monolithic `lift` key, the one the Foundry suite pins, did not move — but that was luck rather than design, and a comment edit to `statement/` is exactly the change that could move it.

## Why the guard held

Nothing silently accepted a stale artefact.
`vkeys` failed loudly and printed the correct constant; the drivers now also assert pin freshness at startup, so a stale pin names itself immediately instead of surfacing minutes later as a root refusing a claimed key.
That is the difference between this and [[patch-table-silently-disengages]], where the failure was silent and cost a round of proofs.

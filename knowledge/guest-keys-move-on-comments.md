# A comment moves a guest's verifying key

Established 2026-08-29 by experiment, after an adversarial review prompted a routine re-check.

## What happened

Three crates that the zkVM guests link — `crates/statement/`, `crates/split/`, `crates/elephant/` — were edited to fix stale and missing security documentation.
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
   **It was not affected here** — the monolithic `lift` key, the one the Foundry suite pins, did not move — but that was luck rather than design, and a comment edit to `crates/statement/` is exactly the change that could move it.

## Why the guard held

Nothing silently accepted a stale artefact.
`vkeys` failed loudly and printed the correct constant; the drivers now also assert pin freshness at startup, so a stale pin names itself immediately instead of surfacing minutes later as a root refusing a claimed key.
That is the difference between this and [[patch-table-silently-disengages]], where the failure was silent and cost a round of proofs.

## Package names and paths move them too

Measured 2026-09-09, each independently, on the monolithic lift guest:

| change                                                             | lift verifying key |
| ------------------------------------------------------------------ | ------------------ |
| baseline                                                           | `0x00f1810f…`      |
| package renamed `sp1-lifter-program` → `pofseed-sp1-lift-program`  | `0x00b1555b…`      |
| folder moved `program/` → `programs/program/`, package name intact | `0x00c556ba…`      |

Both are sufficient on their own, and they produce different keys, so neither subsumes the other.
The guest ELF carries its own package name and its source paths, and the verifying key is a digest of that ELF.

Circuit fingerprints do **not** move under either change: they are computed from circuit structure, which a rename does not touch.
So a restructure invalidates the verifying keys and everything pinned to them, and leaves the fingerprints and the proof-format identifier alone.

The consequence is that reorganising the workspace is not a free operation.
It costs a re-pin of `pinned-keys`, a regeneration of the on-chain fixture with its verifying-key pin, and a re-proving of every cached leaf proof, because a cached leaf verifies only under the key it was proved with.

Trap: after a rename or a move, the Foundry suite goes **green** on the stale fixture, because the fixture and the pin are both stale and therefore consistent with each other.
Nothing in the suite can tell that the program it attests no longer exists.
`vkeys` is what catches this, and it must be run after any structural change, not only after an edit to a crate's contents.

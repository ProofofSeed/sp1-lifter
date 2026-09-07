# An instruction saving is not a time saving

Measured on this machine, 2026-09-07, across the move from the withdrawn zkboo 2.0.0 to 1.2.0,
on the hardened-child lift at full classical-128 soundness:

| | 2.0.0 | 1.2.0 | change |
| ------------------------- | -------------: | -------------: | ------: |
| guest instructions at 219 | 4.775013465e9 | 4.3861e9 | −8.14% |
| zkVM proving seconds | 10,296.11 | 9,927.49 | −3.58% |
| effective rate | 463,769 instr/s | 441,791 instr/s | −4.74% |

The release removed 8.1% of the guest's instructions and returned 3.6% of the wall clock. The
machine got **4.7% slower per instruction**, because the release changed the instruction *mix* as
well as the count — a Keccak precompile call and an ALU op both count as one instruction and do not
cost the same to prove.

So: an instruction count is a proxy for proving time only while the program's composition is held
still, which is precisely what a dependency release does not do. Quote instruction deltas as
instruction deltas.

**Confirmed on a second statement the same day.** The elephant leaf gave back 11.70% of its
instructions and 6.84% of its seconds, a degradation of 5.50% against the child's 4.74%. Same
direction, same order, not the same number — so the effect is real and general, and its size is
still statement-specific. A degradation factor may be borrowed across statements only into a band
wide enough to survive being wrong, never into a point estimate.

## The other half: derive nothing from a law you have just refitted

The same day, the elephant leaf was predicted at −14.5% by refitting the monolithic elephant law and
then reading the leaf off it. Two natural derivations were available:

- the leaf-to-marginal **gap** is constant → `2.1148e9 − 67,567,359` = −14.5%;
- the leaf-to-marginal **ratio** is constant → `0.97255 × 2.1148e9` = −14.1%.

Both were wrong, in the same direction. The measured leaf is 2,114,787,163 instructions, **−11.70%**,
and a 15-second `elephant-split 2 execute` was available the whole time.

The refit-never-scale rule already on this project's books says: when the world changes, re-measure
the quantity rather than scaling the old one. What today added is that the rule applies **one level
in**. Refitting a law and then deriving a second quantity from it is still scaling; the law is
measured, the derivation is not. A quantity is measured only where it is measured.

## The residue, which is a real fact and worth keeping

    leaf_new / marginal_new = 0.999994   (6 parts per million)
    leaf_old / marginal_old = 0.97255

`program-leaf-elephant` and `program-elephant` are different ELFs and the marginal term is fitted
from the monolithic one at n = 1, 2, 4, so the coincidence is not an artefact of shared arithmetic.
Stated as a difference: the monolithic per-response path shed 347.9M instructions and the leaf shed
280.3M; the 67.6M between them is exactly the old gap. **Something worth 67.6M instructions per
response left the monolithic program's per-response path without leaving the leaf's.**

🔎 Open, deliberately not chased — it is 3% of a leaf and out of scope for a version port. Recorded
with its number so it can be picked up on purpose rather than rediscovered by accident.

## Provenance, including the one weak input

`HASH_FULL_SECONDS = 10,296.11` is confirmed against `fixtures/lift-219-gpusafe.summary.log:18`.
`HASH_FULL_INSTRUCTIONS = 4.775013465e9` is an embedded constant whose ELF no longer exists, so it
cannot be re-derived; it is supported by being co-provenanced with the seconds figure in the same
commit, and by implying an old law (fixed 31.47e6, marginal 21.66e6) that sits sanely beside
today's (30.27e6, 19.89e6). Supporting, not proof. The −8.14% carries that caveat; the −3.58% does
not.

Both constants have since been deleted from `elephant_probe.rs`, which now refits the child law on
every run and asserts the fit's residual — see `embedded-measurements-go-stale-silently` in the
session memory for why a constant recording a measurement rather than a definition is a claim
nothing re-checks.

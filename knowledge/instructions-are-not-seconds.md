# Instruction counts do not predict proving time

Guest instruction counts and zkVM proving time diverge across a dependency release, because the release changes the instruction mix as well as the count.
A Keccak precompile call and an ALU operation each count as one instruction and cost different amounts to prove.

## Measurements

Hardened-child lift at 219 responses, this machine, 2026-09-07, across the move from the withdrawn zkboo 2.0.0 to 1.2.0:

|                           |           2.0.0 |           1.2.0 | change |
| ------------------------- | --------------: | --------------: | -----: |
| guest instructions at 219 |   4.775013465e9 |        4.3861e9 | −8.14% |
| zkVM proving seconds      |       10,296.11 |        9,927.49 | −3.58% |
| effective rate            | 463,769 instr/s | 441,791 instr/s | −4.74% |

Elephant leaf, same release, 2026-09-08: −11.70% instructions, −6.84% seconds, rate −5.50%.

Both statements slowed per instruction, by different amounts.
The effect is general; its magnitude is statement-specific.
A rate-degradation factor may be carried between statements only into an interval wide enough to tolerate being wrong, not into a point estimate.

## Consequence

Quote instruction deltas as instruction deltas.
An instruction count is a proxy for proving time only while the program's composition is held constant, which a dependency release does not do.

## Derivation from a refitted law is still scaling

The elephant leaf was predicted at −14.5% by refitting the monolithic instruction law and reading the leaf off it.
Two derivations were available:

- constant leaf-to-marginal gap: `2.1148e9 − 67,567,359` −14.5%;
- constant leaf-to-marginal ratio: `0.97255 × 2.1148e9` −14.1%.

Both are wrong in the same direction.
The measured leaf is 2,114,787,163 instructions, −11.70%, obtainable in fifteen seconds from `elephant-split 2 execute`.

The refit-never-scale rule applies one level further in than it was being read.
Refitting a law and then deriving a second quantity from it is still scaling: the law is measured, the derivation is not.

## Residue

 leaf_new / marginal_new = 0.999994 (6 parts per million) leaf_old / marginal_old = 0.97255

`program-leaf-elephant` and `program-elephant` are distinct ELFs and the marginal term is fitted from the monolithic program at n = 1, 2, 4, so the agreement is not an artefact of shared arithmetic.
As a difference: the monolithic per-response path shed 347.9M instructions and the leaf shed 280.3M, leaving 67.6M — the size of the old gap.

The cause is not established and may not be establishable.
Ablation of the leaf's non-replay work *increased* its instruction count by 44,915,899 (+2.12%), reproducibly: restoring the deleted lines returned the count to 2,114,787,163 exactly.
Deleting three lines perturbs code generation across a 2.1e9-instruction program by two thirds of the difference under investigation, so a 67.6M difference between two programs across a release carries no reliable signal.
The historical half is unanswerable in any case: it requires the withdrawn 2.0.0 build, whose history was force-pushed away and whose published versions are yanked.

## Provenance

`HASH_FULL_SECONDS = 10,296.11` is confirmed against `fixtures/lift-219-gpusafe.summary.log:18`.

`HASH_FULL_INSTRUCTIONS = 4.775013465e9` was an embedded constant whose ELF no longer exists and cannot be re-derived.
It is supported by co-provenance with the seconds figure in the same commit, and by implying an old law (fixed 31.47e6, marginal 21.66e6) consistent with the current one (30.27e6, 19.89e6).
The −8.14% carries that caveat; the −3.58% does not.

Both constants were deleted from `elephant_probe.rs`, which refits the child law on each run and asserts the fit residual.

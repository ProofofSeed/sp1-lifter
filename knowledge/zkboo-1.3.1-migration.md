# Moving to zkboo 1.3.1

`zkboo` alone was released as **1.3.1** on 2026-11-11 (`e7f59a8`, tag `v1.3.1`, unpublished), and this workspace moved to it on 2026-11-12.
This file records what the release changed, what the move invalidated here and what it cost.
The upstream account is `zkboo`'s `CHANGELOG.md`; the previous round is `zkboo-1.3.0-migration.md`.

The other nine crates did not move: seven stay at 1.3.0, and `zkboo-keccak` and `zkboo-ripemd160` at 1.2.0.
The build guard names the version per crate, and only the `zkboo` row of its table and the `zkboo` requirement of each manifest changed.

## What changed upstream

**A short AND-message or input-share vector fails replay** (`zkboo`).
Under 1.3.0 the replay source zero-filled a vector past its end without advancing its cursor, and the consumption check compared the cursor with the vector's length, so a short vector passed whenever the cursor happened to land on its length and a long one never passed.
The cursor now counts the words demanded, and an absent party-2 share is compared against an empty shape, so a short vector fails the existing check as a long one did.
This was encoding malleability only: the prover chooses those words freely, so every view a short encoding replays is also replayed by some explicit full-length encoding, and no consumer of this repository hashes or compares proof bytes.
The audit of 2026-10-09 (`untrusted-input.md`) found it and filed the specification zkboo implemented.

Nothing else changed: no circuit, no hashing role, no wire format.

## What it invalidates here

- **All six guest verifying keys**, re-derived, and the two leaf pins re-anchored.
  Every guest links `zkboo`, and a crate's version alone moves a key.
- **The on-chain fixture and its `EXPECTED_VKEY` pin**, regenerated from a full-soundness run and re-anchored in the same change.
- **The cached leaf proofs**, which no longer verified under the new leaf keys.
  The caches had already been deleted on 2026-11-06 with every other build output; `fixtures/leaves/n4` was re-proved on 2026-11-12, and `n64`, `n219` and the elephant `n2` were not, since no test reads them and the elephant pair alone is about six hours of proving.

What did **not** move:

- **Both circuit fingerprints**: the hardened child's `0x72936f44…` and the Ethereum address's `0xa64f0f8a…`.
  The release changes the verifier's reading of a response, not a gate, so this is the expected control.
- **`PROOF_FORMAT_ID`** `0x4be5d4e6…`, `LIFT_JOURNAL_VERSION` and `LIFT_FRAME_VERSION`.
- **`EXPECTED_FINGERPRINT` and `EXPECTED_PROOF_FORMAT_ID`** in the Foundry suite, which the regenerated fixture passes unchanged.

## Changes made here

- The `zkboo` requirement is `=1.3.1` in the eleven manifests that name it, and the build guard's table says the same; `cargo update -p zkboo` moved the one lockfile line, which carries no `source` key, so the patch table engages.
- The two leaf keys in `crates/pinned-keys` and `EXPECTED_VKEY` in `verifier/test/LiftGroth16.t.sol` were re-anchored.
- No source line of any guest or host crate changed: the lifter never relied on the zero-fill, and its own frame decoders already refused a short frame.
- `untrusted-input.md` records the upstream fix against its finding.

## What it cost

Measured on this machine, full GPU path, on the clean checkouts the provenance line names (`zkboo=e7f59a8559af`, the other nine as on 1.3.0), with `RAYON_NUM_THREADS=12 GOMAXPROCS=12`, one run at a time.

|                                    |         1.3.0 |         1.3.1 | change |
| ---------------------------------- | ------------: | ------------: | -----: |
| lift at 219, guest instructions    | 4,415,364,477 | 4,415,367,654 | +3,177 |
| `lift 219`, proving                |    3,662.54 s |    3,820.30 s | +4.31% |
| `lift 219`, peak RSS               |      50.95 GB |      50.15 GB | −1.57% |
| `lift 219`, `(user+sys)/real`      |          8.29 |          8.20 |      — |
| monolithic compressed over 4       |      106.75 s |      115.47 s | +8.17% |
| hardened-child leaf, mean of four  |       22.99 s |       24.26 s | +5.52% |
| hardened-child leaf, slowest       |       23.20 s |       25.70 s |      — |
| root over four leaves              |       46.70 s |       49.73 s | +6.49% |
| on-chain, pairing check + gate     |       280,590 |       280,590 |      0 |
| proof on the wire                  |         356 B |         356 B |      0 |

Logs: `fixtures/lift-219-2026-11-12.summary.log`, `s1-acceptance-2026-11-12`, `split-probe-2026-11-12`, `forge-gas-2026-11-12` and `forge-negatives-2026-11-12` (each `.summary.log`), untracked in `fixtures/`.
The 1.3.0 column is the 2026-09-21 lift and the 2026-09-19 split of `zkboo-1.3.0-migration.md`.

- The guest did not change in any way that costs: 3,177 instructions over 4.4 billion is the replay source's cursor arithmetic, and the executor read the same fingerprint and the same journal.
- Every proving time nevertheless lands 4% to 8% above September, one run on each side, with the instruction count ruling the guest out.
  Neither run was taken on a machine checked quiet: another session's one-core script was running when the lift launched, and the split launched a minute after the acceptance run finished, with the one-minute load average still at 3.
  The lift's 8.20 cores delivered sits close to the 8.29 of the quiet September run, so the lift's +4.31% is not explained by contention either, and this record attributes it to nothing.
  Whoever needs the difference resolved re-takes the September arrangement on a quiet machine; nothing here rescales a figure.
- The root over four lands 7.1% above `root seconds = 42.40 + 1.0073 × n`, where the 1.3.0 point landed 0.58% above it; the law is not refitted from one point.
- The on-chain cost is read on the regenerated fixture by a plain `forge test -vv`: `SP1VerifierGroth16.verifyProof` 225,975 and `LiftJournalGate.check` 54,615, together 280,590, `forge test` 18 of 18, unchanged to the unit as the unchanged journal layout and proof shape predicted.
  🪤 The same suite run with `--gas-report` reads the pairing check at 228,475, 2,500 more, on either fixture; the flag perturbs the inline `gasleft()` measurement, and the September figure was taken without it.
- Green afterwards: `vkeys`; `s1-acceptance both`, R1 to R8 each refused by its named refuser, the control accepting, the full-scale positive passing the consumer gate; the nine-case cross-leaf gate matrix at 24 leaves; `split-probe 4 both`; the three forged-leaf negatives `forge-vk`, `forge-count` and `forge-order`, refusing as before with the refusing layer of the last two as undetermined as before; `elephant-split 2 gate`; the frame fuzz test with the tally of 2026-10-09 (132 decoder refusals, 122 verifier refusals, one version refusal, one honest acceptance past the frame, no panic).

## The guest verifying keys on 1.3.1

From `vkeys` on the clean checkouts (`zkboo=e7f59a8559af`, `zkboo-bip32=ad5b92d5ea20`):

| guest                       | verifying key                                                        |
| --------------------------- | -------------------------------------------------------------------- |
| lift (monolithic)           | `0x000105a97118f004455341061a0bcb97815fbe560ac25bc9198148e3c47caa85` |
| elephant (monolithic probe) | `0x00ee789d5f13b4e171ad881acdb82bd422261e8907e3c5c7b3869f0701959329` |
| leaf                        | `0x008cce7ed8c09359e6bdb26648f556b57ba96ed5a2c28b7da888244ff357c3d5` |
| root                        | `0x00d8471e1caf3c5600ae88f5046647ee1f4849f60852b1aae4711734e0ab42c6` |
| leaf-elephant               | `0x004b7ef6b0c6b3b8436d7185ff5940d647c4c9e05b3b142846e0ec8b7989cd08` |
| root-elephant               | `0x0087ff693c86a21a79664e1ef63673297abc3d0690bb7c6f5e93abee3d86eae6` |

The lift key is `EXPECTED_VKEY` in `verifier/test/LiftGroth16.t.sol`, and the two leaf keys are pinned in `crates/pinned-keys`; the other three are recorded here and nowhere else.
The 1.3.0 keys they replace are tabulated in `zkboo-1.3.0-migration.md`.

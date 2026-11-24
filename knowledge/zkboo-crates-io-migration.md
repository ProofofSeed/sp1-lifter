# Resolving zkboo from crates.io

The zkboo releases this workspace is built against were published to crates.io on 2026-11-24, and the workspace moved to them that day.
This file records what the move invalidated here and what the lift measured afterwards.
The previous round is `zkboo-1.3.1-migration.md`.

## What changed

- The ten `zkboo` entries of `[patch.crates-io]` are gone; `tiny-keccak` and the six `sp1-metal` entries stay.
  Every requirement is unchanged and exact (`=1.3.1` for `zkboo`, `=1.2.0` for `zkboo-keccak` and `zkboo-ripemd160`, `=1.3.0` for the other seven).
- The lockfile gained a `source` and a `checksum` line per zkboo crate and changed nothing else.
- `host/build.rs` is back in its registry form: it refuses any zkboo crate that is not at its required version from crates.io, and records each crate's version and checksum prefix into the binary in place of the working trees' revisions.
- The published crates are the trees the workspace was built from: each repository's `main` was its tagged release, clean, when it was published.

## What it invalidates here

- **All six guest verifying keys.**
  No source line changed; a crate's source, like its version or path, is enough to move a key.
  The two leaf pins in `crates/pinned-keys` were re-anchored, and `vkeys` passes.
- **The on-chain fixture and its `EXPECTED_VKEY` pin**, regenerated from the run below and re-anchored to the monolithic lift key `0x00bf1adc…`.

What did **not** move: both circuit fingerprints (`0x72936f44…` and `0xa64f0f8a…`), the on-chain cost, and the proof on the wire.

## The guest verifying keys

From `vkeys` against the crates.io sources:

| guest                       | verifying key                                                        |
| --------------------------- | -------------------------------------------------------------------- |
| lift (monolithic)           | `0x00bf1adc7ba480180f27a025f5f2692c029f6ec1eeecfdc7fc0d5c726ba5be5e` |
| elephant (monolithic probe) | `0x0050b3565ac5f42e74ce76e822d7b0b08e645c1c10419dc4a5bb75b3f9892ffb` |
| leaf                        | `0x0000b0a3c1ef65cbb7b27078d69de7d94dafa8ffad9bb5f66ae8a4a0b61c2fb5` |
| root                        | `0x008e83f27285f3f7c0e70f42757603d209a5f6e2269e3ac8853f0433d28c7643` |
| leaf-elephant               | `0x00dabed913205497bf21d494167679057c194f968bb0d9f03971f3603edc1e1f` |
| root-elephant               | `0x0003dc77739cc5f60ad9e2672d61ef881cc3ed05c88f8dbd93e57300a6ddbb00` |

## What the lift measured

One `lift 219`, full GPU path, `RAYON_NUM_THREADS=12 GOMAXPROCS=12 RUST_LOG=error`, under `/usr/bin/time -l`, prebuilt binary at `9e30c6f`, launched at 10:28 on 2026-11-24 with a one-minute load average of 0.97 and nothing of this project running.
During the run the operating session ran only git, `curl`, the GitHub command-line client and small file edits; no build or other CPU-bound work ran.

|                                 | 2026-09-21 (1.3.0) | 2026-11-12 (1.3.1) | 2026-11-24 (crates.io) |
| ------------------------------- | -----------------: | -----------------: | ---------------------: |
| proving                         |         3,662.54 s |         3,820.30 s |             3,809.72 s |
| peak RSS                        |           50.95 GB |           50.15 GB |               50.28 GB |
| `(user+sys)/real`               |               8.29 |               8.20 |                   8.24 |
| on-chain, pairing check + gate  |            280,590 |            280,590 |                280,590 |
| proof on the wire               |              356 B |              356 B |                  356 B |

- This run was launched on a quiet machine and reproduces the 2026-11-12 figure to 0.28%, against 4.02% above September.
  The difference from September is therefore not the noise the 2026-11-12 record could not rule out; it is attributed to nothing here, the guest's instruction count having moved by 3,177 of 4.4 billion between the two.
  Nothing rescales the September figure.
- On-chain cost from a plain `forge test -vv`: `SP1VerifierGroth16.verifyProof` 225,975 and `LiftJournalGate.check` 54,615, 18 of 18 tests passing.

Logs: `fixtures/lift-219-2026-11-24.launch.log`, `fixtures/lift-219-2026-11-24.summary.log` and `fixtures/forge-gas-2026-11-24.summary.log`, untracked.

## Not re-run

`s1-acceptance`, `split-probe`, the forged-leaf negatives, `elephant-split` and the frame fuzz test were not re-run in this round; they last passed on 2026-11-12 against the same code built from the local trees.

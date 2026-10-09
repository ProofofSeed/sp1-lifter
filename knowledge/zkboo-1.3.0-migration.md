# Moving to zkboo 1.3.0

The zkboo crates were released as **1.3.0** on 2026-09-19, having passed through 1.2.1 on 2026-09-09.
This file records what the two releases cost here, what they invalidate, and what they buy.
The upstream account is each crate's `CHANGELOG.md`; the previous round is `zkboo-1.2.0-migration.md`.

Eight of the ten crates this workspace links are at 1.3.0.
`zkboo-keccak` and `zkboo-ripemd160` are still at 1.2.0, and the guard names a required version per crate rather than one for all ten.

## What changed upstream

**A verifier must name the repetition count it demands** (`zkboo`).
`verify`, `par_verify` and `Verifier::new` take a `Repetitions`, and a proof of any other length is rejected.
Verification checked each response it was given and nothing about how many there were, so a proof truncated to one response — or to none, which verified against any output — passed.

**The BIP-32 derivations assert what BIP-32 states** (`zkboo-bip32`).
`master_key` asserts a non-zero `IL` below the group order; `hardened_child_key` and `normal_child_key` assert `IL` below the group order and a non-zero child key.
Both of the first two take an assertion accumulator they did not take before.
A derivation reaches an invalid key with probability below `2^-127`, so no seed exercises these assertions; what they buy is that a witness which does cannot be lifted into an accepting proof.

**The repetition formula is published** (`zkboo::Repetitions::for_pq_security_bits`).
It lived in `zkboo-harness`, which is `publish = false`, so no consumer could reach it.

## What it invalidates here

- **Every proof of the hardened child, and its fingerprint**: `0xbd121f6f…` → `0x72936f44…`.
  The derivation's assertions are part of the circuit's structure.
- **All six guest verifying keys**, re-derived and the two leaf pins re-anchored.
  A crate's version alone moves a key even when its sources do not change, which 1.2.1 demonstrated on this workspace: its four crates were byte-identical to the trees the previous keys came from.
- **The on-chain fixture and its `EXPECTED_VKEY` and `EXPECTED_FINGERPRINT` pins**, regenerated and re-anchored in the same change.
- **The cached leaf proofs** under `fixtures/leaves/` and `fixtures/elephant-leaves/`, re-proved on 2026-09-19 (`n4`) and 2026-09-21 (`n64`, `n219` and the elephant `n2`).

What did **not** move:

- **The Ethereum-address fingerprint** `0xa64f0f8a…`, which is what makes the hardened child's move attributable to the BIP-32 assertions rather than to the release at large.
- **`PROOF_FORMAT_ID`** `0x4be5d4e6…`: the wire format is untouched, so the journal gate's format clause is unchanged.
- **The journal and frame layouts**, so `LIFT_JOURNAL_VERSION` and `LIFT_FRAME_VERSION` stay put.

## Changes made here

- `HardenedChildCircuit::exec` passes the scope's accumulator into `hardened_child_key`.
  The flag it emits was previously the constant 1 for this statement; it now carries the BIP-32 validity conditions.
- Every `verify` call site names a count.
  The two guests require the count the proof carries, which makes zkboo's new check vacuous *there* and says so in a comment: a lifting program cannot know what soundness level a relying party wants.
  The count is committed in the journal, and `check_journal` and the on-chain gate refuse anything but the count they demand — so it is required exactly once, by the party whose security rests on it.
- The lift guest refuses an empty proof with its own diagnostic tag rather than reaching `Repetitions::exactly`, which panics on a count of zero.
- `n_eps` delegates to `Repetitions::for_pq_security_bits` instead of carrying a copy of the arithmetic.
- The acceptance matrix's R5 changed refuser.
  An empty proof was accepted by the ZKBoo verifier by design, and the count gate was the only thing between that and an accepted lift; the program now refuses it outright.
  The gate still refuses it, which R4 exercises on a proof that does verify, so R5 now measures that the refusal happens earlier rather than that it happens at all.

## What it cost

Measured on this machine, full GPU path, on the clean 1.3.0 checkouts (the `vkeys` and `lift` provenance lines name the revisions).

|                                 |         1.2.0 |         1.3.0 |
| ------------------------------- | ------------: | ------------: |
| lift at 219, guest instructions | 4.386e9 (law) | 4,415,364,477 |
| hardened-child fingerprint      | `0xbd121f6f…` | `0x72936f44…` |
| Ethereum-address fingerprint    | `0xa64f0f8a…` | `0xa64f0f8a…` |
| proof format id                 | `0x4be5d4e6…` | `0x4be5d4e6…` |

The instruction count is `+0.67%` against what the 1.2.0 law gives at 219 (`3.0274e7 + 1.9889e7 × n`), which is the assertions.
That is a comparison of a measurement with a fitted law, not of two measurements.

**The split path at n = 4**, measured on a quiet machine with the documented `RAYON_NUM_THREADS=12`:

|                                |   1.2.0 |   1.3.0 |
| ------------------------------ | ------: | ------: |
| leaf, mean of four             | 22.81 s | 22.99 s |
| leaf, slowest of four          |       — | 23.20 s |
| root over four leaves          | 47.54 s | 46.70 s |
| root over four leaves, the law | 46.43 s |       — |

The 1.2.0 leaves are the four-leaf split re-proved on 2026-09-09 (`fixtures/relaunch-2026-09-07.summary.log`), and its root is `fixtures/split-4-root-2026-09-09.summary.log`.
The 1.3.0 leaves are 23.20, 22.78, 23.17 and 22.81 s (`fixtures/split-probe-2026-09-19.summary.log`): the mean is +0.8% on 1.2.0 and the slowest leaf +1.7%.
An earlier version of this table set the slowest 1.3.0 leaf against the 23.13 s mean of eight single leaves of 2026-09-08, which is neither the same statistic nor the same arrangement.
The root lands 0.58% above `root seconds = 42.40 + 1.0073 × n` and 1.8% below the 1.2.0 measurement at this count.
Two points at one repetition count do not refit that law, and nothing here claims they do.

**The full-soundness lift and the split path at scale**, re-taken on 2026-09-21 and 2026-09-22 with `RAYON_NUM_THREADS=12 GOMAXPROCS=12`, one run at a time from prebuilt binaries, the machine checked quiet at launch:

|                                    |                 1.2.0 |      1.3.0 | change |
| ---------------------------------- | --------------------: | ---------: | -----: |
| `lift 219`, proving                |            3,613.03 s | 3,662.54 s | +1.37% |
| `lift 219`, peak RSS               |              48.39 GB |   50.95 GB | +5.29% |
| `lift 219`, `(user+sys)/real`      |                  8.36 |       8.29 |      — |
| hardened-child leaf, mean over 64  |                     — |    23.34 s |      — |
| hardened-child leaf, mean over 219 |                     — |    23.20 s |      — |
| root over 64 leaves                |              105.32 s |   107.52 s | +2.09% |
| root over 219 leaves               |              263.43 s |   247.80 s | −5.93% |
| elephant leaf, mean of two         | 1,340.08 s (re-prov.) | 1,377.20 s | +2.77% |
| elephant root over two leaves      |            5,911.38 s | 6,012.93 s | +1.72% |
| elephant `leaves + root`, peak RSS |                     — |   54.68 GB |      — |

Logs: `fixtures/lift-219-2026-09-21.summary.log`, `split-64-2026-09-21`, `split-219-2026-09-21` and `elephant-split-2-both-2026-09-21` (each `.summary.log`).
The 1.2.0 column is the 2026-09-08/09 full-path set in `performance-table.md`; the elephant leaf compares with the re-proved leaves of 2026-09-09 (`fixtures/relaunch-2026-09-07.summary.log`), and is +3.25% on the 1,333.82 s of 2026-09-08.

- The lift costs +1.37% of seconds for +0.67% of instructions, one run on each side.
- The **elephant statement did not change** (its fingerprint is unmoved), so its +2.8% leaf and +1.7% root are not the assertions. The two elephant leaves of this run differ from each other by 2.5% (1,360.04 and 1,394.35 s), so a single pair does not attribute the difference to the release.
- The root at 219 lands 5.8% **below** `root seconds = 42.40 + 1.0073 × n`, where the counts 4 and 64 land 0.58% and 0.61% above it. A refit over the three 1.3.0 points would read `45.10 + 0.929 × n` with a worst residual of 4.3%, which is not a law this record adopts from one run per count; the 1.2.0 law stands, with this point marked as its largest disagreement.
- Carried as before, the elephant root at 219 is about 6,013 + 217 × 1.007 ≈ 6,232 s (1 h 44 m) on 1.3.0.

🔴 **The fixture run of 2026-09-19 is still not a measurement**, but for a narrower reason than first recorded: it left `RAYON_NUM_THREADS` unset.
Its `(user+sys)/real = 9.05` was first read as contention, which was a misapplication: 11.88 is the signature of a quiet run on the CPU-bound safe path, while on the full GPU path a quiet lift reads about 8.3, and the extra threads account for the higher figure.
Whether that machine was also contended is unknown.

**The on-chain cost is measured on the 1.3.0 fixture**, regenerated by the 2026-09-21 run: `SP1VerifierGroth16.verifyProof` 225,975 and `LiftJournalGate.check` 54,615, together 280,590, `forge test` 18/18 (`fixtures/forge-gas-2026-09-21.summary.log`).
Unchanged to the unit, as the unchanged journal layout predicted.

## The guest verifying keys on 1.3.0

From `vkeys` on the clean checkouts named in its provenance line (`zkboo=b37bfeef9875`, `zkboo-bip32=ad5b92d5ea20`):

| guest                       | verifying key                                                        |
| --------------------------- | -------------------------------------------------------------------- |
| lift (monolithic)           | `0x005b7af58b3e62ccdf711b5c43732f9498eeea399c139fe897ed96c028b1f957` |
| elephant (monolithic probe) | `0x00c2a78670bcfa1c1d2ec3abd23302e08221e86baa6853a8c5157ef0fedad0c8` |
| leaf                        | `0x0062f3580f157036c99db66a9c476f937ab3e57e58702c73adf00abe701d7e1f` |
| root                        | `0x0002c1e4aa12630db4dbf314944b0b8661f24e848c20b31dd222a57d058393ab` |
| leaf-elephant               | `0x0005ef71e534e5462063bc1ba0379c9ebcf5f1cbea1accb178f71e39d79396af` |
| root-elephant               | `0x002c3e677b158f77278f380ae7623b2624ecbacd0cc5d2ccc11234805dd0ace9` |

The lift key is `EXPECTED_VKEY` in `verifier/test/LiftGroth16.t.sol`, and the two leaf keys are pinned in `crates/pinned-keys`; the other three are recorded here and nowhere else.
The two root keys are those of 2026-10-09, when both roots began refusing trailing bytes after a leaf journal (`knowledge/untrusted-input.md`); until then they were `0x00f737b4…c49d95` and `0x009ae014…75ed58`, and the other four keys did not move.

The leaf caches under `fixtures/leaves/n64`, `n219` and `fixtures/elephant-leaves/n2` were re-proved by the runs above and verify under these keys.

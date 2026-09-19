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
- **The cached leaf proofs** under `fixtures/leaves/` and `fixtures/elephant-leaves/`, which no longer verify against the current keys.

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

**The split path costs what it did**, measured at n = 4 on a quiet machine with the documented `RAYON_NUM_THREADS=12`:

|                       |          1.2.0 |   1.3.0 |
| --------------------- | -------------: | ------: |
| leaf                  |       23.13 s  | 23.20 s |
| root over four leaves | 46.43 s (law)  | 46.70 s |

The root figure is against `root seconds = 42.40 + 1.0073 × n` rather than against a stored measurement at this count, and lands 0.58% above it.
Two points at one repetition count do not refit that law, and nothing here claims they do.

🔴 **The full-soundness lift was NOT re-timed.**
The run that regenerated the fixture left `RAYON_NUM_THREADS` unset and read `(user+sys)/real = 9.05`, so its 3,862.60 s is evidence that the artefact exists and nothing about what the port costs.
Re-take it on a quiet machine, with the threads set, before any figure for 1.3.0 at 219 is quoted.

# The delta on top of zkboo 1.2.0

A second breaking change landed upstream on 2026-09-06, two days after the 1.2.0 port and before anything here had been built.
It is in the local trees only — nothing pushed, nothing tagged, the versions still say `1.2.0` — so it arrives the moment this workspace rebuilds against the local zkboo trees.
Upstream record: the zkboo repository's `MIGRATION-NEXT.md` and `DESIGN-ASSERTIONS-HINTS.md`, which are not yet public.

Read this after `zkboo-1.2.0-migration.md`; it amends that file's central trap.

## 1.  The proof-format identity, which fixes the trap the 1.2.0 port had to record

`zkboo::PROOF_FORMAT_VERSION` is deleted.
`zkboo::PROOF_FORMAT_ID` is a `[u8; 32]` that *is* the digest of a canonical reference proof.

This is the direct repair of the defect recorded in `zkboo-1.2.0-migration.md`: the counter said `3` on both sides of an incompatible format change, so the journal gate's format clause accepted a proof from a format it existed to refuse.
The counter and the bytes it named were two records of one fact with nothing keeping them in agreement.
The identity now *is* the bytes, so it cannot fail to move when they move; the price is that it also moves for things that are not format changes, since the canonical proof is made by a circuit.
Over-triggering costs a regeneration, which is the right side to err on.

Here that turned into:

- `REQUIRED_PROOF_FORMAT_VERSION: u32 = 3` became `REQUIRED_PROOF_FORMAT_ID: [u8; 32]`, still a pinned literal rather than a read of the linked `zkboo`, for the reason it always was: an expectation taken from what a consumer links tracks that consumer instead of constraining it.
- `LiftJournal.proof_format_version` and `ElephantJournal.proof_format_version` became `proof_format_id`, and the guests commit `zkboo::PROOF_FORMAT_ID`.
- The host's elephant constant is now `REQUIRED_ELEPHANT_PROOF_FORMAT_ID`, defined **as** the statement crate's constant rather than as a second literal of the same digest.
  Two copies of one fact is precisely the arrangement that failed; the host already links the statement crate, so naming it moves no guest key.
- The Solidity gate reads it as thirty-two raw bytes.
  It is the one field in the journal other than the fingerprint that is **not** a varint, and it replaced a field that was — so a decoder ported clause-by-clause reads one byte where it must read thirty-two, and then fails on the trailing-byte check rather than on the field it got wrong.
  `LiftJournalGate.t.sol` pins the digest as a third independent copy, on purpose: a relying contract is its own trust domain.
- `LiftJournalGate.t.sol` gained `test_rejects_a_different_proof_format`.
  The clause had no negative of its own before, which is uncomfortable given it is the clause this change is about.

**Journal layout versions moved with the shape:** `LIFT_JOURNAL_VERSION` 2 → 3, `ELEPHANT_JOURNAL_VERSION` 1 → 2, `ROOT_JOURNAL_VERSION` 1 → 2 (it covers the lift journal it nests).
`LEAF_JOURNAL_VERSION` stays at 1 — a leaf journal carries no format field.
The input frame versions do not move either: no frame carries one.

## 2. Assertions come only from a scope

`Assertions::new`, `finish`, `output` and `Default` are gone.
The accumulator is reachable only from `Assertions::scope(frontend, |assertions| { … })`, which emits the flag when the body returns.
So everything a circuit outputs has to move inside the body, or the flag stops being the last output word.

What it fixes: a circuit could accumulate assertions and never emit the flag, and both sides, building from the same definition, would agree on an output carrying no flag and verify a proof constraining nothing.
Omission is now unrepresentable.

What it does not fix, and zkboo says so: a circuit that never asserts at all.
**zkboo also declined to impose a universal trailing word** — a circuit that opens no scope emits none.
Both circuits here open one.
On `EthereumAddressCircuit` the flag is the conjunction of everything the comb asserts about its slopes and is the whole of what makes the proof mean anything.
On `HardenedChildCircuit` nothing is asserted, so it is the constant `1`, costing one output word and no AND gate; the scope is opened anyway so that one journal shape, one Rust gate and one Solidity decoder serve every statement.

## 3. Comb advice is gone: pass the scalar, not the slopes

`AffineCombAdvice`, `PublicKeyAdvice`, `TaprootAdvice` and the `public_key_advice*` functions are deleted.
Every secp256k1 derivation now takes `Option<CompositeWord<u64, 4>>` — the scalar its comb multiplies by — and mirrors it internally, one window at a time.
`Some` when proving or executing, `None` when replaying a view, fingerprinting or profiling.

`EthereumAddressCircuit` therefore holds a scalar where it held a slope table: `with_witness` stores `Some(be_words(&private_key))` and `public_only` stores `None`.
The comb allocates its slope inputs either way, so the circuit's shape does not depend on which.

Upstream's reasons are worth keeping, because two of them are failure modes this project cares about independently:

- **Erasure.**
  The slopes determine the scalar — each fixes which entry of a public table its window used, and the recoding is a bijection — so the object holding them was the private key in another base, alive for a whole proof, with no accessor and no `Drop`.
  A caller scrubbing its 32-byte key left 1,664 equivalent bytes behind it.
  The mirror is now a ~128-byte local, erased when the comb returns.
- **Yielding.**
  The slope arithmetic ran in a constructor, before any backend existed: 52 modular inversions in one call with no way for a caller to service an operating system in the middle.
  That is the shape of the non-yielding pass that has destroyed a Ledger Flex on this project.
  It now runs between two gates, under whatever `BackendHook` the caller already has.
- **Table sources.**
  The advice pass built its own tables, routing a caller with tables in flash back through native point arithmetic.

Cost, per upstream's measurement: the mirror runs once per circuit execution rather than once per proof, and interleaved runs put the difference at **+0.3% to +0.5%** of prover wall clock.

## Effect on this repository

**The ZKBoo proof bytes are reported unchanged.**
The port of the device-side stack measured eight statement fingerprints byte-identical across the change, and four raw proof-byte rows — both `public_key` rows among them — identical in length *and* digest.
Upstream states the same: gate for gate, in the same order, with the same values.
So instruction counts and cost laws should carry.

**That is their measurement of their circuits, not a measurement of ours.**
The two statements in this repository are not among the eight, and nothing here has been executed since.
Treat "fingerprints did not move" as a well-founded expectation to check with `vkeys` and the first fingerprint print, not as an established fact about this tree.

**The guest verifying keys move regardless**, on any dependency edit at all — that is the `guest-keys-move-on-comments.md` finding.
So the outstanding work is unchanged from `zkboo-1.2.0-migration.md`: build, `vkeys`, re-pin, re-prove the caches, re-run acceptance, regenerate the on-chain fixture.
The fixture is now stale for a second reason as well as the first: its journal declares layout version 2 and carries the format field as a varint.

## Addendum, 2026-09-07: four upstream changes, none reaching this tree

Checked against the trees rather than inferred, after they were reported elsewhere.

1. **The affine conversion is asserted rather than computed.**
   The in-circuit modular inverse lost its hint when the hint gate went, adding ~90,000 AND messages to every statement that converts a point out of projective coordinates.
   Restored by advising the affine coordinates and asserting they scale back — `x·Z² == X`, `y·Z³ == Y`, `Z ≠ 0`.
   It reaches **Taproot and Ed25519 only**, and the advised forms now reject the point at infinity by making the assertion false rather than emitting a sentinel.
   `taproot_tweak_scalar` is replaced by `TaprootWitness::compute`, and Ed25519 derivations take an accumulator for the first time.
2. **`Montgomery<W, N>`**, separating a field element's stored form from its canonical residue; `MontgomeryWord::value` renamed `canonical`.
   Worth knowing even though it misses us: under a pseudo-Mersenne modulus, which is what secp256k1 uses, the two forms are *the same word*, so passing one for the other is invisible on secp256k1 and makes every assertion silently false on Ed25519 — no error, no panic, the flag simply comes out `0`.
   A test passed only because it was written for the pseudo-Mersenne field.
3. `MontgomeryFrontendIO::montgomery_output_inner`, additive.
4. `ExecOptions` is `Copy` whatever the hook, additive.

**Why none of them reach us, verified.**
This repository has exactly two `Circuit` impls, `EthereumAddressCircuit` and `HardenedChildCircuit`.
The Ethereum address never leaves affine coordinates — the comb emits `(x, y)` directly — so change 1 misses it, and a grep of every tracked `.rs` file finds no `.canonical()`/`.value()`, no `montgomery_output`, no `MontgomeryWord`, no `taproot_tweak_scalar` and no Ed25519 or Solana call site, so 2 and 3 miss it too.
`cargo check` on the three `no_std` crates against the current trees is clean.

**The format pin is still correct**, compared byte for byte: `REQUIRED_PROOF_FORMAT_ID` equals `zkboo::PROOF_FORMAT_ID` today, and `git log` on `response.rs` and `test_proof_format.rs` shows nothing has touched either since the commit this pin was taken from.
Nothing to re-anchor.

**"Current on code" is not "keys unchanged".**
`zkboo-ecc`, `zkboo-modular` and `zkboo-bip32` all moved, and a guest verifying key moves on any dependency edit at all — a doc comment has been enough.
The six guest keys were already stale; they are stale again for a further reason.
That changes nothing about the outstanding work, but it is not a conclusion the code check licenses.

**Taproot, Solana and `pubkey_hash160` are not statements of this repository.**
They appear here only as a line in a superseded plan.
Cost rows for them live in a separate research repository that is not yet public.

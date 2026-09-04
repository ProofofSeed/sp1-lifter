# Moving to zkboo 1.2.0

The zkboo ecosystem was re-architected and re-released as **1.2.0** on 2026-09-04, replacing the
2.0.0 line this repository had been built against. 2.0.0 is withdrawn: its history is gone from
every remote, all thirty-two previously published crates.io versions are yanked, and 1.2.0 is not
published at all. The upstream record is `<zkboo>/HANDOVER.md` and `<zkboo>/MIGRATION.md`;
this file records what it cost here and which of this repository's numbers it invalidates.

## Everything measured before this date is superseded

Not "probably drifted" — arithmetically invalid, for three independent reasons, any one of which
is enough on its own.

1. **Every proof.** 1.2.0 domain-separates three hashing roles that were previously separated once,
   partially, or not at all: the Fiat-Shamir challenge transcript (`ZKBOO-CHALLENGE`), the party
   view commitment (`ZKBOO-VIEW-COMMITMENT`) and the hash-based PRG (`ZKBOO-PRG`), each absorbing
   its tag and an 8-byte little-endian length before its payload. Every commitment and therefore
   every challenge differs. No proof made before this verifies after it.
2. **Every circuit in the suite.** The `Backend` trait is back to its 1.0.0 surface. Prover advice
   is no longer a gate (`Backend::hint` is gone) but an ordinary secret input the prover computes
   on the host, and assertions are no longer accumulated by the backend but by circuit-side sugar
   (`zkboo::circuit::Assertions`) that a circuit must explicitly emit. Both statements this
   repository lifts changed shape as a result, so both fingerprints moved.
3. **Every verifying key.** Both of the above reach the guests, and so does the vendored hasher
   below. All six keys move; the two pinned in `pinned-keys` are marked stale in place.

So the tables in `performance-table.md`, `elephant-cost.md`, `iteration.md` and
`split-and-streaming.md` describe a stack that no longer exists. They are kept, banner-marked,
because the *method* and the *shape* of those results carry over and the cost of re-taking them
does not. Nothing in them may be quoted as current, and no figure in them may be rescaled by a
factor observed elsewhere — the elephant refit is this project's standing lesson on that.

## 🪤 `PROOF_FORMAT_VERSION` did not move, and cannot catch this

It is `3` under 1.2.0 and was `3` under 2.0.0, across an incompatible format change. The constant
that exists to identify the format was left where the withdrawn release had put it.

`REQUIRED_PROOF_FORMAT_VERSION`, the journal gate's clause built for exactly this, therefore
accepts a 2.0.0 journal. It is not a defect in the clause: no value it could hold would separate
two formats that declare the same number. What actually refuses an old proof is verification
failing, the circuit fingerprint having moved, and the guest verifying keys having moved — three
refusals, none of them the one designed for the job. Recorded because the next reader will
otherwise assume that clause covers this transition.

## The changes this repository made

**Dependency resolution — reversed.** Every manifest asked for `=2.0.0` from crates.io with no
path patch, because a patch that silently disengages once cost this project a round of proofs.
1.2.0 is unpublished and every published version is yanked, so there is no registry copy to build
against and the crates are now patched to `../../zkboo/*`. The guard that enforced a registry
source now enforces a path source, and the old failure is caught by the requirement itself: no
`1.2.0` exists on crates.io, so a disengaged patch fails to resolve loudly rather than resolving
to something else quietly. `host/build.rs` also records each checkout's git revision and dirty
state into the binary, where it used to record a lockfile checksum. Restore the registry
arrangement when 1.2.0 is published.

**The Keccak-256 hasher, vendored.** `zkboo` ships no concrete `Hasher` at all now; the
implementation moved to `zkboo-harness`, which is `publish = false` and needs `std`. It is copied
verbatim into `statement/src/hasher.rs`, forty lines, still patched to SP1's accelerated
`tiny-keccak` so a guest still drives the Keccak precompile. No digest changes: the hasher moved,
the transcript did not.

**`n_eps`, vendored** into `snark_lifter_host` from the same harness, for the same publishability
reason. The arithmetic is unchanged.

**Entry points.** Fifteen collapsed into six, every optional argument travelling in an options
value: `exec(circuit, ExecOptions::new())`, `verify(circuit, output, proof, binding,
VerifyOptions::new())`, `build_challenge_entropy(..., ChallengeOptions::new())`,
`ProofBuilder::new(..., ProofOptions::new())`. `par_prove` and `par_verify` are unchanged.

**The cross-leaf gate's challenge framing.** `split::aggregate` re-runs the second half of
`par_verify`, so it opens the challenge transcript exactly as zkboo does. It held its own copy of
the tag, `ZKBOO-BINDING-v1`. It now calls `zkboo::crypto::absorb_framed` with
`zkboo::crypto::TAG_CHALLENGE`, so the constant cannot drift from the library again — which it
would have done here silently, re-deriving a challenge sequence no honest proof could match.

**The assertion flag is now this repository's responsibility.** 2.0.0 appended it to every
circuit's output automatically, from the backend. 1.2.0 makes it circuit-side sugar, so a circuit
that does not call `Assertions::output` emits no flag at all — and the journal gates, which
require the trailing word to be `1`, would refuse every proof of it. Both circuits emit it
explicitly now. On `EthereumAddressCircuit` it is load-bearing: the affine comb runs on advice and
the flag is the conjunction of the assertions pinning that advice down. On `HardenedChildCircuit`
nothing is asserted, so it is the constant `1` and carries no information; it is emitted anyway so
that one journal shape, one Rust gate and one Solidity decoder serve every statement.

That the hole itself is unchanged is worth stating plainly, because the release was expected to
close it. It does not, and `zkboo`'s own documentation now says so: *"Advice not covered by an
assertion is unconstrained, and nothing in this crate can detect that."* The forgery route is
exactly as recorded — honest prover, unsatisfying witness, perfectly valid proof, flag `0`, any
address, no seed — and the gates in `check_journal`, `check_elephant_journal` and
`LiftJournalGate.sol` remain the enforcement rather than a defence in depth behind it.

## What has not been done yet

Nothing has been built or proved: the machine was committed elsewhere. Outstanding, in order:

1. Build the workspace and the six guests; run `vkeys` and re-pin both leaf keys.
2. Re-prove the cached leaf fixtures — `fixtures/leaves/n4`, and `fixtures/elephant-leaves/n2` at
   roughly two and a half hours.
3. Re-run acceptance: `s1-acceptance`, `split-probe 4 both`, the three forge negatives,
   `elephant-split 2 gate`.
4. Regenerate the on-chain fixture with `lift 219` and update `EXPECTED_VKEY` and
   `EXPECTED_FINGERPRINT` in `verifier/test/LiftGroth16.t.sol`.

⚠️ Until step 4, `forge test` goes green against a fixture produced by the withdrawn 2.0.0 stack.
The fixture is internally consistent, so nothing in the suite can tell; the `EXPECTED_VKEY` pin
fires on the run *after* regeneration, not before it.

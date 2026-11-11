# Moving to zkboo 1.2.0

The zkboo ecosystem was re-architected and re-released as **1.2.0** on 2026-09-04, replacing the 2.0.0 line this repository had been built against.
2.0.0 is withdrawn: its history is gone from every remote, all thirty-two previously published crates.io versions are yanked, and 1.2.0 is not published at all.
The upstream record is the zkboo repository's `knowledge/archive/handover-2026-09-06.md` and `knowledge/archive/migration-1.2.0.md`, which are not yet public; this file records what it cost here and which of this repository's numbers it invalidates.

## Superseded measurements

Not "probably drifted" — arithmetically invalid, for three independent reasons, any one of which is enough on its own.

1. **Every proof.**
   1.2.0 domain-separates three hashing roles that were previously separated once, partially, or not at all: the Fiat-Shamir challenge transcript (`ZKBOO-CHALLENGE`), the party view commitment (`ZKBOO-VIEW-COMMITMENT`) and the hash-based PRG (`ZKBOO-PRG`), each absorbing its tag and an 8-byte little-endian length before its payload.
   Every commitment and therefore every challenge differs.
   No proof made before this verifies after it.
2. **Every circuit in the suite.**
   The `Backend` trait is back to its 1.0.0 surface.
   Prover advice is no longer a gate (`Backend::hint` is gone) but an ordinary secret input the prover computes on the host, and assertions are no longer accumulated by the backend but by circuit-side sugar (`zkboo::circuit::Assertions`) that a circuit must explicitly emit.
   Both statements this repository lifts changed shape as a result, so both fingerprints moved.
3. **Every verifying key.**
   Both of the above reach the guests, and so does the vendored hasher below.
   All six keys move; the two pinned in `pinned-keys` are marked stale in place.

So the tables in `performance-table.md`, `elephant-cost.md`, `iteration.md` and `split-and-streaming.md` describe a stack that no longer exists.
They are kept, banner-marked, because the *method* and the *shape* of those results carry over and the cost of re-taking them does not.
Nothing in them may be quoted as current, and no figure in them may be rescaled by a factor observed elsewhere — the elephant refit is this project's standing lesson on that.

## `PROOF_FORMAT_VERSION` did not discriminate; replaced upstream

It was `3` under 1.2.0 and `3` under the withdrawn 2.0.0, across an incompatible format change: the constant whose whole job was identifying the format had been left where the earlier release put it.
`REQUIRED_PROOF_FORMAT_VERSION`, the journal gate's clause built for exactly this, therefore accepted a 2.0.0 journal, and no value it could have held would have separated two formats declaring the same number.
What actually refused an old proof was verification failing, the circuit fingerprint having moved, and the guest verifying keys having moved — three refusals, none of them the one designed for the job.

**Closed upstream on 2026-09-06.**
The counter is deleted; `zkboo::PROOF_FORMAT_ID` is a `[u8; 32]` that *is* the digest of a canonical reference proof, so the identity and the bytes it names are one record rather than two, and re-pinning them is the same edit as announcing the change.
It over-triggers by design — the canonical proof is made by a circuit, so a change to the circuit vocabulary moves it too — which costs a regeneration where the counter cost a silent acceptance.
See `zkboo-next-delta.md` for what absorbing it changed here.

## Changes made here

**Dependency resolution — reversed.**
Every manifest asked for `=2.0.0` from crates.io with no path patch, because a patch that silently disengages once cost this project a round of proofs.
1.2.0 is unpublished and every published version is yanked, so there is no registry copy to build against and the crates are now patched to `../../zkboo/*`.
The guard that enforced a registry source now enforces a path source, and the old failure is caught by the requirement itself: no `1.2.0` exists on crates.io, so a disengaged patch fails to resolve loudly rather than resolving to something else quietly.
`crates/host/build.rs` also records each checkout's git revision and dirty state into the binary, where it used to record a lockfile checksum.
Restore the registry arrangement when 1.2.0 is published.

**The Keccak-256 hasher, vendored.**
`zkboo` ships no concrete `Hasher` at all now; the implementation moved to `zkboo-harness`, which is `publish = false` and needs `std`.
It is copied verbatim into `crates/statement/src/hasher.rs`, forty lines, still patched to SP1's accelerated `tiny-keccak` so a guest still drives the Keccak precompile.
No digest changes: the hasher moved, the transcript did not.

**`n_eps`, vendored** into `pofseed_sp1_lift_host` from the same harness, for the same publishability reason.
The arithmetic is unchanged.

**Entry points.**
Fifteen collapsed into six, every optional argument travelling in an options value: `exec(circuit, ExecOptions::new())`, `verify(circuit, output, proof, binding, VerifyOptions::new())`, `build_challenge_entropy(..., ChallengeOptions::new())`, `ProofBuilder::new(..., ProofOptions::new())`.
`par_prove` and `par_verify` are unchanged.

**The cross-leaf gate's challenge framing.**
`split::aggregate` re-runs the second half of `par_verify`, so it opens the challenge transcript exactly as zkboo does.
It held its own copy of the tag, `ZKBOO-BINDING-v1`.
It now calls `zkboo::crypto::absorb_framed` with `zkboo::crypto::TAG_CHALLENGE`, so the constant cannot drift from the library again — which it would have done here silently, re-deriving a challenge sequence no honest proof could match.

**The assertion flag is now this repository's responsibility.**
2.0.0 appended it to every circuit's output automatically, from the backend.
1.2.0 makes it circuit-side sugar, so a circuit that does not call `Assertions::output` emits no flag at all — and the journal gates, which require the trailing word to be `1`, would refuse every proof of it.
Both circuits emit it explicitly now.
On `EthereumAddressCircuit` it is load-bearing: the affine comb runs on advice and the flag is the conjunction of the assertions pinning that advice down.
On `HardenedChildCircuit` nothing is asserted, so it is the constant `1` and carries no information; it is emitted anyway so that one journal shape, one Rust gate and one Solidity decoder serve every statement.

That the hole itself is unchanged is worth stating plainly, because the release was expected to close it.
It does not, and `zkboo`'s own documentation now says so: *"Advice not covered by an assertion is unconstrained, and nothing in this crate can detect that."*
The forgery route is exactly as recorded — honest prover, unsatisfying witness, perfectly valid proof, flag `0`, any address, no seed — and the gates in `check_journal`, `check_elephant_journal` and `LiftJournalGate.sol` remain the enforcement rather than a defence in depth behind it.

## Completed 2026-09-07

All four steps this file left outstanding are done, on this machine, on a quiet run.

1.  Workspace and all six guests built; `vkeys` run and both leaf keys re-pinned (`LEAF_VK` `0x0c330b47…`, `LEAF_ELEPHANT_VK` `0x2e310e62…`).
2.  Cached leaf fixtures re-proved: `fixtures/leaves/n4` via `split-probe 4 both`; `fixtures/elephant-leaves/n2` via `elephant-split 2 both` — 6 h 28 m, leaf 3,919.77 s, root 15,476.43 s, gate green.
3.  Acceptance re-run green — `s1-acceptance` (seven-case reject matrix plus a full-scale positive at 219), the nine-case cross-leaf gate with the control accepting, and the three forge negatives (whose refusal layer is  **undetermined** — see `split-and-streaming.md`; the earlier "host bookkeeping" attribution is retracted).
4.  On-chain fixture regenerated by `lift 219` and all three Solidity pins re-anchored in the same edit: vkey `0x00f1810f…`, fingerprint `0xbd121f6f…`, and the format clause converted from a one-byte counter to the thirty-two-byte `EXPECTED_PROOF_FORMAT_ID`.
    `forge test`: 18 passed.

The warning this section used to carry — that `forge test` goes green against a fixture from the withdrawn stack, and the `EXPECTED_VKEY` pin only fires on the run *after* regeneration — played out exactly as written.
It is why the pins were updated in the same commit as the artefact rather than left to fail once first.

### Re-measured figures

Both columns are on the safe backend path, so the comparison isolates the release.
Neither is the current cost: the backend defect that forced that path was fixed on 2026-09-08, and the lift now runs in 3,613.03 s.
See `performance-table.md`.

| Quantity                    | 2.0.0 stack | 1.2.0 stack | Change |
| --------------------------- | ----------: | ----------: | -----: |
| `lift 219` zkVM proving     | 10,296.11 s |  9,927.49 s | −3.58% |
| `lift 219` peak RSS         |    77.49 GB |    76.52 GB | −1.25% |
| Journal length              |       188 B |       219 B |  +31 B |
| Groth16 proof               |       356 B |       356 B |      — |
| On-chain pairing verify ⁽ᵍ⁾ | 270,441 gas | 272,635 gas | +0.81% |
| On-chain journal gate ⁽ᵍ⁾   |  85,287 gas |  89,325 gas | +4.74% |

⁽ᵍ⁾  **Both gas rows measure the test harness as well as the subject, and are superseded within the day they were taken.**
The suites hold the fixture in contract storage, so the measured region pays a cold SLOAD per 32-byte word.
What a relying contract pays is **225,975** and **54,615**, and the entire +2,194 is one extra cold storage slot — the journal crossing from six words to seven.
The instruments are fixed and the full three-arrangement measurement is in `performance-table.md`.
The +31 journal bytes are exactly the format field growing from a varint to a bare 32-byte array; on memory-resident values they cost 79 gas.

Instruction laws, both refitted rather than scaled (see `instructions-are-not-seconds.md`):

- Ethereum address: `5.0986e9 + 2.1148e9 × n`, marginal −14.13%, fixed unmoved (−0.035%).
- Hardened child: `3.0274e7 + 1.9889e7 × n`, marginal −8.18%, fixed −3.8%.
- Elephant leaf, measured not derived: 2,114,787,163 instructions, −11.70%.

Three different savings for three different things.
None of them predicts another, and none of them is a time saving.
Measured on both statements now: the hardened child gave back 8.14% of its instructions and 3.58% of its seconds; the elephant leaf gave back 11.70% and 6.84%.
Both got slower per instruction — 4.74% and 5.50% — because the release changed the instruction mix as well as the count.
See `instructions-are-not-seconds.md`.

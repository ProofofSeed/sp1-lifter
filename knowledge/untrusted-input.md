# What the guests read from untrusted bytes

Audit of 2026-10-09, prompted by the forgery of Google's quantum-cryptanalysis attestation (Ryan, Trail of Bits, 2026-04-17), where two guest bugs became proof bugs: an invalid enum discriminant read from untrusted circuit bytes through `rkyv::access_unchecked` took an unintended jump-table branch, and a gate did not check that its control and target qubits were distinct.
The question here is whether the six guests of this repository, which deserialise an untrusted ZKBoo proof and run zkboo's verifier on it, have an entry point of that class.
They do not; what follows is the evidence and the one change it produced.

## Deserialisation

- Every frame and journal is postcard over serde derives (`crates/statement`, `crates/split`, `crates/elephant`), and the guests contain no `unsafe`, no `transmute`, no unchecked cast and no rkyv.
- zkboo's `unsafe` is confined to in-place word operations on owned values, single-threaded global word pools and `ManuallyDrop` field moves; none of it reads untrusted bytes.
- The only enum-like value in a proof is the challenge party, which zkboo's `Party` rejects outside `0..=2` at deserialisation (`prover/challenge/challenge.rs`, upstream commit `8ba7079` of 2026-08-19); its field is private and both `From` impls reduce modulo 3, so the `unreachable!()` in the replay backend's input arm is unreachable.
- That invariant is the one load-bearing validation: the replay's finalisation permutes the three commitments by `(3 - party) % 3` and its neighbours and moves each out exactly once, which a party of 3 or more would turn into a double move in a release build.
- A leaf journal's `party` is a `u8` the root re-checks before use, and the Fiat-Shamir match binds it in any case.

## The replay's length and shape assumptions

- A short AND-message vector is zero-filled past its end without advancing the cursor, and `check_consumed` requires the cursor to equal the vector's length, so a short vector passes only when the cursor lands exactly on its length and a long one never passes.
  This is encoding malleability without soundness content: the prover chooses the AND-message words freely, so every word sequence a short encoding induces is also the induced sequence of some explicit full-length encoding, and no consumer hashes or deduplicates proof bytes.
- The party-2 input share is present exactly when the challenge opens party 2, checked by `is_well_formed` before any replay in `verify`, `par_verify` and both leaf guests.
- `verify` returns `Ok(false)` unless the proof carries exactly the named repetition count, the sequential verifier re-checks the ingested count at finalisation, the lift guest names the count the proof carries and commits it, and the gates demand their minimum of the committed count.
- Finalisation refuses a replayed output whose shape differs from the expected output, and `Words::update_hasher` frames each word type with a feature byte and a length, so neither the challenge transcript nor the statement digest has a concatenation ambiguity.
- There is no analogue of the aliasing bug: a proof supplies seeds, words and one digest, never circuit structure, and every wire index comes from the circuit compiled into the guest.
- Guests are release builds without overflow checks; the only arithmetic on untrusted-derived values is a cursor compared with a length and a position compared with an index.

## A panic is never an accepted proof

- In the guest, a panic reaches `sys_panic`, which halts with exit code 1 (`sp1-zkvm 6.4.0`, `syscalls/sys.rs`), and SP1's own SDK test expects exit code 1 of a panicking guest.
- The exit code is chained through every shard and the compress circuit and committed as the third Groth16 public input.
- `client.verify(proof, vk, None)` demands success, `SP1VerifierGroth16.verifyProof` reverts on a non-zero exit code before the pairing check, and the deferred-proof circuit asserts a zero exit code on every admitted leaf.
- Out-of-memory, an overflowed input region and non-termination each yield no proof.
- This covers panics only: a wrong computation that terminates is not caught by the exit code, which is why the two sections above carry the weight.

## What the gates would still catch

The journal gate catches a wrong version, verdict, fingerprint, statement, binding, count, format identity or assertion word, and trailing bytes; the cross-leaf gate catches a wrong leaf version, position, statement digest or party, a challenge sequence that does not re-derive, and an empty set, with the leaf key compiled into the root.
None of them would catch a guest that accepted an invalid response and committed a well-formed journal, and no entry point for that was found.

## What changed

- Both root guests decode a leaf's public values with `postcard::take_from_bytes` and refuse a non-empty remainder, as the host's `decode_journal` and the on-chain decoder already did; `from_bytes` tolerates trailing bytes.
  A leaf commits exactly one journal on its accepting path, so nothing admissible changes, and both root keys move.
- `crates/host/tests/frame_fuzz.rs` mirrors the lift guest natively over a 32-response proof and 256 deterministic mutations of its frame (bit flips, header bit flips, substitutions, truncations, insertions, deletions, zero runs), requiring every mutated frame to be refused by the decoder, refused by the verifier or to panic, or to verify as exactly the honest statement when only bytes past the decoded frame changed.
  The first run, over a three-response proof, accepted a substituted byte on its 25th round: a mutated proof passes the challenge match with probability 3^-n, which is 1 in 27 at three responses and 3^-32 at the count the test now uses, so it runs in about seven seconds and its one accepting round is a byte appended past the frame.
  The run tallied 132 decoder refusals, 122 verifier refusals, one version refusal and no panics.

## Checks run on the rebuilt guests

- `vkeys` passes: the lift, elephant and both leaf keys are unmoved and the two leaf pins hold.
- `split-probe 4 root` proves a root over the four cached leaves and passes the relying-party gate; `forge-vk`, `forge-count` and `forge-order` refuse as before, with the refusing layer of the last two as undetermined as before.
- `elephant-split 2 execute-root` executes the elephant root over its two cached leaves at 5,100,199,840 instructions, 102 above the root instruction law's value at two leaves, which is the remainder check.
- The fuzz test passes in about seven seconds.
- No proving time from these runs is a figure: the fuzz test was proving alongside the root.

## Not checked

- The zkvm target's panic handler in the Succinct toolchain, whose source is not on this machine; the evidence is the hook it calls and the SDK test.
- SP1's binding of a deferred proof's key and public-values digest beyond the zero-exit-code assertion.
- Bounds inside zkboo's word pools, whose indices are circuit-driven and not attacker-controlled.

# What removing Poseidon would take

Asked 2026-08-28: the Ethereum Foundation no longer believes in Poseidon, so how bad would getting rid of it be?

This is the feasibility investigation, not a plan.
Everything below is read out of the 6.4.0 sources or measured; where it is an inference it says so.

## 1. Where Poseidon actually is

**Stage 1 is clean.**
The ZKBoo crates use **Keccak-256** throughout — commitments, PRGs, challenge entropy.
`grep -ril poseidon` over `<zkboo>/*/src` matches only `zkboo-plonky3-draft`, an abandoned prototype that nothing depends on.
**Nothing on the device touches Poseidon.**

**Our lifter code is clean.**
One match across `crates/statement/`, `programs/program/`, `programs/program-leaf/`, `programs/program-root/`, `crates/split/`, `crates/elephant/`, `crates/host/` and `verifier/`, and it is a comment describing what SP1 does.

**It is entirely SP1's**, through `p3-poseidon2` and `slop-poseidon2`, in four roles:

| role                                   | where                                     | what breaks if the hash breaks           |
| -------------------------------------- | ----------------------------------------- | ---------------------------------------- |
| Merkle/trace commitment (`Compressor`) | `slop-merkle-tree`, every committed round | binding of the committed trace           |
| Sponge hasher (`Hasher`)               | leaf hashing, `hashable_key`              | the verifying key's identity             |
| Duplex challenger (`Challenger`)       | Fiat-Shamir across every phase            | non-interactive soundness                |
| Deferred-proof digest chain            | `sp1_primitives::hash_deferred_proof`     | the leaf/root binding the split rests on |

Plus a **second, separate instantiation over BN254** (`SP1OuterGlobalContext = BNGC<…>`) for the shrink and wrap stages that produce the on-chain proof.

## 2. What a break would and would not cost us

**It would not leak a seed.**
Zero-knowledge of the witness is ZKBoo's property, and ZKBoo is Keccak-based.
A Poseidon2 weakness does not help anyone recover a seed, on the device or off it.

**It would let someone forge a lift.**
The commitment scheme and Fiat-Shamir both rest on it, so a collision or a structural attack is a soundness break: a prover could produce a Groth16 proof attesting a ZKBoo verification that never happened.
Against an ERC-4337 account that accepts a lift as authorisation, that is the whole of the security.

Note the existing ceiling: this track is already capped at **BN254's ~100-bit pairing security**.
Poseidon2 matters if it is weaker than that, and the question is whether it now is.

## 3. Swappability of the hash

SP1 is generic over an `IopCtx` with associated `Digest`, `Challenger`, `Hasher` and `Compressor`.
`KoalaBearDegree4Duplex` is one instantiation of it, and it is pure Poseidon2:

```rust
type Challenger  = DuplexChallenger<F, KoalaPerm, 16, 8>;
type Hasher      = PaddingFreeSponge<KoalaPerm, 16, 8, 8>;
type Compressor  = TruncatedPermutation<KoalaPerm, 2, 8, 16>;
```

So the *shape* is right, and `sp1_primitives::SP1GlobalContext` is a one-line alias pointing at it.
Changing that alias is a fork of `sp1-primitives`, which is a thing this project already does — `sp1-metal` vendors six SP1 crates for the GPU backend.

**That is where "easy" stops.**

## 4. Why it is not a configuration change

The prover is only half of it.
**The recursion layer must verify the hash inside a circuit**, and SP1 does that with dedicated machinery built for Poseidon2 specifically:

- `sp1-recursion-machine/src/chips/poseidon2_wide` and `poseidon2_helper` — the recursion VM has **Poseidon2 in its instruction set**, as chips of its own.
- `sp1-recursion-circuit/src/{challenger,hash}.rs`, `basefold/tcs.rs` — the in-circuit duplex challenger and two-to-one compressor, hand-written against Poseidon2.
- `sp1-recursion-compiler` emits those instructions.
- `sp1-verifier/src/compressed/internal.rs` — the on-chain-adjacent verifier.

A different hash needs a new recursion chip, new in-circuit verifiers, a compiler that emits them, and re-derived FRI/basefold soundness and grinding parameters.
That is a proof-system project, not a swap.

## 5. What a non-algebraic hash would cost — measured

This is the number the question turns on, and it is why algebraic hashes exist.
Measured from the crates SP1 links, in trace cells per permutation:

|                                  | columns | rows/permutation | cells/permutation | bytes absorbed | cells per absorbed byte |
| -------------------------------- | ------: | ---------------: | ----------------: | -------------: | ----------------------: |
| Keccak-f[1600] (`p3-keccak-air`) |   2 633 |               24 |        **63 192** |            136 |                   464.6 |
| Poseidon2, SP1's wide chip       |     179 |                1 |           **179** |             32 |                     5.6 |

**353× per permutation, 83× per absorbed byte.**

Output: `fixtures/probe-air-width.log`.
The probe reads the constants out of the crates rather than quoting them, so it cannot go stale silently against a version bump — against `p3-keccak-air` `=0.4.3-succinct` and `sp1-hypercube` `=6.4.0`:

```rust
use sp1_hypercube::operations::poseidon2::permutation::NUM_POSEIDON2_DEGREE3_COLS;

let kc = p3_keccak_air::NUM_KECCAK_COLS;   // 2633
let kr = p3_keccak_air::NUM_ROUNDS;        //   24
let pc = NUM_POSEIDON2_DEGREE3_COLS;       //  179, and one row per permutation
```

Keccak absorbs 136 bytes per permutation against Poseidon2-KoalaBear-16's 32 (eight field elements), which is where the 353× per permutation becomes 83× per absorbed byte.

Recursion is *made of* hashing — every Merkle path, every challenger absorb — so replacing Poseidon2 with Keccak, SHA-3 or Blake3 multiplies the recursion layer by something approaching that.
It would not be a slower lifter; it would be a different feasibility class.

**83× is a per-byte trace-cell ratio, not an end-to-end multiplier**, and it is deliberately not turned into one here.
What the end-to-end factor would be depends on how much of each phase is hashing, which is not measured.
For scale, on a phase profile of a real proof taken by `sp1-metal`, `commit` is 13.7% of prover thread-time and is essentially all hashing, `pcs` is 19.0% and is partly Merkle paths, and `recursion` is 2.2%.

## 6. The options, and what each is worth

**A.
Stay, and state the dependency.**
Record that the lift's soundness rests on Poseidon2 over KoalaBear and over BN254, alongside the BN254 ~100-bit ceiling already stated.
Costs nothing, changes nothing, and is only acceptable if Poseidon2 at these parameters is in fact fine.

**B.
A different algebraic hash** — Monolith, Rescue-Prime, Griffin, Anemoi, Tip5.
Same cost class in-circuit, so §5's penalty does not apply.
The work is a new recursion chip, in-circuit verifier, compiler support and parameters — and **none of these ships in the `slop`/Plonky3 tree we depend on**, so it would have to be written.
This is the realistic path *if the concern is Poseidon specifically rather than algebraic hashes generally*.

**C.
A bit-oriented hash.** §5 says no.

**D.
A different proving stack.**
Every production recursive STARK — Plonky3, SP1, RISC Zero, Boojum — uses a Poseidon-family hash for exactly the reason in §5.
"No algebraic hash" is close to "no recursive STARK", so this is a change of architecture rather than of component.

## 7.  The question that has to be answered before any of this is scoped

**Which claim is it?**
The options above diverge completely depending on the answer, and this note cannot pick between them:

- If the concern is **Poseidon v1 and not Poseidon2**, our exposure may already be nil — SP1 uses Poseidon2 throughout.
- If it is **specific parameter sets** (round counts, S-box degree, a particular field), the fix may be re-parameterising rather than replacing, which is far cheaper than B.
- If it is **Poseidon2 over BN254 specifically**, only the outer context is exposed, and that is the smaller half.
- If it is **the algebraic-hash design approach in general**, then B does not help and only D remains.

## 8. What also carries this exposure

The **STARK lifter** — the post-quantum track, now on the larger server — ratified Poseidon2-inner with Keccak-outer, so it has the same dependency and a larger one, since it has no BN254 layer beneath which the question is moot.
Anything decided here applies there.

## 9. Recommendation

Do not start B before §7 is answered.
It is months of proof-system work, it lands on a fork of a stack we do not own, and three of the four possible readings of the concern make it the wrong move.

What is worth doing now, and is cheap:

1. **Get the actual claim**, with a reference.
2. **State the dependency in the paper** either way.
   The security section already names BN254's ~100 bits as the ceiling and does not currently name Poseidon2 at all; it should, because a reader cannot otherwise tell what the lift rests on.
3. **Keep stage 1 out of it.**
   The device and the ZKBoo layer are Keccak-based and unaffected, and that is worth saying explicitly rather than leaving a reader to assume the whole stack is exposed.

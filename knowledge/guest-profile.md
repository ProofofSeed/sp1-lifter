# Where the guest's instructions go

Measured 2026-09-15 with SP1's own instruction profiler (`TRACE_FILE`, under the `profiling` feature `crates/host` already enables), against the binaries built 2026-09-09 on zkboo 1.2.0, executing only.
Each instruction is attributed to the innermost function executing it; figures are self counts unless marked inclusive.

- `cycle-probe 2`: the monolithic hardened-child lift guest over two responses, every instruction recorded. 70,053,538 instructions, on the law `3.0274e7 + 1.9889e7 × n`.
- `elephant-split 2 root-law` with `TRACE_SAMPLE_RATE=100`: the elephant root guest over two natively replayed leaves, one instruction in a hundred. 5,100,200,480 instructions, on the 2026-09-08 root law.

Rollups: `fixtures/guest-profile-lift-n2-2026-09-15.summary.log` and `fixtures/guest-profile-elephant-root-n2-2026-09-15.summary.log`.

The stack has since moved to zkboo 1.3.0 (`zkboo-1.3.0-migration.md`), which adds the BIP-32 validity assertions to the hardened child: +0.67% of guest instructions at 219 responses, and nothing that moves the shares below by more than that.
The elephant statement is untouched by it.

## Hardened child, per response

The `verify` subtree is 38,677,379 instructions over two responses, 19.34 M per response.
Frame decode is outside it, at 0.56 M per response, almost all of it postcard reading every AND-message `u64` as a varint.

| category                                                   | per response | share of `verify` |
| ---------------------------------------------------------- | -----------: | ----------------: |
| replay backend: gate evaluation, tape draws, message reads |       5.94 M |             30.7% |
| `WordRef` gate front end                                   |       3.76 M |             19.4% |
| word pool, `AllocSet`, refcounts                           |       3.05 M |             15.8% |
| `tiny-keccak` software sponge: absorb, pad, squeeze        |       3.03 M |             15.7% |
| `memcpy` and `memset` under the hasher and the PRG         |       1.83 M |              9.4% |
| `HashPRG` glue                                             |       0.80 M |              4.1% |
| circuit code: SHA-512, HMAC, BIP-32                        |       0.29 M |              1.5% |
| zeroize                                                    |       0.22 M |              1.1% |
| heap allocator                                             |       0.20 M |              1.0% |
| `Keccak256Hasher` wrapper                                  |       0.14 M |              0.7% |
| Keccak-permute syscall stubs                               |       0.02 M |              0.1% |

By cause, inclusive:

- **CARRY replay: 7.63 M, 39.5%.** `ViewReplayerBackend::carry` for `u64` walks the 64 bits of each of 3,068 gates one at a time, for both parties.
- **PRG: 4.72 M, 24.4%.** Each 32-byte tape block is three `update` calls absorbing 49 bytes one byte at a time, a `finalize` that swaps in a freshly zeroed 200-byte state, and a zeroising buffer replacement: about 1,460 instructions around one precompile call, 3,226 times per response.
- **View-commitment absorb: 1.27 M, 6.6%**, one 8-byte `update` per hashed word per party, called from AND and CARRY.

Hashing and PRG glue together — sponge, its copies, PRG glue, zeroize, wrapper and stubs — is 31.2% of `verify`, about 6.0 M instructions per response spent around 3,988 permutations.

The fixed term is `hash_circuit`: 30.24 M instructions, 73.5% of them the software sponge.

## Elephant root

The `hash_circuit` pass over `EthereumAddressCircuit` is 99.97% of the root guest; the aggregate gate and the deferred-proof claims are the remaining 0.03%.

| category                                            | share |
| --------------------------------------------------- | ----: |
| `tiny-keccak` software sponge: absorb, pad, squeeze | 71.9% |
| word pool, `AllocSet`, refcounts                    | 10.9% |
| `WordRef` gate front end                            | 10.4% |
| circuit-hash backend                                |  4.1% |
| `Keccak256Hasher` wrapper                           |  1.3% |
| Keccak-permute syscall stubs                        | 0.24% |
| other                                               |  1.2% |

About 3.7e9 of the root's 5.1e9 instructions absorb bytes into a sponge whose digest does not depend on how they are absorbed.
The syscall stub is five instructions per call, calibrated on the hardened-child `verify` subtree (39,910 instructions against 7,976 expected permutations), which puts the pass at about 2.4 M permutations, about 330 MB absorbed, and about 11 software instructions per absorbed byte.

## What this retracts

`iteration.md` read the guest as running "at approximately native efficiency", comparing its instruction count with a native wall-clock figure.
Natively the permutations are most of the work; in the guest they are syscalls, so the comparison could not see what surrounds them.
About a third of a response's instructions are glue around the precompile and another third memory-manager bookkeeping.

## Candidates, none measured

Every candidate below accepts exactly the same proofs and yields the same digests, and every one moves the guest verifying keys.

In the lifter alone:

1. A Keccak-256 `Hasher` over lanes: buffered absorb, a direct syscall, no state swap on finalize. In the elephant root it addresses up to 73% of the instructions; in a response, the 3.03 M of sponge and part of the 1.83 M of copies.
2. A Keccak-specialised `PseudoRandomGenerator` producing the byte stream of `HashPRG<Keccak256Hasher>`: a prepadded lane template, the counter written in place, four lanes read out. It addresses most of the PRG's 4.72 M, and needs a differential test against `HashPRG`.
3. A flat word pool with a free list in place of `OwnedFlexibleWordPairPool`, addressing part of the 3.05 M.
4. `lto = "fat"` and `codegen-units = 1` for the guests. No release profile is set, so cross-crate calls such as `AllocSet::alloc` and `tiny_keccak::Keccak::update` remain calls.
5. `TRACE_CHUNK_SLOTS=3`, which was 9.5% faster on the CPU in August and has never been taken on the Metal path; every full-scale Metal run used the default of 5.
6. For the elephant root, the fingerprint computed at build time rather than in the guest. `EthereumAddressCircuit::public_only()` takes no parameters, so the fingerprint is one value for the statement, and the same code can compute it natively and embed it; the journal and gates are unchanged. The pass then disappears, instructions and permutation rows both. The value becomes bound by the verifying key instead of recomputed under it, which is a design decision rather than a performance one. The hardened child's fingerprint covers its chain code and index, so it cannot be embedded, and is 0.7% of that lift in any case.

In zkboo, output-identical:

- CARRY replay in closed form, as a prefix scan over the carry recurrence, addressing most of the 7.63 M. It must be differentially tested against the bit loop over all challenges; written in a lifter-owned backend instead, it would reopen the lift-fidelity gap.
- The `WordRef` front end's `Rc`, `RefCell` and checked-refcount overhead, an API-level change.

## What instructions do not settle

A Keccak permutation is 24 rows of a chip weighted 2,640 plus a control row weighted 682 in SP1's cost table (`rv64im_costs.json`), against roughly 150–200 for an ordinary instruction across fetch (36), decode (61), the operation's chip and memory.
Removing glue leaves the permutation rows standing, 873,372 of them in a 219-response hardened-child lift, and leaves the compress, shrink and wrap tail of about 171 s.
The elephant root proves at 0.575× the elephant leaf's rate per instruction and is far denser in permutations, so how much of its 5,911 s the permutation rows account for is not known, and neither is what a faster hasher would buy there in seconds.
Computing the fingerprint at build time removes both.

## Against the Flock lifter

The Flock lifter proves the same 219-response hardened-child lift in 13.30 s against 3,613.03 s here, about 272× faster (`flock-lifter/knowledge/performance-table.md`, re-measured 2026-09-16; it was 12.33 s, about 293×, when this profile was taken).
That gap is structural: this lifter proves a RISC-V machine executing the verifier, one proven state transition per instruction, while Flock proves the verification relation directly, one committed bit per AND gate with XORs free.
Nothing found here closes it.
The glue and bookkeeping above are a small multiple of the hardened child's guest instructions, not orders of magnitude, and the permutation rows and the recursion tail would remain.
The two also end in different places: a 356-byte proof checked on L1 for 280,590 gas here, a 1.68 MB proof there, verified natively in 0.02 s or by EVM contracts costing about 1.27 billion gas over at least 76 transactions, which Flock's own record calls not deployable as it stands.

# sp1-lifter

A succinct lifter for ZKBoo proofs of seed with classical (pre-quantum) security, built to be cheap to verify on Ethereum L1 today: the genuine `zkboo` Rust verifier runs as an SP1 zkVM program, and the verification run is wrapped (fully locally, via SP1's native gnark Groth16 layer — no Docker, no external service) into a single BN254 pairing proof checkable on-chain for a few hundred thousand gas.
This trades the post-quantum guarantees of the companion STARK lifter for immediate deployability; the secure-element story (stage 1) is unchanged.
See `knowledge/milestones.md` for what is established so far, including the recorded stack history (an initial RISC Zero implementation, superseded the same day for measured reasons).
The repository is named for its proof system because other back ends are expected alongside it, not because SP1 is the only way to lift these proofs.

The zkboo ecosystem was re-architected as 1.2.0 on 2026-09-04, changing every proof, every circuit fingerprint and every guest verifying key, with a second breaking change on 2026-09-06.
It has moved three times since: 1.2.1 on 2026-09-09, which this workspace skipped; 1.3.0 on 2026-09-19, which makes the BIP-32 derivations assert the validity conditions BIP-32 states and makes a verifier name the repetition count it demands; and `zkboo` alone to 1.3.1 on 2026-11-11, which makes a short AND-message or input-share vector fail replay instead of zero-filling.
Each port is complete and the stack has been rebuilt, re-keyed and re-proved; the 1.3.0 round re-measured the full-soundness lift, the split path at 4, 64 and 219 leaves, the elephant path and the on-chain cost, the 1.3.1 round re-measured the full-soundness lift, the split at four leaves and the on-chain cost, and each earlier round stands for every figure a later one did not re-take. The current figures are in `knowledge/performance-table.md` under "Current figures", and `knowledge/zkboo-1.2.0-migration.md`, `knowledge/zkboo-1.3.0-migration.md`, `knowledge/zkboo-1.3.1-migration.md` and `knowledge/zkboo-next-delta.md` record what changed and what it cost.
Figures elsewhere in `knowledge/` belong to the previous round and say so; none of them may be rescaled into a current one, for a reason this project measured rather than assumed (`knowledge/instructions-are-not-seconds.md`).

## Status

This is a research prototype and has not been audited.
It is a proof of concept of a lift usable on Ethereum L1 today, and nothing in it is production-ready.
The repository cannot be built by others yet: it patches the unpublished zkboo crates from local working trees, and the GPU backend it patches in for proving is not yet public.
What a reader can do today is read the code and the design record in `knowledge/`, run `forge test` in `verifier/`, and check the fixture against the vendored SP1 verifier.

Security caveats:

- The soundness of the wrapped proof is capped at roughly 100 bits by the security of the BN254 pairing, which is the weakest link of the classical track and is not removed by raising the repetition count.
- The default repetition count of 219 gives 128-bit classical security only in the forgery-work-ratio sense (an attacker making T Fiat-Shamir queries forges with probability at most T·(2/3)^219, so work over success probability is at least 2^128); stricter definitions would credit less.
  The repetition count is a runtime parameter, and 438 is the post-quantum 128-bit figure, which this track does not claim.
- Nothing here is post-quantum: the Groth16 wrap rests on pairing assumptions.
- Neither the host code nor `verifier/src/LiftJournalGate.sol` has been audited.
  The vendored SP1 verifier contracts are upstream code, audited upstream, and are not modified here.
- A valid proof can prove nothing.
  ZKBoo prover advice is constrained only by in-circuit assertions, accumulated into the last `u8` word of the circuit output (the assertion word), and an honest prover run on a bad witness yields a valid proof whose assertion word is 0, obtainable by anyone for any address.
  A relying party must therefore reject any journal whose assertion word is not 1, require a minimum response count, and pin the verifying key; `verifyProof` alone authorises nothing.
  `verifier/src/LiftJournalGate.sol` implements these checks, and `verifier/README.md` states how to use it.
- The Solidity gate is a library.
  No contract is deployed and there is no ERC-4337 account contract.

## Usage

Requires the SP1 toolchain (`sp1up`; sdk 6.4.0 with the `succinct` rust toolchain) and Go, for the native gnark wrap.
The ZKBoo crates come from local checkouts from sibling working trees (`../../zkboo/*` relative to the workspace root), patched in at the workspace root, because every release since 1.2.0 is unpublished and every published version is yanked.
The build script names the required version of each crate — they are released independently, with `zkboo` at 1.3.1, two of the ten still at 1.2.0 and the rest at 1.3.0 — and refuses anything that does not resolve at exactly that version from a path, recording each checkout's git revision and dirty state into the binary.
This inverts the arrangement that held while the crates were published, and should be inverted back when they are published again.

Proving runs on the GPU.
Build with `--features metal` and everything below goes through the Metal backend for SP1 Hypercube (`knowledge/gpu-proving.md`).
To measure against the CPU, force the GPU path off with the switches below rather than by rebuilding, so that both arms are one binary.

```
SP1_METAL_MIN_ENTRIES=999999999 SP1_METAL_MERKLE=0 SP1_METAL_TRANSITION=0 SP1_METAL_ZEROCHECK=0 SP1_METAL_ENCODE=0 SP1_METAL_JAGGED=0 SP1_METAL_FIRST_LAYER=0 SP1_METAL_JAGGED_VALUES=0 SP1_METAL_JAGGED_ASSIST=0 SP1_METAL_BATCH=0 SP1_METAL_UPLOAD_CACHE=0 SP1_METAL_GKR_ROUND=0
```

Use the full GPU path, with no `SP1_METAL_*` switches set.
The backend at `v0.1.0` produced a proof that failed its own verification about once in twenty-eight, and `SP1_METAL_MERKLE=0` was the workaround; the defect was fixed upstream, the workspace now builds a revision past the fix, and the workaround was retired on 2026-09-08.
See `knowledge/gpu-corruption.md` for the defect and `knowledge/performance-table.md` for what the full path costs.

```sh
cargo build --release -p pofseed-sp1-lift-host --features metal   # once; the binaries below are in target/release

./target/release/cycle-probe                # instruction-law sweep
./target/release/s1-acceptance              # reject matrix + full-scale 219
./target/release/lift 219                   # prove + wrap + Foundry fixture
./target/release/lift 8 compressed          # throughput probe, no wrap
./target/release/split-probe 4 both         # split-and-aggregate probe
./target/release/elephant-probe             # the secp256k1 statement's cost
./target/release/elephant-split 2 both      # the split, on the secp256k1 statement
./target/release/stream-lift 8              # leaves proved as responses arrive
./target/release/vkeys                      # print every key, check the pinned ones
cargo test --release -p pofseed-sp1-lift-host --test frame_fuzz   # mutated frames never verify as another statement
(cd verifier && forge test)                 # on-chain verify + gas + rejects
```

The Foundry suite reads a fixture that `lift` generates; without one it **skips**, and a fully skipped suite still prints `Suite result: ok`, so check for `18 tests passed` in the closing summary rather than `ok`.
The fixture in `verifier/fixtures/` was regenerated from a full-soundness run on zkboo 1.3.1 on 2026-11-12; of the three pins that describe it — verifying key, circuit fingerprint and proof-format identifier — the verifying key was re-anchored in the same change, and the other two did not move, because the release changes no circuit and no wire format.
That matters more than it sounds: a fixture is internally consistent whatever produced it, so a stale one passes the suite silently, and a pin taken from an artefact only fails on the run *after* the artefact is replaced.

`lift` performs one proving pass: in the default `groth16` mode the single SP1 call runs core → compress → shrink → wrap, so the on-chain fixture costs one pass rather than two.
`compressed` mode is for throughput probes at small repetition counts; it is not a stage to run before `groth16`.

Proving cost is tunable from the environment without a rebuild — `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD` and `TRACE_CHUNK_SLOTS` are read by `SP1CoreOpts::default()`, and `RAYON_NUM_THREADS` caps parallelism.
`lift` prints the configuration it used before it starts, because a proving time means nothing without it.

## Structure

Every root-level folder is a grouping: `crates/` for host-side Rust, `programs/` for the zkVM guests, `verifier/` for the Solidity, `knowledge/` for the design record and the measurements.

All crates are named `pofseed-sp1-lift-*`, after the project and the proof system, since other SNARK back ends are expected alongside this one.
None is published: crates.io has no namespace mechanism, so the prefix is what distinguishes them in local tooling, and every manifest carries `publish = false`.

### `crates/` — host-side

- `statement/` — shared host and guest definitions: the proof-of-seed circuits, the fixed Keccak-256 crypto suite, the postcard wire format, and the reference journal gate.
- `elephant/` — the secp256k1 Ethereum-address statement, kept separate so that probing it cannot move the main verifying key.
  See `knowledge/elephant-cost.md`.
- `split/` — the split-and-aggregate frames and the cross-leaf global Fiat-Shamir gate, which is what makes the lift distributable and streamable.
  See `knowledge/split-and-streaming.md`.
- `pinned-keys/` — the leaf verifying keys, compiled into the roots so that a root's own key says which leaf program it will admit.
  **Run `vkeys` after editing any crate a guest links, and after moving or renaming one: a comment-only change has been measured moving a guest key, and so has a change of package name or of path** (`knowledge/guest-keys-move-on-comments.md`).
  **It has no dependencies on purpose:** a crate the leaf programs link would move their keys whenever it changed, and the pin would never converge.
  `vkeys` re-derives every key from the built ELFs and fails if a pinned constant has drifted.
- `host/` — proof generation, the execution and proving drivers, and the acceptance harnesses.
  `host/tests/frame_fuzz.rs` mirrors the lift guest natively over mutated input frames; what the guests read from untrusted bytes, and why none of it is an entry point, is in `knowledge/untrusted-input.md`.
  `host/src/orchestrator.rs` dispatches a leaf proof as each response arrives rather than after the last one, so leaf proving overlaps response generation; driven by `stream-lift`, see `knowledge/streaming-orchestrator.md`.

### `programs/` — zkVM guests

- `program/` — the main lift: a thin shim around `zkboo::verifier::verify`.
- `program-elephant/` — the same for the Ethereum-address statement.
- `program-leaf/`, `program-root/` — one proof per response, plus a root carrying the cross-leaf gate.
- `program-leaf-elephant/`, `program-root-elephant/` — the same split over the Ethereum-address statement, which has no public parameters and a different journal shape, so it needs its own pair of guests.
  Driven by `elephant-split`.

### The rest

- `verifier/` — Foundry suite over the vendored audited SP1 Groth16 verifier (sp1-contracts v6.1.0), and the relying-party journal gate as a Solidity library (see `verifier/README.md`).
- `knowledge/` — design record, plan, and measured numbers.
  `performance-table.md` is the current cost of everything and the place to start; `gpu-proving.md` covers what the Metal backend does to this workload and `gpu-corruption.md` the defect that used to bound it, fixed upstream and retired on 2026-09-08; `zkboo-1.2.0-migration.md` with `zkboo-next-delta.md` record the dependency re-architecture and `zkboo-1.3.0-migration.md` and `zkboo-1.3.1-migration.md` the releases since; `instructions-are-not-seconds.md` is why a saving in one may not be quoted as a saving in the other.
- `fixtures/` — not tracked; created by the drivers for run logs and cached leaf proofs (`fixtures/leaves/n<N>/` and `fixtures/elephant-leaves/n<N>/`).
  The distilled logs the notes cite were removed from the repository; each citation names the file as it stood at commit `35ed318`.

## Licence

Copyright © 2026 Stefano Gogioso.
The code is LGPL-3.0-or-later (see `LICENSE`); vendored third-party code keeps its own licence.

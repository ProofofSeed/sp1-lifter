# sp1-lifter

A succinct lifter for ZKBoo proofs of seed with classical (pre-quantum) security, built to be cheap to verify on Ethereum L1 today: the genuine `zkboo` Rust verifier runs as an SP1 zkVM program, and the verification run is wrapped (fully locally, via SP1's native gnark Groth16 layer — no Docker, no external service) into a single BN254 pairing proof checkable on-chain for a few hundred thousand gas.
This trades the post-quantum guarantees of the companion STARK lifter for immediate deployability; the secure-element story (stage 1) is unchanged.
See `knowledge/milestones.md` for what is established so far, including the recorded stack history (an initial RISC Zero implementation, superseded the same day for measured reasons).
The repository is named for its proof system because other back ends are expected alongside it, not because SP1 is the only way to lift these proofs.

The zkboo ecosystem was re-architected as 1.2.0 on 2026-09-04, changing every proof, every circuit fingerprint and every guest verifying key, with a second breaking change on 2026-09-06. The port is complete and the stack has been rebuilt, re-proved and re-measured: the current figures are in `knowledge/performance-table.md` under "Re-measured on zkboo 1.2.0", and `knowledge/zkboo-1.2.0-migration.md` with `knowledge/zkboo-next-delta.md` record what changed and what it cost. Figures elsewhere in `knowledge/` belong to the previous round and say so; none of them may be rescaled into a current one, for a reason this project measured rather than assumed (`knowledge/instructions-are-not-seconds.md`).

## Usage

Requires the SP1 toolchain (`sp1up`; sdk 6.4.0 with the `succinct` rust toolchain) and Go, for the native gnark wrap.
The ZKBoo crates come from local checkouts under `<zkboo>`, patched in at the workspace root, because 1.2.0 is unpublished and every published version is yanked. The build script refuses anything that does not resolve at exactly that version from a path, and records each checkout's git revision and dirty state into the binary. This inverts the arrangement that held while the crates were published, and should be inverted back when they are published again.

Proving runs on the GPU. Build with `--features metal` and everything below goes through the Metal backend for SP1 Hypercube (`knowledge/gpu-proving.md`).
To measure against the CPU, force the GPU path off with the switches in `fixtures/cpu-arm-switches.txt` rather than by rebuilding, so that both arms are one binary.

⚠️ **Set `SP1_METAL_MERKLE=0` for any run you are not watching.** With every phase accelerated the backend is 4.7× faster than this machine's CPU prover on a one-response lift — and it also produces a proof that fails its own verification about once in twenty-eight and then parks the run for ever. Leaving the Merkle tree on the CPU has run 115 consecutive proofs clean, at 1.9× on a single leaf and 2.3× on a full-scale lift. See `knowledge/gpu-corruption.md`.

```sh
cargo build --release -p sp1-lifter-host --features metal   # once; the binaries below are in target/release

./target/release/cycle-probe                # instruction-law sweep
./target/release/s1-acceptance              # reject matrix + full-scale 219
./target/release/lift 219                   # prove + wrap + Foundry fixture
./target/release/lift 8 compressed          # throughput probe, no wrap
./target/release/split-probe 4 both         # split-and-aggregate probe
./target/release/elephant-probe             # the secp256k1 statement's cost
./target/release/elephant-split 2 both      # the split, on the secp256k1 statement
./target/release/stream-lift 8              # leaves proved as responses arrive
./target/release/vkeys                      # print every key, check the pinned ones
(cd verifier && forge test)                 # on-chain verify + gas + rejects
```

The Foundry suite reads a fixture that `lift` generates; without one it **skips**, and a fully skipped suite still prints `Suite result: ok`, so check for `5 passed` rather than `ok`.
The fixture in `verifier/fixtures/` was regenerated from a full-soundness run on 2026-09-07, and the three pins that describe it — verifying key, circuit fingerprint and proof-format identifier — were re-anchored in the same change. That matters more than it sounds: a fixture is internally consistent whatever produced it, so a stale one passes the suite silently, and a pin taken from an artefact only fails on the run *after* the artefact is replaced.

`lift` performs one proving pass: in the default `groth16` mode the single SP1 call runs core → compress → shrink → wrap, so the on-chain fixture costs one pass rather than two.
`compressed` mode is for throughput probes at small repetition counts; it is not a stage to run before `groth16`.

Proving cost is tunable from the environment without a rebuild — `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD` and `TRACE_CHUNK_SLOTS` are read by `SP1CoreOpts::default()`, and `RAYON_NUM_THREADS` caps parallelism.
`lift` prints the configuration it used before it starts, because a proving time means nothing without it.

## Structure

- `statement/` — shared host/program definitions: the proof-of-seed circuits, the fixed Keccak-256 crypto suite, the postcard wire format, and the reference journal gate.
- `program/` — the SP1 program: a thin shim around `zkboo::verifier::verify`.
- `elephant/`, `program-elephant/` — the secp256k1 Ethereum-address statement and its guest, kept separate so that probing it cannot move the production verifying key. See `knowledge/elephant-cost.md`.
- `host/` — proof generation, execution/proving drivers, and the acceptance harnesses.
- `verifier/` — Foundry suite over the vendored audited SP1 Groth16 verifier (sp1-contracts v6.1.0).
- `split/`, `program-leaf/`, `program-root/` — the split-and-aggregate probe: one proof per response plus a root carrying the cross-leaf global Fiat-Shamir gate, which is what makes the lift distributable and streamable. See `knowledge/split-and-streaming.md`.
- `program-leaf-elephant/`, `program-root-elephant/` — the same split over the secp256k1 Ethereum-address statement, which has no public parameters and a different journal shape, so it needs its own pair of guests. Driven by `elephant-split`.
- `pinned-keys/` — the leaf verifying keys, compiled into the roots so that a root's own key says which leaf program it will admit. 🚨 **Run `vkeys` after editing any crate a guest links — a comment-only change has been measured moving a guest key** (`knowledge/guest-keys-move-on-comments.md`). **It has no dependencies on purpose:** a crate the leaf programs link would move their keys whenever it changed, and the pin would never converge. `vkeys` re-derives every key from the built ELFs and fails if a pinned constant has drifted.
- `host/src/orchestrator.rs` — the streaming orchestrator: a leaf proof is dispatched as each response arrives rather than after the last one, so leaf proving overlaps response generation. Driven by `stream-lift`; see `knowledge/streaming-orchestrator.md`.
- `knowledge/` — design record, milestone plan, and measured numbers. `performance-table.md` is the current cost of everything and the place to start; `gpu-proving.md` covers what the Metal backend does to this workload and `gpu-corruption.md` the defect that used to bound it, fixed upstream and retired on 2026-09-08; `zkboo-1.2.0-migration.md` with `zkboo-next-delta.md` record the dependency re-architecture; `instructions-are-not-seconds.md` is why a saving in one may not be quoted as a saving in the other.

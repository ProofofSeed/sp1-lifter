# snark-lifter

A succinct lifter for ZKBoo proofs of seed with classical (pre-quantum) security, built to be cheap to verify on Ethereum L1 today: the genuine `zkboo` Rust verifier runs as an SP1 zkVM program, and the verification run is wrapped (fully locally, via SP1's native gnark Groth16 layer — no Docker, no external service) into a single BN254 pairing proof checkable on-chain for a few hundred thousand gas.
This trades the post-quantum guarantees of the companion STARK lifter for immediate deployability; the secure-element story (stage 1) is unchanged.
See `knowledge/milestones.md` for what is established so far, including the recorded stack history (an initial RISC Zero implementation, superseded the same day for measured reasons).

## Usage

Requires the SP1 toolchain (`sp1up`; sdk 6.x with the `succinct` rust toolchain), Go (for the native gnark wrap), and the local zkboo working trees at `../../zkboo` (see the `[patch.crates-io]` table in `Cargo.toml`).

```sh
cargo run --release -p snark-lifter-host --bin cycle-probe           # instruction-law sweep
cargo run --release -p snark-lifter-host --bin s1-acceptance         # reject matrix + full-scale 219
cargo run --release -p snark-lifter-host --bin lift -- 219           # prove + wrap + Foundry fixture
cargo run --release -p snark-lifter-host --bin lift -- 8 compressed  # throughput probe, no wrap
(cd verifier && forge test)                                          # on-chain verify + gas + rejects
```

The Foundry suite reads a fixture that `lift` generates; without one it **skips**, and a fully skipped suite still prints `Suite result: ok`, so check for `5 passed` rather than `ok`.

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
- `knowledge/` — design record, milestone plan, and measured numbers.

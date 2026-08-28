# snark-lifter

A succinct lifter for ZKBoo proofs of seed with classical (pre-quantum) security, built to be cheap to verify on Ethereum L1 today: the genuine `zkboo` Rust verifier runs as an SP1 zkVM program, and the verification run is wrapped (fully locally, via SP1's native gnark Groth16 layer — no Docker, no external service) into a single BN254 pairing proof checkable on-chain for a few hundred thousand gas.
This trades the post-quantum guarantees of the companion STARK lifter for immediate deployability; the secure-element story (stage 1) is unchanged.
See `knowledge/milestones.md` for what is established so far, including the recorded stack history (an initial RISC Zero implementation, superseded the same day for measured reasons).

## Usage

Requires the SP1 toolchain (`sp1up`; sdk 6.4.0 with the `succinct` rust toolchain) and Go, for the native gnark wrap.
The ZKBoo crates come from crates.io at the exact version the build script demands, so no local working tree is needed and none is used; a build that resolves them any other way is refused.

Proving runs on the GPU. Build with `--features metal` and everything below goes through the Metal backend for SP1 Hypercube (`knowledge/gpu-proving.md`).
To measure against the CPU, force the GPU path off with the switches in `fixtures/cpu-arm-switches.txt` rather than by rebuilding, so that both arms are one binary.

⚠️ **Set `SP1_METAL_MERKLE=0` for any run you are not watching.** With every phase accelerated the backend is 4.7× faster than this machine's CPU prover, and it also produces a proof that fails its own verification about once in twenty and then parks the run for ever. Leaving the Merkle tree on the CPU has run 115 consecutive proofs clean at 1.9×. See `knowledge/gpu-corruption.md`.

```sh
cargo build --release -p snark-lifter-host --features metal   # once; the binaries below are in target/release

./target/release/cycle-probe                # instruction-law sweep
./target/release/s1-acceptance              # reject matrix + full-scale 219
./target/release/lift 219                   # prove + wrap + Foundry fixture
./target/release/lift 8 compressed          # throughput probe, no wrap
./target/release/split-probe 4 both         # split-and-aggregate probe
./target/release/elephant-probe             # the secp256k1 statement's cost
(cd verifier && forge test)                 # on-chain verify + gas + rejects
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
- `split/`, `program-leaf/`, `program-root/` — the split-and-aggregate probe: one proof per response plus a root carrying the cross-leaf global Fiat-Shamir gate, which is what makes the lift distributable and streamable. See `knowledge/split-and-streaming.md`.
- `knowledge/` — design record, milestone plan, and measured numbers, including `gpu-proving.md` for what the Metal backend does to this workload and `gpu-corruption.md` for the defect that bounds it.

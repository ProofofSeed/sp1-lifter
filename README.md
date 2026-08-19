# snark-lifter

A succinct lifter for ZKBoo proofs of seed with classical (pre-quantum) security, built to be cheap to verify on Ethereum L1 today: the genuine `zkboo` Rust verifier runs as a RISC Zero zkVM guest, and the zkVM's universal Groth16 wrapper turns the verification run into a single BN254 pairing proof checkable on-chain for a few hundred thousand gas.
This trades the post-quantum guarantees of the companion STARK lifter for immediate deployability; the secure-element story (stage 1) is unchanged.

## Usage

Requires the RISC Zero toolchain (`rzup install`, pinned at cargo-risczero 3.0.6) and the local zkboo working trees at `../../zkboo` (see the `[patch.crates-io]` table in `Cargo.toml`).

```sh
cargo run --release -p snark-lifter-host --bin cycle-probe          # sweep 1 2 4 8
cargo run --release -p snark-lifter-host --bin cycle-probe -- 16 32 # custom sweep
```

## Structure

- `statement/` — shared host/guest definitions: the proof-of-seed circuits, the fixed Keccak-256 crypto suite, and the postcard wire format.
- `methods/` — the embedded zkVM guest (`methods/guest/`, its own cargo workspace) and its generated ELF/image id.
- `host/` — proof generation and zkVM drivers; currently the cycle probe.
- `knowledge/` — design record and milestone plan.

// SPDX-License-Identifier: LGPL-3.0-or-later

//! Compressed (succinct) prover for the lift program, on SP1.
//!
//! Generates a genuine hardened-child proof of seed at the given repetition count,
//! proves the lift program with SP1's compressed proof mode (the constant-size
//! succinct receipt), verifies it via the full relying-party gate (verifying key +
//! journal checks), and writes the proof to `fixtures/`.
//!
//! Usage: `s2-prove [n]` (default 219, the classical-128 count). Run small counts first
//! to measure throughput before committing to the full run.

use std::time::Instant;

use sp1_sdk::{
    ProvingKey,
    blocking::{ProveRequest, Prover, ProverClient},
};
use snark_lifter_host::{
    HARDENED_OFFSET, LIFT_ELF, bip32_vector_1, check_lifted_proof, expectation_for,
    lift_input_for, lift_stdin, vector_1_proof,
};
use zkboo::params::n_eps;

const SEED_ENTROPY: &[u8] = b"snark-lifter S1 acceptance entropy";
const BINDING: &[u8] = b"snark-lifter S1 acceptance tx binding";

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map(|a| a.parse().expect("repetition count must be a positive integer"))
        .unwrap_or_else(|| n_eps(64));
    assert!(n >= 1, "repetition count must be positive");

    println!("── compressed proving run (n={n}) ──");
    let t = Instant::now();
    let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
    println!("  proved {n} ZKBoo responses in {:.2?}", t.elapsed());
    let (_, chain_code) = bip32_vector_1();
    let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);

    let client = ProverClient::from_env();
    let pk = client.setup(LIFT_ELF).expect("program setup");
    let t = Instant::now();
    let lifted = client
        .prove(&pk, lift_stdin(&input))
        .compressed()
        .run()
        .expect("compressed proving");
    let prove_wall = t.elapsed();
    println!("  zkVM compressed proving: {prove_wall:.2?}");

    let t = Instant::now();
    let journal = check_lifted_proof(
        &client,
        &lifted,
        pk.verifying_key(),
        &expectation_for(&input, n as u32),
    )
    .expect("lifted-proof gate");
    println!("  proof verified + journal gate passed in {:.2?}", t.elapsed());
    assert_eq!(journal.num_responses, n as u32);
    println!("  lifted statement: {} responses", journal.num_responses);

    std::fs::create_dir_all("fixtures").expect("fixtures dir");
    let path = format!("fixtures/lift-{n}.compressed-proof.bin");
    lifted.save(&path).expect("save proof");
    let size = std::fs::metadata(&path).expect("stat").len();
    println!("  compressed proof: {size} bytes → {path}");
    println!("compressed proving run: PASS");
}

// SPDX-License-Identifier: LGPL-3.0-or-later

//! Groth16 wrap for the lifted proof, on SP1 — fully local (`native-gnark`, no Docker).
//!
//! Runs the whole pipeline (core → compress → shrink → Groth16 wrap) for the lift
//! program on a genuine proof of seed, verifies the wrapped proof via the full
//! relying-party gate, and exports the Foundry fixture consumed by `verifier/`:
//! the verifying-key hash, the public values (postcard-encoded journal), and the
//! EVM proof bytes.
//!
//! Usage: `s3-wrap [n]` (default 219).

use std::time::Instant;

use sp1_sdk::{
    HashableKey, ProvingKey,
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

    println!("── Groth16 wrap run (n={n}, native gnark) ──");
    let t = Instant::now();
    let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
    println!("  proved {n} ZKBoo responses in {:.2?}", t.elapsed());
    let (_, chain_code) = bip32_vector_1();
    let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);

    let client = ProverClient::from_env();
    let pk = client.setup(LIFT_ELF).expect("program setup");
    let vk = pk.verifying_key();
    let t = Instant::now();
    let lifted = client
        .prove(&pk, lift_stdin(&input))
        .groth16()
        .run()
        .expect("Groth16 proving (core → compress → shrink → native wrap)");
    let prove_wall = t.elapsed();
    println!("  zkVM Groth16 pipeline: {prove_wall:.2?}");

    let journal = check_lifted_proof(&client, &lifted, vk, &expectation_for(&input, n as u32))
        .expect("wrapped-proof gate");
    assert_eq!(journal.num_responses, n as u32);
    println!("  proof verified + journal gate passed ({} responses)", journal.num_responses);

    std::fs::create_dir_all("fixtures").expect("fixtures dir");
    let bin_path = format!("fixtures/lift-{n}.groth16-proof.bin");
    lifted.save(&bin_path).expect("save proof");
    println!("  Groth16 proof → {bin_path}");

    let proof_bytes = lifted.bytes();
    let public_values = lifted.public_values.as_slice();
    let json = format!(
        "{{\n  \"vkey\": \"{}\",\n  \"publicValues\": \"0x{}\",\n  \"proof\": \"0x{}\"\n}}\n",
        vk.bytes32(),
        hex::encode(public_values),
        hex::encode(&proof_bytes),
    );
    std::fs::create_dir_all("verifier/fixtures").expect("fixtures dir");
    let json_path = format!("verifier/fixtures/lift-{n}.onchain.json");
    std::fs::write(&json_path, json).expect("write json");
    println!(
        "  Foundry fixture → {json_path} (proof {} bytes, public values {} bytes)",
        proof_bytes.len(),
        public_values.len(),
    );
}

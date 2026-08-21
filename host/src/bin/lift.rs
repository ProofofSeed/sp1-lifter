// SPDX-License-Identifier: LGPL-3.0-or-later

//! The lifter's proving driver: one statement, one proving pass, both artefacts.
//!
//! Generates a genuine hardened-child proof of seed at the requested repetition count,
//! proves the lift program in SP1, puts the result through the full relying-party gate
//! (verifying key + journal checks) and writes what the run produced. In `groth16` mode
//! the single SP1 call runs core → compress → shrink → wrap, so the on-chain fixture
//! costs exactly one proving pass.
//!
//! ⚠️ Proving the same statement twice — once compressed, once wrapped — pays for core
//! proving and compression twice, and those two stages are essentially the entire cost
//! (the wrap adds ~171 s to a ~6.4 h run). The first full-scale chain did exactly that
//! and took 12 h 48 m for 6 h 26 m of work. `compressed` mode exists for throughput
//! probes at small repetition counts, not as a stage to run before `groth16`.
//!
//! Usage: `lift [n] [compressed|groth16]` (default: 219 responses, groth16).

use std::time::Instant;

use sp1_sdk::{HashableKey, ProvingKey, blocking::{ProveRequest, Prover}};
use snark_lifter_host::{
    HARDENED_OFFSET, LIFT_ELF, bip32_vector_1, check_lifted_proof, expectation_for,
    lift_input_for, lift_stdin, prover_client, vector_1_proof,
};
use zkboo::params::n_eps;

const SEED_ENTROPY: &[u8] = b"snark-lifter S1 acceptance entropy";
const BINDING: &[u8] = b"snark-lifter S1 acceptance tx binding";

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args
        .next()
        .map(|a| a.parse().expect("repetition count must be a positive integer"))
        .unwrap_or_else(|| n_eps(64));
    assert!(n >= 1, "repetition count must be positive");
    let mode = args.next().unwrap_or_else(|| "groth16".to_string());
    assert!(
        mode == "groth16" || mode == "compressed",
        "mode must be `groth16` or `compressed`, got `{mode}`"
    );
    // SP1's logger installs an `EnvFilter` that falls back to "off" when RUST_LOG is
    // unset, so the default would be a multi-hour proving run that leaves no phase record
    // of itself. Default it on, before any thread exists to observe the change.
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    sp1_sdk::setup_logger();
    let (client, config) = prover_client();
    println!("── lift run (n={n}, mode={mode}) ──");
    println!("  prover config: {config}");
    let t = Instant::now();
    let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
    println!("  proved {n} ZKBoo responses natively in {:.2?}", t.elapsed());
    let (_, chain_code) = bip32_vector_1();
    let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);
    let pk = client.setup(LIFT_ELF).expect("program setup");
    let t = Instant::now();
    let request = client.prove(&pk, lift_stdin(&input));
    let lifted = if mode == "groth16" {
        request.groth16().run()
    } else {
        request.compressed().run()
    }
    .expect("zkVM proving");
    let prove_wall = t.elapsed();
    println!("  zkVM {mode} proving: {prove_wall:.2?}");
    let t = Instant::now();
    let journal = check_lifted_proof(
        &client,
        &lifted,
        pk.verifying_key(),
        &expectation_for(&input, n as u32),
    )
    .expect("lifted-proof gate");
    assert_eq!(journal.num_responses, n as u32);
    println!(
        "  proof verified + journal gate passed in {:.2?} ({} responses)",
        t.elapsed(),
        journal.num_responses
    );
    std::fs::create_dir_all("fixtures").expect("fixtures dir");
    let suffix = if mode == "groth16" { "groth16-proof" } else { "compressed-proof" };
    let bin_path = format!("fixtures/lift-{n}.{suffix}.bin");
    lifted.save(&bin_path).expect("save proof");
    println!("  proof → {bin_path} ({} bytes on disk)", std::fs::metadata(&bin_path).expect("stat").len());
    if mode == "groth16" {
        let proof_bytes = lifted.bytes();
        let public_values = lifted.public_values.as_slice();
        let json = format!(
            "{{\n  \"vkey\": \"{}\",\n  \"publicValues\": \"0x{}\",\n  \"proof\": \"0x{}\"\n}}\n",
            pk.verifying_key().bytes32(),
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
    println!("lift run: PASS");
}

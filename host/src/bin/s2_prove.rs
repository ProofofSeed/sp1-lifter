// SPDX-License-Identifier: LGPL-3.0-or-later

//! Succinct-receipt prover for the lift guest.
//!
//! Generates a genuine hardened-child proof of seed at the given repetition count, runs
//! the lift guest under the real prover (`r0vm`, Metal-accelerated on this machine) with
//! succinct receipt options, verifies the receipt against the lift image id, passes the
//! journal through the reference consumer gate, and writes the receipt to `fixtures/`
//! for the Groth16 wrap stage.
//!
//! Usage: `s2-prove [n]` (default 219, the classical-128 count). Run small counts first
//! to measure throughput before committing to the full run.

use std::time::Instant;

use risc0_zkvm::{ProverOpts, default_prover};
use snark_lifter_host::{bip32_vector_1, expectation_for, guest_env, lift_input_for, vector_1_proof};
use snark_lifter_methods::{LIFT_ELF, LIFT_ID};
use snark_lifter_statement::{LiftJournal, check_journal};
use zkboo::params::n_eps;
use zkboo_bip32::HARDENED_OFFSET;

const SEED_ENTROPY: &[u8] = b"snark-lifter S1 acceptance entropy";
const BINDING: &[u8] = b"snark-lifter S1 acceptance tx binding";

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map(|a| a.parse().expect("repetition count must be a positive integer"))
        .unwrap_or_else(|| n_eps(64));
    assert!(n >= 1, "repetition count must be positive");

    println!("── succinct proving run (n={n}) ──");
    let t = Instant::now();
    let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
    println!("  proved {n} ZKBoo responses in {:.2?}", t.elapsed());
    let (_, chain_code) = bip32_vector_1();
    let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);

    let env = guest_env(&input).expect("guest env");
    let opts = ProverOpts::succinct();
    let t = Instant::now();
    let info = default_prover()
        .prove_with_opts(env, LIFT_ELF, &opts)
        .expect("succinct proving");
    let prove_wall = t.elapsed();
    let stats = &info.stats;
    println!(
        "  zkVM proving: {prove_wall:.2?}  ({} segments, {} user cycles, {} total cycles)",
        stats.segments, stats.user_cycles, stats.total_cycles,
    );

    let receipt = info.receipt;
    let t = Instant::now();
    receipt.verify(LIFT_ID).expect("receipt verification");
    println!("  receipt verified against the lift image id in {:.2?}", t.elapsed());
    let journal: LiftJournal = receipt.journal.decode().expect("journal decode");
    check_journal(&journal, &expectation_for(&input, n as u32)).expect("consumer gate");
    assert_eq!(journal.num_responses, n as u32);
    println!("  journal passes the consumer gate ({} responses)", journal.num_responses);

    let bytes = bincode::serialize(&receipt).expect("serialize receipt");
    std::fs::create_dir_all("fixtures").expect("fixtures dir");
    let path = format!("fixtures/lift-{n}.succinct-receipt.bin");
    std::fs::write(&path, &bytes).expect("write receipt");
    println!("  succinct receipt: {} bytes → {path}", bytes.len());
    println!("succinct proving run: PASS");
}

// SPDX-License-Identifier: LGPL-3.0-or-later

//! Cycle probe for the SNARK-lifter program.
//!
//! Generates genuine BIP-32 hardened-child proofs of seed at a sweep of repetition
//! counts, verifies each natively as a sanity check, then executes the lift program on
//! each in the SP1 executor (no proving) and reports the executed instruction counts.
//! A least-squares linear fit over the sweep extrapolates the full-scale cost at the
//! classical-128 (219) and PQ-128 (438) repetition counts.
//!
//! Unlike the earlier in-guest cycle bracket, SP1 reports whole-program counts, so the
//! law includes frame decode and the fingerprint pass (a few percent at full scale).
//!
//! Usage: `cycle-probe [iters...]` (default sweep: 1 2 4 8).

use std::time::Instant;

use sp1_sdk::blocking::ProverClient;
use snark_lifter_host::{
    HARDENED_OFFSET, WPP, bip32_vector_1, execute_lift, lift_input_for, vector_1_proof,
};
use snark_lifter_statement::{H, PV, S};
use zkboo::{params::n_eps, verifier::verify};

const SEED_ENTROPY: &[u8] = b"snark-lifter cycle probe entropy";
const BINDING: &[u8] = b"";

fn main() {
    println!("  zkboo: {}", snark_lifter_host::zkboo_provenance());
    let mut iters: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("iteration counts must be positive integers"))
        .collect();
    assert!(
        iters.iter().all(|&n| n >= 1),
        "iteration counts must be positive (zkboo accepts an empty proof as valid, \
         so an n=0 point would be meaningless)"
    );
    if iters.is_empty() {
        iters = vec![1, 2, 4, 8];
    }

    println!("statement: BIP-32 hardened child (test vector 1, m/0H), Keccak-256 suite");
    let client = ProverClient::from_env();

    let mut points: Vec<(f64, f64)> = Vec::new();
    for &n in &iters {
        let t = Instant::now();
        let (circuit, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        let prove_time = t.elapsed();
        let ok =
            verify::<_, H, PV, S, WPP>(&circuit, &output, &proof, BINDING).expect("native verify");
        assert!(ok, "native verification failed at {n} iters");
        let (_, chain_code) = bip32_vector_1();
        let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);
        let t = Instant::now();
        let (journal, report) = execute_lift(&client, &input).expect("program execution");
        let wall = t.elapsed();
        assert!(journal.valid, "program verification failed at {n} iters");
        assert_eq!(journal.num_responses as usize, n, "response count mismatch");
        let cycles = report.total_instruction_count();
        println!(
            "n={n:>3}  prove {prove_time:>8.2?}  exec {wall:>8.2?}  {cycles:>12} instructions"
        );
        points.push((n as f64, cycles as f64));
    }

    if points.len() >= 2 {
        let k = points.len() as f64;
        let sx: f64 = points.iter().map(|p| p.0).sum();
        let sy: f64 = points.iter().map(|p| p.1).sum();
        let sxx: f64 = points.iter().map(|p| p.0 * p.0).sum();
        let sxy: f64 = points.iter().map(|p| p.0 * p.1).sum();
        let slope = (k * sxy - sx * sy) / (k * sxx - sx * sx);
        let intercept = (sy - slope * sx) / k;
        println!();
        println!("fitted law: instructions ≈ {intercept:.3e} + {slope:.3e} × n_responses");
        for (label, target) in [("classical-128", n_eps(64)), ("PQ-128", n_eps(128))] {
            let cycles = intercept + slope * target as f64;
            println!("  extrapolated @ {label} ({target} responses): {cycles:.3e}");
        }
    }
}

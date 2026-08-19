// SPDX-License-Identifier: LGPL-3.0-or-later

//! Cycle probe for the SNARK-lifter guest.
//!
//! Generates genuine BIP-32 hardened-child proofs of seed at a sweep of repetition
//! counts, verifies each natively as a sanity check, then runs the probe guest on each
//! inside the RISC Zero executor (execution only, no proving) and reports the measured
//! guest cycles. A least-squares linear fit over the sweep extrapolates the full-scale
//! verification cost at the classical-128 (219) and PQ-128 (438) repetition counts.
//!
//! Usage: `cycle-probe [iters...]` (default sweep: 1 2 4 8).

use std::time::Instant;

use snark_lifter_host::{HARDENED_OFFSET, WPP, bip32_vector_1, run_guest, vector_1_proof};
use snark_lifter_methods::PROBE_ELF;
use snark_lifter_statement::{H, LIFT_FRAME_VERSION, LiftInput, PV, ProbeJournal, S};
use zkboo::{params::n_eps, verifier::verify};

const SEED_ENTROPY: &[u8] = b"snark-lifter cycle probe entropy";
const BINDING: &[u8] = b"";

fn main() {
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
    println!("guest ELF: {} bytes", PROBE_ELF.len());
    println!();

    let mut points: Vec<(f64, f64)> = Vec::new();
    for &n in &iters {
        let t = Instant::now();
        let (circuit, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        let prove_time = t.elapsed();
        let ok =
            verify::<_, H, PV, S, WPP>(&circuit, &output, &proof, BINDING).expect("native verify");
        assert!(ok, "native verification failed at {n} iters");
        let (_, chain_code) = bip32_vector_1();
        let input = LiftInput {
            version: LIFT_FRAME_VERSION,
            chain_code,
            index: HARDENED_OFFSET,
            expected_output: output,
            binding: BINDING.to_vec(),
            proof,
        };
        let t = Instant::now();
        let session = run_guest(PROBE_ELF, &input).expect("guest execution");
        let wall = t.elapsed();
        let journal: ProbeJournal = session.journal.decode().expect("journal decode");
        assert!(journal.valid, "guest verification failed at {n} iters");
        assert_eq!(journal.num_responses as usize, n, "response count mismatch");
        let total_cycles: u64 = session.segments.iter().map(|s| s.cycles as u64).sum();
        println!(
            "n={n:>3}  prove {prove_time:>8.2?}  exec {wall:>8.2?}  \
             verify {:>12} cycles  total {:>12} cycles",
            journal.verify_cycles, total_cycles,
        );
        points.push((n as f64, journal.verify_cycles as f64));
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
        println!("fitted law: verify_cycles ≈ {intercept:.3e} + {slope:.3e} × n_responses");
        for (label, target) in [("classical-128", n_eps(64)), ("PQ-128", n_eps(128))] {
            let cycles = intercept + slope * target as f64;
            println!("  extrapolated @ {label} ({target} responses): {cycles:.3e} cycles");
        }
    }
}

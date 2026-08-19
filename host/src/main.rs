// SPDX-License-Identifier: LGPL-3.0-or-later

//! Cycle probe for the SNARK-lifter guest.
//!
//! Generates genuine BIP-32 hardened-child proofs of seed at a sweep of repetition
//! counts, verifies each natively as a sanity check, then runs the zkVM guest verifier
//! on each inside the RISC Zero executor (execution only, no proving) and reports the
//! measured guest cycles. A least-squares linear fit over the sweep extrapolates the
//! full-scale verification cost at the classical-128 (219) and PQ-128 (438) repetition
//! counts.
//!
//! Usage: `cycle-probe [iters...]` (default sweep: 1 2 4 8).

use std::time::Instant;

use risc0_zkvm::{ExecutorEnv, default_executor};
use snark_lifter_methods::SNARK_LIFTER_GUEST_ELF;
use snark_lifter_statement::{H, HardenedChildCircuit, LiftInput, PS, PV, ProbeJournal, S};
use zkboo::{
    executor::{OwnedFlexibleWordPool, exec},
    params::n_eps,
    prover::{prove, views::OwnedFlexibleWordTriplePool},
    verifier::{replay::OwnedFlexibleWordPairPool, verify},
};
use zkboo_bip32::HARDENED_OFFSET;

type WP = OwnedFlexibleWordPool<usize>;
type WTP = OwnedFlexibleWordTriplePool<usize>;
type WPP = OwnedFlexibleWordPairPool<usize>;

const SEED_ENTROPY: &[u8] = b"snark-lifter cycle probe entropy";
const BINDING: &[u8] = b"";

fn from_hex(s: &str) -> Vec<u8> {
    return (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
}

fn main() {
    let mut iters: Vec<usize> = std::env::args()
        .skip(1)
        .map(|a| a.parse().expect("iteration counts must be positive integers"))
        .collect();
    if iters.is_empty() {
        iters = vec![1, 2, 4, 8];
    }

    // BIP-32 test vector 1, chain m -> m/0H (a public test vector, not a secret).
    let parent_priv = from_hex("e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35");
    let chain_code = from_hex("873dff81c02f525623fd1fe5167eac3a55a049de3d314bb42ee227ffed37d508");

    println!("statement: BIP-32 hardened child (test vector 1, m/0H), Keccak-256 suite");
    println!("guest ELF: {} bytes", SNARK_LIFTER_GUEST_ELF.len());
    println!();

    let mut points: Vec<(f64, f64)> = Vec::new();
    for &n in &iters {
        let circuit = HardenedChildCircuit::with_witness(
            chain_code.clone(),
            parent_priv.clone(),
            HARDENED_OFFSET,
        );
        let output = exec::<_, WP>(&circuit);
        let t = Instant::now();
        let proof = prove::<_, H, PS, PV, S, WTP>(&circuit, n, SEED_ENTROPY, BINDING);
        let prove_time = t.elapsed();
        let ok =
            verify::<_, H, PV, S, WPP>(&circuit, &output, &proof, BINDING).expect("native verify");
        assert!(ok, "native verification failed at {n} iters");
        let input = LiftInput {
            chain_code: chain_code.clone(),
            index: HARDENED_OFFSET,
            expected_output: output,
            binding: BINDING.to_vec(),
            proof,
        };
        let frame = postcard::to_allocvec(&input).expect("serialize input frame");
        // Length-prefixed frame, packed into u32 words (little-endian) for `write_slice`.
        let mut padded = frame.clone();
        padded.resize(frame.len().div_ceil(4) * 4, 0);
        let words: Vec<u32> = padded
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        let env = ExecutorEnv::builder()
            .write(&(frame.len() as u32))
            .expect("write frame length")
            .write_slice(&words)
            .build()
            .expect("executor env");
        let t = Instant::now();
        let session = default_executor()
            .execute(env, SNARK_LIFTER_GUEST_ELF)
            .expect("guest execution");
        let wall = t.elapsed();
        let journal: ProbeJournal = session.journal.decode().expect("journal decode");
        assert!(journal.valid, "guest verification failed at {n} iters");
        assert_eq!(journal.num_responses as usize, n, "response count mismatch");
        let total_cycles: u64 = session.segments.iter().map(|s| s.cycles as u64).sum();
        println!(
            "n={n:>3}  frame {:>9} B  prove {prove_time:>8.2?}  exec {wall:>8.2?}  \
             verify {:>12} cycles  total {:>12} cycles",
            frame.len(),
            journal.verify_cycles,
            total_cycles,
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
        println!(
            "fitted law: verify_cycles ≈ {intercept:.3e} + {slope:.3e} × n_responses"
        );
        for (label, target) in [("classical-128", n_eps(64)), ("PQ-128", n_eps(128))] {
            let cycles = intercept + slope * target as f64;
            println!("  extrapolated @ {label} ({target} responses): {cycles:.3e} cycles");
        }
    }
}

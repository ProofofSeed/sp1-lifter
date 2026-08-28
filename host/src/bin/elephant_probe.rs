// SPDX-License-Identifier: LGPL-3.0-or-later

//! What it would cost to lift the statement that actually binds a seed to an Ethereum
//! address.
//!
//! The proof-of-seed suite splits sharply in two. The hash-only statements (master key,
//! hardened child, BIP-39 seed) cost tens of kilobytes per response. The statements that
//! touch secp256k1 — public key, normal child, and the Ethereum address that binds an
//! account — cost megabytes per response, because an in-circuit scalar multiplication
//! dwarfs a few HMAC-SHA512 compressions. Everything the lifter proves at full scale is on
//! the cheap side of that split.
//!
//! This probe measures the expensive side and reports what it implies at full scale, rather
//! than committing months of machine time to find out.
//!
//! **It sweeps rather than taking one point, and that is the whole design.** The cost is
//! `fixed + marginal × n`, the fixed part is the in-guest circuit-fingerprint pass over a
//! secp256k1 circuit plus the frame decode, and at one response that fixed part was two
//! thirds of the total — so a single point overstates the marginal cost, and hence full
//! scale, by nearly threefold. Two points give the marginal cost; three say whether the law
//! is linear.
//!
//! Usage: `elephant-probe [n...]` (default `1 2 4`). Response sizes here are megabytes;
//! raising n multiplies both memory and time.

use std::time::Instant;

use sp1_sdk::blocking::{Prover, ProverClient};
use snark_lifter_elephant::{ELEPHANT_FRAME_VERSION, ElephantInput, EthereumAddressCircuit};
use snark_lifter_host::{ELEPHANT_ELF, WP, WTP};
use snark_lifter_statement::{H, PS, PV, S};
use zkboo::{executor::exec, params::n_eps, prover::par_prove, verifier::verify};
use zkboo::verifier::replay::OwnedFlexibleWordPairPool;

type WPP = OwnedFlexibleWordPairPool<usize>;

const SEED_ENTROPY: &[u8] = b"snark-lifter elephant probe entropy";
const BINDING: &[u8] = b"snark-lifter elephant probe binding";

/// A private key with no special structure, used only as a witness.
const PRIV_KEY: [u8; 32] = [
    0x18, 0xe1, 0x4a, 0x7b, 0x6a, 0x30, 0x7f, 0x42, 0x6a, 0x94, 0xf8, 0x11, 0x47, 0x01, 0xe7, 0xc8,
    0xe7, 0x74, 0xe7, 0xf9, 0xa4, 0x7e, 0x2c, 0x20, 0x35, 0xdb, 0x29, 0xa2, 0x06, 0x32, 0x17, 0x25,
];

/// The hardened-child statement's measured full-scale figures, for comparison.
///
/// Whole-program instructions for a 219-response guest run, and the wall clock of the
/// full-scale proving run that produced the on-chain fixture. Both are measured on this
/// machine against zkboo 2.0.0; see `knowledge/iteration.md`. The proving *rate* implied by
/// their ratio is what the days-at-full-scale figure below rests on, and it is the
/// hardened-child statement's rate, assumed to carry to a workload with an entirely
/// different instruction mix.
const HASH_MARGINAL_INSTR: f64 = 21.66e6;
const HASH_FULL_INSTRUCTIONS: f64 = 4.775013465e9;
const HASH_FULL_SECONDS: f64 = 10_296.11;

/// One point of the sweep: everything measured at a given response count.
struct Point {
    n: usize,
    instructions: u64,
    frame_bytes: usize,
    prove: std::time::Duration,
    verify: std::time::Duration,
    guest_wall: std::time::Duration,
}

fn probe(n: usize) -> Point {
    let circuit = EthereumAddressCircuit::with_witness(PRIV_KEY.to_vec());
    let output = exec::<_, WP>(&circuit);
    let t = Instant::now();
    let proof = par_prove::<_, H, PS, PV, S, WTP>(&circuit, n, SEED_ENTROPY, BINDING);
    let prove = t.elapsed();
    let verify_circuit = EthereumAddressCircuit::public_only();
    let t = Instant::now();
    let ok = verify::<_, H, PV, S, WPP>(&verify_circuit, &output, &proof, BINDING).expect("verify");
    let verify_time = t.elapsed();
    assert!(ok, "native verification failed at n={n}");
    let input = ElephantInput {
        version: ELEPHANT_FRAME_VERSION,
        expected_output: output,
        binding: BINDING.to_vec(),
        proof,
    };
    let frame = postcard::to_allocvec(&input).expect("serialize frame");
    let client = ProverClient::from_env();
    let mut stdin = sp1_sdk::SP1Stdin::new();
    stdin.write_vec(frame.clone());
    let t = Instant::now();
    let (_, report) = client.execute(ELEPHANT_ELF, stdin).run().expect("guest execution");
    let guest_wall = t.elapsed();
    assert_eq!(report.exit_code, 0, "guest execution failed at n={n}");
    return Point {
        n,
        instructions: report.total_instruction_count(),
        frame_bytes: frame.len(),
        prove,
        verify: verify_time,
        guest_wall,
    };
}

fn main() {
    let counts: Vec<usize> = {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if args.is_empty() {
            vec![1, 2, 4]
        } else {
            args.iter().map(|a| a.parse().expect("response counts must be positive integers")).collect()
        }
    };
    assert!(counts.iter().all(|&n| n >= 1), "response counts must be positive");
    assert!(counts.len() >= 2, "a single point cannot separate the fixed cost from the marginal one; give at least two counts");
    println!("── elephant probe (Ethereum address = Keccak256(x_be ‖ y_be)[12:]) ──");
    println!("  zkboo: {}", snark_lifter_host::zkboo_provenance());

    let circuit = EthereumAddressCircuit::with_witness(PRIV_KEY.to_vec());
    let t = Instant::now();
    let prof = zkboo_profiling::profile(&circuit);
    let profile_time = t.elapsed();
    let predicted_response = prof.response_size::<S, S>();
    let and_msgs: usize = prof.and_msg_size().sum();
    println!("  circuit profile in {profile_time:.2?}");
    println!("    nonlinear AND-messages: {and_msgs}");
    println!("    predicted response size: {predicted_response} bytes ({:.3} MB)", predicted_response as f64 / 1e6);
    println!("    predicted proof size at {}: {:.3} GB", n_eps(64), prof.proof_size::<S, S>(n_eps(64)) as f64 / 1e9);
    let t = Instant::now();
    let output = exec::<_, WP>(&circuit);
    let exec_time = t.elapsed();
    let addr: Vec<String> = output.u8.iter().take(20).map(|b| format!("{b:02x}")).collect();
    println!("  executed in {exec_time:.2?} → address 0x{}", addr.join(""));
    println!();

    let mut points: Vec<Point> = Vec::new();
    for &n in &counts {
        let p = probe(n);
        println!(
            "  n={:<3} instructions={:>15}  frame={:>10} B ({:.3} MB/response)  prove={:.2?}  verify={:.2?}  guest={:.2?}",
            p.n, p.instructions, p.frame_bytes,
            p.frame_bytes as f64 / 1e6 / p.n as f64,
            p.prove, p.verify, p.guest_wall,
        );
        points.push(p);
    }
    println!();

    // Least squares over the sweep, which needs no more than two points but reports how well
    // three or more agree — a law quoted from a fit nobody checked is a guess with error bars.
    let k = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.n as f64).sum();
    let sy: f64 = points.iter().map(|p| p.instructions as f64).sum();
    let sxx: f64 = points.iter().map(|p| (p.n as f64).powi(2)).sum();
    let sxy: f64 = points.iter().map(|p| p.n as f64 * p.instructions as f64).sum();
    let marginal = (k * sxy - sx * sy) / (k * sxx - sx * sx);
    let fixed = (sy - marginal * sx) / k;
    println!("  ── fitted law ──");
    println!("    instructions = {fixed:.4e} + {marginal:.4e} × n");
    for p in &points {
        let predicted = fixed + marginal * p.n as f64;
        println!(
            "    n={:<3} measured {:>15}  fitted {:>15.0}  error {:+.4}%",
            p.n, p.instructions, predicted,
            (p.instructions as f64 - predicted) / p.instructions as f64 * 100.0,
        );
    }
    let single_point = points[0].instructions as f64 / points[0].n as f64;
    println!(
        "    fixed term is {:.0}% of the n={} run; taking that run's per-response figure as marginal \
         would overstate it by {:.2}x",
        fixed / points[0].instructions as f64 * 100.0,
        points[0].n,
        single_point / marginal,
    );
    println!();

    let n_full = n_eps(64);
    let full = fixed + marginal * n_full as f64;
    let rate = HASH_FULL_INSTRUCTIONS / HASH_FULL_SECONDS;
    println!("  ── extrapolation to a full classical-128 lift ({n_full} responses) ──");
    println!("    vs hardened child: {:.0}x the instructions per response", marginal / HASH_MARGINAL_INSTR);
    println!("    instructions at {n_full}: {full:.4e}");
    println!("    proof data through the guest: {:.2} GB", predicted_response as f64 * n_full as f64 / 1e9);
    println!("    at this machine's measured rate ({:.0}k instr/s): {:.1} days", rate / 1e3, full / rate / 86400.0);
    println!();
    println!("  ⚠️  The rate above is the hardened-child statement's, assumed to carry over. It has");
    println!("      not been measured on an elephant workload, and no elephant response has ever been");
    println!("      proved — only executed. Treat the days figure as an order of magnitude.");
}

// SPDX-License-Identifier: LGPL-3.0-or-later

//! What it would cost to lift the statement that actually binds a seed to an Ethereum
//! address.
//!
//! The proof-of-seed suite splits sharply in two. The hash-only statements (master key,
//! hardened child, BIP-39 seed) cost tens of kilobytes per response. The statements that
//! touch secp256k1 — public key, normal child, and the Ethereum address that binds an
//! account — cost megabytes per response, because an in-circuit scalar multiplication
//! dwarfs a few HMAC-SHA512 compressions. Everything the lifter has measured so far is on
//! the cheap side of that split.
//!
//! This probe measures the expensive side at one response and reports what the numbers
//! imply at full scale, rather than committing days of machine time to find out. It
//! reports, in order: the circuit's own profile, the real cost of producing and natively
//! verifying one response, and the zkVM instruction count for verifying it in-guest —
//! then extrapolates against the hardened-child statement's measured full-scale run.
//!
//! Usage: `elephant-probe [n]` (default 1). Response sizes here are megabytes; raising n
//! multiplies both memory and time.

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
const HASH_INSTR_PER_RESPONSE: f64 = 21.66e6;
const HASH_FULL_INSTRUCTIONS: f64 = 4.773338656e9;
const HASH_FULL_SECONDS: f64 = 23320.2;

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map(|a| a.parse().expect("response count must be a positive integer"))
        .unwrap_or(1);
    assert!(n >= 1, "response count must be positive");
    println!("── elephant probe (Ethereum address = Keccak256(x_be ‖ y_be)[12:], n={n}) ──");
    println!("  zkboo: {}", snark_lifter_host::zkboo_provenance());

    let circuit = EthereumAddressCircuit::with_witness(PRIV_KEY.to_vec());
    let t = Instant::now();
    let prof = zkboo_profiling::profile(&circuit);
    let profile_time = t.elapsed();
    let predicted_response = prof.response_size::<S, S>();
    println!("  circuit profile in {profile_time:.2?}");
    println!("    predicted response size: {} bytes ({:.2} MB)", predicted_response, predicted_response as f64 / 1e6);
    println!("    predicted proof size at {}: {:.2} GB", n_eps(64), prof.proof_size::<S, S>(n_eps(64)) as f64 / 1e9);

    let t = Instant::now();
    let output = exec::<_, WP>(&circuit);
    let exec_time = t.elapsed();
    let addr: Vec<String> = output.u8.iter().map(|b| format!("{b:02x}")).collect();
    println!("  executed in {exec_time:.2?} → address 0x{}", addr.join(""));

    let t = Instant::now();
    let proof = par_prove::<_, H, PS, PV, S, WTP>(&circuit, n, SEED_ENTROPY, BINDING);
    let prove_time = t.elapsed();
    println!("  proved {n} response(s) in {prove_time:.2?}");

    let verify_circuit = EthereumAddressCircuit::public_only();
    let t = Instant::now();
    let ok = verify::<_, H, PV, S, WPP>(&verify_circuit, &output, &proof, BINDING).expect("verify");
    let verify_time = t.elapsed();
    assert!(ok, "native verification failed");
    println!("  natively verified in {verify_time:.2?}");

    let input = ElephantInput {
        version: ELEPHANT_FRAME_VERSION,
        expected_output: output,
        binding: BINDING.to_vec(),
        proof,
    };
    let frame = postcard::to_allocvec(&input).expect("serialize frame");
    println!("  frame: {} bytes ({:.2} MB), {:.2} MB per response", frame.len(), frame.len() as f64 / 1e6, frame.len() as f64 / 1e6 / n as f64);

    let client = ProverClient::from_env();
    let mut stdin = sp1_sdk::SP1Stdin::new();
    stdin.write_vec(frame.clone());
    let t = Instant::now();
    let (_, report) = client.execute(ELEPHANT_ELF, stdin).run().expect("guest execution");
    let exec_wall = t.elapsed();
    assert_eq!(report.exit_code, 0, "guest execution failed");
    let instructions = report.total_instruction_count();
    println!("  zkVM executed in {exec_wall:.2?} → {instructions} instructions ({:.3e} per response)", instructions as f64 / n as f64);

    let per_response = instructions as f64 / n as f64;
    let full = per_response * n_eps(64) as f64;
    let ratio = per_response / HASH_INSTR_PER_RESPONSE;
    let seconds = full / (HASH_FULL_INSTRUCTIONS / HASH_FULL_SECONDS);
    println!();
    println!("  ── extrapolation to a full classical-128 lift ({} responses) ──", n_eps(64));
    println!("    vs hardened child: {ratio:.0}x the instructions per response");
    println!("    instructions at {}: {full:.3e}", n_eps(64));
    println!("    proof data through the guest: {:.1} GB", predicted_response as f64 * n_eps(64) as f64 / 1e9);
    println!("    at this machine's measured rate ({:.0}k instr/s): {:.1} days",
        HASH_FULL_INSTRUCTIONS / HASH_FULL_SECONDS / 1e3, seconds / 86400.0);
    println!();
    println!("  ⚠️  The rate above is the hardened-child statement's, assumed to carry over.");
    println!("      It is an extrapolation from one response, not a measured elephant lift.");
}

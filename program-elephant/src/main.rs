// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program for the elephant statement, used to measure what lifting it costs.
//!
//! Structurally identical to the production lift program — the genuine
//! `zkboo::verifier::verify` over a circuit rebuilt in-program, with the same
//! fingerprint pass — but over the secp256k1 Ethereum-address circuit. It exists to put
//! an instruction count on a statement whose responses are hundreds of times larger than
//! the hash-only ones, before anything is built on it.

#![no_main]
sp1_zkvm::entrypoint!(main);

use snark_lifter_elephant::{ELEPHANT_FRAME_VERSION, ElephantInput, EthereumAddressCircuit, H, PV, S};
use zkboo::verifier::{replay::OwnedFlexibleWordPairPool, verify};
use zkboo_circuit_hash::hash_circuit;

type WPP = OwnedFlexibleWordPairPool<usize>;

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: ElephantInput = postcard::from_bytes(&frame).expect("malformed input frame");
    assert_eq!(input.version, ELEPHANT_FRAME_VERSION, "input frame version mismatch");
    let circuit = EthereumAddressCircuit::public_only();
    let fingerprint = hash_circuit::<_, H>(&circuit);
    let valid = verify::<_, H, PV, S, WPP>(
        &circuit,
        &input.expected_output,
        &input.proof,
        &input.binding,
    )
    .expect("view replay");
    assert!(valid, "ZKBoo proof rejected");
    sp1_zkvm::io::commit_slice(fingerprint.as_ref());
    sp1_zkvm::io::commit_slice(&(input.proof.len() as u32).to_le_bytes());
}

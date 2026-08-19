// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM guest: the SNARK lifter proper.
//!
//! Verifies a ZKBoo proof of seed by running the genuine `zkboo::verifier::verify` — the
//! same code that verifies these proofs natively, with no re-implementation in between —
//! and commits the complete lifted statement as a [`LiftJournal`]: the circuit
//! fingerprint (computed here, in-guest, over the circuit reconstructed from the public
//! parameters — never host-supplied), the public parameters, the expected output, the
//! binding, and the soundness-critical repetition count. The guest asserts validity
//! before committing, so a receipt only ever exists for an accepting run.

use risc0_zkvm::guest::env;
use snark_lifter_statement::{
    H, HardenedChildCircuit, LIFT_FRAME_VERSION, LIFT_JOURNAL_VERSION, LiftInput, LiftJournal, PV,
    S,
};
use zkboo::verifier::{replay::OwnedFlexibleWordPairPool, verify};
use zkboo_circuit_hash::hash_circuit;

type WPP = OwnedFlexibleWordPairPool<usize>;

fn main() {
    // Length-prefixed frame, packed into u32 words (little-endian) for `read_slice`.
    let frame_len: u32 = env::read();
    let mut words = vec![0u32; (frame_len as usize).div_ceil(4)];
    env::read_slice(&mut words);
    let mut frame: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    frame.truncate(frame_len as usize);
    let input: LiftInput = postcard::from_bytes(&frame).expect("malformed input frame");
    assert_eq!(input.version, LIFT_FRAME_VERSION, "input frame version mismatch");
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let fingerprint = hash_circuit::<_, H>(&circuit);
    let valid = verify::<_, H, PV, S, WPP>(
        &circuit,
        &input.expected_output,
        &input.proof,
        &input.binding,
    )
    .expect("view replay error");
    assert!(valid, "ZKBoo proof rejected");
    env::commit(&LiftJournal {
        version: LIFT_JOURNAL_VERSION,
        valid,
        fingerprint,
        chain_code: input.chain_code,
        index: input.index,
        expected_output: input.expected_output,
        binding: input.binding,
        num_responses: input.proof.len().try_into().expect("response count exceeds u32"),
    });
}

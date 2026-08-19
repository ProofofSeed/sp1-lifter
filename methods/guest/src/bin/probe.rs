// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM guest: verifies a ZKBoo proof of seed inside the RISC Zero zkVM.
//!
//! The guest is a thin shim around the genuine `zkboo::verifier::verify` — the same code
//! that verifies these proofs natively — so the lifted claim is exactly "this ZKBoo proof
//! verifies", with no re-implementation in between. It reads a postcard-encoded
//! [`LiftInput`] frame, reconstructs the circuit from its public parameters (the witness
//! slots are structural zeros; the replay backend ignores them), verifies, and commits a
//! [`ProbeJournal`] recording the verdict and the cycles spent in verification proper.

use risc0_zkvm::guest::env;
use snark_lifter_statement::{H, HardenedChildCircuit, LiftInput, PV, ProbeJournal, S};
use zkboo::verifier::{replay::OwnedFlexibleWordPairPool, verify};

type WPP = OwnedFlexibleWordPairPool<usize>;

fn main() {
    // Length-prefixed frame, packed into u32 words (little-endian) for `read_slice`.
    let frame_len: u32 = env::read();
    let mut words = vec![0u32; (frame_len as usize).div_ceil(4)];
    env::read_slice(&mut words);
    let mut frame: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    frame.truncate(frame_len as usize);
    let input: LiftInput = postcard::from_bytes(&frame).expect("malformed input frame");
    assert_eq!(
        input.version,
        snark_lifter_statement::LIFT_FRAME_VERSION,
        "input frame version mismatch"
    );
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let before = env::cycle_count();
    let valid = verify::<_, H, PV, S, WPP>(
        &circuit,
        &input.expected_output,
        &input.proof,
        &input.binding,
    )
    .expect("view replay error");
    let after = env::cycle_count();
    env::commit(&ProbeJournal {
        valid,
        num_responses: input.proof.len() as u32,
        verify_cycles: after - before,
    });
}

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
    let frame = env::read_frame();
    let input: LiftInput = postcard::from_bytes(&frame).expect("malformed input frame");
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

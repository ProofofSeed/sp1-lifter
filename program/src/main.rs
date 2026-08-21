// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program: the SNARK lifter proper, on SP1.
//!
//! Verifies a ZKBoo proof of seed by running the genuine `zkboo::verifier::verify` — the
//! same code that verifies these proofs natively, with no re-implementation in between —
//! and commits the complete lifted statement as postcard-encoded [`LiftJournal`] public
//! values: the circuit fingerprint (computed here, in-program, over the circuit rebuilt
//! from the public parameters — never host-supplied), the public parameters, the
//! expected output, the binding, and the soundness-critical repetition count. The
//! program asserts validity before committing, so a proof only ever exists for an
//! accepting run. The crypto suite and circuit family are bound by the SP1 verifying
//! key (the image-id analogue): verifying a proof against this program's vkey is the
//! mandatory precondition of the journal gate.

#![no_main]
sp1_zkvm::entrypoint!(main);

use snark_lifter_statement::{
    H, HardenedChildCircuit, LIFT_FRAME_VERSION, LIFT_JOURNAL_VERSION, LiftInput, LiftJournal, PV,
    S,
};
use zkboo::verifier::{replay::OwnedFlexibleWordPairPool, verify};
use zkboo_circuit_hash::hash_circuit;

type WPP = OwnedFlexibleWordPairPool<usize>;

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: LiftInput = postcard::from_bytes(&frame).expect("malformed input frame");
    assert_eq!(input.version, LIFT_FRAME_VERSION, "input frame version mismatch");
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let fingerprint = hash_circuit::<_, H>(&circuit);
    // SP1 can prove FAILED executions (they verify only under an explicit non-success
    // status; the relying-party gate demands success), so refusals both commit a
    // diagnostic tag as the public values and panic: the tag names the refuser for
    // harness assertions, the panic makes the failure a non-success execution.
    let valid = match verify::<_, H, PV, S, WPP>(
        &circuit,
        &input.expected_output,
        &input.proof,
        &input.binding,
    ) {
        Ok(valid) => valid,
        Err(e) => {
            sp1_zkvm::io::commit_slice(format!("REFUSED: view replay error: {e:?}").as_bytes());
            panic!("view replay error");
        }
    };
    if !valid {
        sp1_zkvm::io::commit_slice(b"REFUSED: ZKBoo proof rejected");
        panic!("ZKBoo proof rejected");
    }
    let journal = LiftJournal {
        version: LIFT_JOURNAL_VERSION,
        valid,
        fingerprint,
        chain_code: input.chain_code,
        index: input.index,
        expected_output: input.expected_output,
        binding: input.binding,
        num_responses: input.proof.len().try_into().expect("response count exceeds u32"),
        proof_format_version: zkboo::PROOF_FORMAT_VERSION,
    };
    sp1_zkvm::io::commit_slice(&postcard::to_allocvec(&journal).expect("encode journal"));
}

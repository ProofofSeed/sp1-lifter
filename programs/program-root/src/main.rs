// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program: aggregates per-response leaf proofs under the global Fiat-Shamir gate.
//!
//! Each leaf proof is admitted through SP1's proof-composition syscall, addressed by
//! `(leaf verifying-key digest, SHA-256 of the leaf's public values)`. The syscall does
//! not verify anything during execution — it records the claim, and SP1's recursion layer
//! discharges it when this program is actually proved. An *executed* root therefore
//! demonstrates the gate but not the leaf-key binding; only a proved one does both.
//!
//! Having admitted the leaves, the root does the part of verification that does not
//! distribute: re-derive the Fiat-Shamir challenge sequence over the commitments of all
//! responses present, and require it to match what each response declared. It then
//! commits the same `LiftJournal` the monolithic lifter commits, so the existing
//! relying-party gate applies to it unchanged.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};
use pofseed_sp1_lift_statement::{
    H, HardenedChildCircuit, LIFT_JOURNAL_VERSION, LiftJournal,
};
use pofseed_sp1_lift_split::{
    LeafJournal, ROOT_FRAME_VERSION, ROOT_JOURNAL_VERSION, RootInput, RootJournal, aggregate,
    statement_digest,
};
use pofseed_sp1_lift_pinned_keys::LEAF_VK;
use zkboo_circuit_hash::hash_circuit;

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: RootInput = postcard::from_bytes(&frame).expect("malformed root frame");
    assert_eq!(input.version, ROOT_FRAME_VERSION, "root frame version mismatch");
    // 1. Admit every leaf proof under the expected leaf verifying key. The public-value
    //    bytes hashed here are the very bytes decoded below, so the journal the gate
    //    reads is the journal the leaf proof attests — not a re-encoding of it.
    // The leaf key is COMPILED IN, so this root's own verifying key says which leaf program
    // it will admit — there is no longer a field a relying party can forget to check. The
    // frame still carries the claimed key and the journal still commits it, so a journal
    // remains self-describing; this refuses any claim that disagrees with what was used.
    if input.leaf_vk != LEAF_VK {
        sp1_zkvm::io::commit_slice(b"REFUSED: claimed leaf key is not the pinned one");
        panic!("claimed leaf key is not the pinned one");
    }
    let mut leaves: Vec<LeafJournal> = Vec::with_capacity(input.leaf_public_values.len());
    for public_values in &input.leaf_public_values {
        let pv_digest: [u8; 32] = Sha256::digest(public_values).into();
        sp1_zkvm::lib::verify::verify_sp1_proof(&LEAF_VK, &pv_digest);
        leaves.push(postcard::from_bytes(public_values).expect("malformed leaf journal"));
    }
    // 2. The cross-leaf global Fiat-Shamir gate.
    let expected_statement = statement_digest(
        &input.chain_code,
        input.index,
        &input.expected_output,
        &input.binding,
    );
    if let Err(e) = aggregate(&leaves, &expected_statement, &input.binding) {
        sp1_zkvm::io::commit_slice(format!("REFUSED: {}", e.tag()).as_bytes());
        panic!("aggregate gate refused");
    }
    // 3. The circuit fingerprint, computed once for the whole proof rather than once per
    //    response: the hoisting the split depends on.
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let fingerprint = hash_circuit::<_, H>(&circuit);
    let journal = RootJournal {
        version: ROOT_JOURNAL_VERSION,
        lift: LiftJournal {
            version: LIFT_JOURNAL_VERSION,
            valid: true,
            fingerprint,
            chain_code: input.chain_code,
            index: input.index,
            expected_output: input.expected_output,
            binding: input.binding,
            num_responses: leaves.len().try_into().expect("response count exceeds u32"),
            proof_format_id: zkboo::PROOF_FORMAT_ID,
        },
        leaf_vk: LEAF_VK,
    };
    sp1_zkvm::io::commit_slice(&postcard::to_allocvec(&journal).expect("encode root journal"));
}

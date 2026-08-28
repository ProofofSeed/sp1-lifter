// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program: aggregates elephant leaf proofs under the cross-leaf Fiat-Shamir gate.
//!
//! The elephant counterpart of `program-root`. Each leaf proof is admitted through SP1's
//! proof-composition syscall, addressed by the leaf verifying-key digest and the SHA-256 of
//! the leaf's public values; the gate then re-derives the challenge sequence over the
//! commitments of every response present and requires it to match what each response
//! declared.
//!
//! It commits an `ElephantJournal` — the elephant probe's own journal shape — rather than the
//! hardened child's `LiftJournal`, because the statement has no chain code and no index.

#![no_main]
sp1_zkvm::entrypoint!(main);

use sha2::{Digest, Sha256};
use snark_lifter_elephant::{
    ELEPHANT_JOURNAL_VERSION, ElephantJournal, ElephantRootJournal, EthereumAddressCircuit, H,
};
use snark_lifter_split::{
    ELEPHANT_SPLIT_FRAME_VERSION, ElephantRootInput, LeafJournal, ROOT_JOURNAL_VERSION,
    aggregate, statement_digest_generic,
};
use zkboo_circuit_hash::hash_circuit;

const NO_PUBLIC_PARAMS: &[u8] = &[];

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: ElephantRootInput = postcard::from_bytes(&frame).expect("malformed root frame");
    assert_eq!(input.version, ELEPHANT_SPLIT_FRAME_VERSION, "root frame version mismatch");
    // 1. Admit every leaf proof under the expected leaf verifying key. The public-value bytes
    //    hashed here are the very bytes decoded below, so the journal the gate reads is the
    //    journal the leaf proof attests — not a re-encoding of it.
    let mut leaves: Vec<LeafJournal> = Vec::with_capacity(input.leaf_public_values.len());
    for public_values in &input.leaf_public_values {
        let pv_digest: [u8; 32] = Sha256::digest(public_values).into();
        sp1_zkvm::lib::verify::verify_sp1_proof(&input.leaf_vk, &pv_digest);
        leaves.push(postcard::from_bytes(public_values).expect("malformed leaf journal"));
    }
    // 2. The cross-leaf global Fiat-Shamir gate.
    let expected_statement =
        statement_digest_generic(NO_PUBLIC_PARAMS, &input.expected_output, &input.binding);
    if let Err(e) = aggregate(&leaves, &expected_statement, &input.binding) {
        sp1_zkvm::io::commit_slice(format!("REFUSED: {}", e.tag()).as_bytes());
        panic!("aggregate gate refused");
    }
    // 3. The circuit fingerprint, once for the whole proof rather than once per response —
    //    on this statement that traversal is the fixed term of the instruction law, so the
    //    hoisting is what makes the split cheaper than the monolithic lift rather than
    //    dearer.
    let circuit = EthereumAddressCircuit::public_only();
    let fingerprint = hash_circuit::<_, H>(&circuit);
    let journal = ElephantRootJournal {
        version: ROOT_JOURNAL_VERSION,
        lift: ElephantJournal {
            version: ELEPHANT_JOURNAL_VERSION,
            valid: true,
            fingerprint,
            expected_output: input.expected_output,
            binding: input.binding,
            num_responses: leaves.len().try_into().expect("response count exceeds u32"),
            proof_format_version: zkboo::PROOF_FORMAT_VERSION,
        },
        leaf_vk: input.leaf_vk,
    };
    sp1_zkvm::io::commit_slice(&postcard::to_allocvec(&journal).expect("encode root journal"));
}

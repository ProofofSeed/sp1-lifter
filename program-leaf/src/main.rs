// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program: replays ONE ZKBoo response and commits its view commitments.
//!
//! This is the leaf of the split-and-aggregate probe. It does the part of verification
//! that distributes — the view replay — and none of the part that does not. It therefore
//! attests nothing on its own; see `snark_lifter_split::LeafJournal`.
//!
//! Note what is deliberately absent: the circuit fingerprint. Computing it here would
//! pay a whole-circuit structural traversal once per response instead of once per proof,
//! which on the secp256k1 statement is worth about two responses of work — the split's
//! entire margin, spent 219 times over. The circuit is compiled into this program and so
//! is pinned by its verifying key; the fingerprint is computed once, in the root.

#![no_main]
sp1_zkvm::entrypoint!(main);

use snark_lifter_statement::{H, HardenedChildCircuit, PV, S};
use snark_lifter_split::{
    LEAF_FRAME_VERSION, LEAF_JOURNAL_VERSION, LeafCommitment, LeafInput, LeafJournal,
    statement_digest,
};
use zkboo::{
    circuit::Circuit,
    verifier::replay::{OwnedFlexibleWordPairPool, ViewReplayerBackend},
};

type WPP = OwnedFlexibleWordPairPool<usize>;

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: LeafInput = postcard::from_bytes(&frame).expect("malformed leaf frame");
    assert_eq!(input.version, LEAF_FRAME_VERSION, "leaf frame version mismatch");
    // Reject a structurally malformed (untrusted) response before replaying it, exactly
    // as `zkboo::verify` does: the replay is not defined on one.
    if !input.response.is_well_formed() {
        sp1_zkvm::io::commit_slice(b"REFUSED: malformed response");
        panic!("malformed response");
    }
    let party = input.response.challenge().index() as u8;
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let replayer =
        ViewReplayerBackend::<H, PV, S, WPP>::new(&input.response).into_view_replayer();
    circuit.exec(&replayer);
    let commitments = match replayer.finalize_with_arg(&input.expected_output) {
        Ok(commitments) => commitments,
        Err(e) => {
            sp1_zkvm::io::commit_slice(format!("REFUSED: view replay error: {e:?}").as_bytes());
            panic!("view replay error");
        }
    };
    let journal = LeafJournal {
        version: LEAF_JOURNAL_VERSION,
        position: input.position,
        statement_digest: statement_digest(
            &input.chain_code,
            input.index,
            &input.expected_output,
            &input.binding,
        ),
        party,
        commitments: commitments.map(|c| LeafCommitment {
            digest: *c.digest(),
            output_share: c.output_share().clone(),
        }),
    };
    sp1_zkvm::io::commit_slice(&postcard::to_allocvec(&journal).expect("encode leaf journal"));
}

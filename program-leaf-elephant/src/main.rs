// SPDX-License-Identifier: LGPL-3.0-or-later

//! zkVM program: replays ONE response of a secp256k1 Ethereum-address proof.
//!
//! The elephant counterpart of `program-leaf`. It does the part of verification that
//! distributes — the view replay — and none of the part that does not, so it attests nothing
//! on its own: one response has soundness 2/3, and no leaf can know whether its declared
//! party is the one Fiat-Shamir demands.
//!
//! The circuit fingerprint is deliberately absent, and on this statement that is not a
//! micro-optimisation. The in-guest structural traversal of a secp256k1 circuit is the fixed
//! term of the measured instruction law — 5.1004e9 against a marginal 2.4627e9 per response
//! — so computing it per leaf would cost about two extra responses each, more than doubling
//! the work the split exists to divide. The circuit is compiled into this program and pinned
//! by its verifying key; the root computes the fingerprint once.

#![no_main]
sp1_zkvm::entrypoint!(main);

use snark_lifter_elephant::{EthereumAddressCircuit, H, PV, S};
use snark_lifter_split::{
    ELEPHANT_SPLIT_FRAME_VERSION, ElephantLeafInput, LEAF_JOURNAL_VERSION, LeafCommitment,
    LeafJournal, statement_digest_generic,
};
use zkboo::{
    circuit::Circuit,
    verifier::replay::{OwnedFlexibleWordPairPool, ViewReplayerBackend},
};

type WPP = OwnedFlexibleWordPairPool<usize>;

/// This statement has no public circuit parameters: the witness is the private key and the
/// output is the address, so the digest is taken over an empty parameter blob.
const NO_PUBLIC_PARAMS: &[u8] = &[];

pub fn main() {
    let frame = sp1_zkvm::io::read_vec();
    let input: ElephantLeafInput = postcard::from_bytes(&frame).expect("malformed leaf frame");
    assert_eq!(input.version, ELEPHANT_SPLIT_FRAME_VERSION, "leaf frame version mismatch");
    // Reject a structurally malformed (untrusted) response before replaying it, exactly as
    // `zkboo::verify` does: the replay is not defined on one.
    if !input.response.is_well_formed() {
        sp1_zkvm::io::commit_slice(b"REFUSED: malformed response");
        panic!("malformed response");
    }
    let party = input.response.challenge().index() as u8;
    let circuit = EthereumAddressCircuit::public_only();
    let replayer = ViewReplayerBackend::<H, PV, S, WPP>::new(&input.response).into_view_replayer();
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
        statement_digest: statement_digest_generic(
            NO_PUBLIC_PARAMS,
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

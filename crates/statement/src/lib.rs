// SPDX-License-Identifier: LGPL-3.0-or-later

//! Shared statement definitions for the SNARK lifter.
//!
//! The host (which holds a ZKBoo proof of seed) and the zkVM guest (which verifies it)
//! must agree on three things: the circuit whose proof is being lifted, the crypto suite
//! the proof was generated under, and the wire format in which the verification input
//! crosses the host/guest boundary. This crate pins all three, so that both sides
//! compile against a single definition.
//!
//! The crypto suite is Keccak-256 throughout ([`Keccak256Hasher`] for commitments, PRGs
//! and challenge entropy), matching the production choice of the wider proof-of-seed
//! stack. The wire format is a postcard-encoded [`LiftInput`] frame.

#![no_std]

extern crate alloc;

mod hasher;

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use zkboo::{
    backend::{Backend, Frontend},
    circuit::{Assertions, Circuit},
    crypto::{HashPRG, Hasher},
    prover::proof::Proof,
    word::Words,
};
use zkboo_bip32::hardened_child_key;

pub use hasher::Keccak256Hasher;

/// Hasher used for commitments, PRGs and challenge entropy: Keccak-256, the production hash.
pub type H = Keccak256Hasher;

/// Seed PRG.
pub type PS = HashPRG<H>;

/// View PRG.
pub type PV = HashPRG<H>;

/// Seed type (the hasher digest type).
pub type S = <H as Hasher>::Digest;

/// A ZKBoo proof under the fixed crypto suite.
pub type LiftProof = Proof<S, S>;

/// Byte length of a BIP-32 chain code.
pub const CHAIN_CODE_LEN: usize = 32;

/// Byte length of a secp256k1 private key.
pub const PARENT_PRIV_LEN: usize = 32;

/// BIP-32 hardened-child-key derivation as a ZKBoo circuit.
///
/// Public parameters: the parent chain code and the (hardened) child index. Witness: the
/// parent private key bytes. The witness field is structural on the verifier side — the
/// view-replay backend sources input values from the opened views, ignoring the values
/// held here — so a verifier constructs the circuit via [`Self::public_only`] with an
/// all-zero placeholder of the correct length.
pub struct HardenedChildCircuit {
    chain_code: Vec<u8>,
    parent_priv: Vec<u8>,
    index: u32,
}

impl HardenedChildCircuit {
    /// Creates the circuit with a genuine witness, for proving.
    pub fn with_witness(chain_code: Vec<u8>, parent_priv: Vec<u8>, index: u32) -> Self {
        assert_eq!(chain_code.len(), CHAIN_CODE_LEN, "chain code must be 32 bytes");
        assert_eq!(parent_priv.len(), PARENT_PRIV_LEN, "parent private key must be 32 bytes");
        return Self {
            chain_code,
            parent_priv,
            index,
        };
    }

    /// Creates the circuit from public parameters only, for verifying.
    ///
    /// The witness slots are filled with zeros: the replay backend ignores witness
    /// values, but the number of allocated inputs is part of the circuit's structure
    /// and must match the prover's.
    pub fn public_only(chain_code: Vec<u8>, index: u32) -> Self {
        assert_eq!(chain_code.len(), CHAIN_CODE_LEN, "chain code must be 32 bytes");
        return Self {
            chain_code,
            parent_priv: alloc::vec![0u8; PARENT_PRIV_LEN],
            index,
        };
    }

    /// The parent chain code (public).
    pub fn chain_code(&self) -> &[u8] {
        return &self.chain_code;
    }

    /// The child index (public).
    pub fn index(&self) -> u32 {
        return self.index;
    }
}

impl Circuit for HardenedChildCircuit {
    fn exec<B: Backend>(&self, frontend: &Frontend<B>) {
        // The scope emits the assertion flag as its last output word when the body returns, so
        // everything this circuit outputs belongs inside it.
        //
        // The flag carries the validity conditions BIP-32 states, which zkboo-bip32 1.3.0
        // asserts inside the derivation: `IL` below the group order and a non-zero child key.
        // A seed reaching an invalid key has probability below 2^-127, so no honest witness
        // exercises them; what they buy is that a witness which does cannot be lifted into an
        // accepting proof. One journal shape, one gate and one on-chain decoder therefore serve
        // every statement in the suite, including the Ethereum-address one, where the flag is
        // the whole of what makes the proof mean anything. zkboo deliberately does not impose a
        // universal trailing word; a circuit that opens no scope emits none, and the gates below
        // would then refuse every proof of it.
        Assertions::scope(frontend, |assertions| {
            let chain_code = self
                .chain_code
                .iter()
                .map(|&b| frontend.alloc(b))
                .collect::<Vec<_>>();
            let parent_priv = self
                .parent_priv
                .iter()
                .map(|&b| frontend.input(b))
                .collect::<Vec<_>>();
            let (child_priv, child_chain_code) = hardened_child_key(
                frontend.allocator(),
                chain_code,
                parent_priv,
                self.index,
                assertions,
            );
            frontend.output(child_priv); // 4x u64 (little-endian word order)
            child_chain_code
                .into_iter()
                .for_each(|w| frontend.output(w)); // 32x u8
        });
    }
}

/// Version tag of the [`LiftInput`] postcard frame.
///
/// Postcard is not self-describing and the layout of [`Words`] depends on which zkboo
/// word features are enabled, so the word features are pinned explicitly in this
/// crate's manifest, and any change to them — or to the shape of [`LiftInput`] —
/// must bump this constant.
pub const LIFT_FRAME_VERSION: u32 = 1;

// zkboo 1.2.0 did not move this, although it looks as though it
// should have. That release invalidated every proof made against its predecessors — the challenge
// transcript, the view commitments and the PRG are now domain-separated, so every commitment and
// every Fiat-Shamir challenge differs. But this constant guards the *layout* of the frame, not the
// values in it, and neither the shape of `LiftInput` nor the enabled word features changed. The
// assertion word is a `u8`, a type these outputs already carry, so the postcard encoding of
// `Words` is unchanged. Bumping a version tag whose documented contract was not breached would
// make the next reader distrust the contract.

/// The verification input frame passed from host to guest (postcard-encoded).
///
/// Everything here is verifier-side data: the circuit's public parameters, the expected
/// circuit output (the public statement), the challenge binding (e.g. a transaction
/// hash) and the proof itself. The witness never crosses this boundary.
#[derive(Serialize, Deserialize)]
pub struct LiftInput {
    /// Frame version; must equal [`LIFT_FRAME_VERSION`].
    pub version: u32,
    /// Parent chain code (public circuit parameter).
    pub chain_code: Vec<u8>,
    /// Child index (public circuit parameter).
    pub index: u32,
    /// Expected circuit output: the derived child key material being proven.
    pub expected_output: Words,
    /// Challenge binding absorbed into the Fiat-Shamir entropy (empty for unbound).
    pub binding: Vec<u8>,
    /// The ZKBoo proof to verify.
    pub proof: LiftProof,
}

/// Version tag of the [`LiftJournal`] layout; bump on any change to its shape.
pub const LIFT_JOURNAL_VERSION: u32 = 3;

/// The ZKBoo proof format this statement is defined over: `zkboo::PROOF_FORMAT_ID` at the
/// revision this crate was written against.
///
/// Declared here rather than read from the linked `zkboo`, deliberately. Deriving the
/// requirement from whatever a consumer happens to link would make the check track the consumer
/// instead of constraining it: a consumer that downgraded its `zkboo` would silently begin
/// *requiring* the older format rather than refusing it. Pinning it means a format change is a
/// decision someone makes here, and until they do, proofs carrying another format are refused.
///
/// **This replaces a counter that could not do the job.** `PROOF_FORMAT_VERSION` was `3` on
/// both sides of an incompatible change — 1.2.0's domain separation moved every commitment and
/// every challenge while the constant stayed put — so this clause accepted a proof from a format
/// it was written to refuse, and no value it could have held would have separated two formats
/// declaring the same number. The identity is now the digest of a canonical reference proof, so
/// it cannot fail to move when the bytes move. It is deliberately conservative in the other
/// direction: it also moves for changes that are not format changes, such as the circuit
/// vocabulary the canonical proof is made with. Over-triggering costs a regeneration;
/// under-triggering was the failure it replaces.
pub const REQUIRED_PROOF_FORMAT_ID: [u8; 32] = [
    0x4b, 0xe5, 0xd4, 0xe6, 0x4c, 0x11, 0xba, 0x4d, 0x40, 0xe7, 0x89, 0x77, 0xf6, 0x8d, 0x45, 0xad,
    0x8b, 0xf0, 0x1f, 0xe3, 0x1a, 0xa2, 0x75, 0x07, 0xe8, 0xdd, 0xca, 0x20, 0x22, 0x3a, 0x73, 0x70,
];

/// The journal committed by the lift guest: the complete lifted statement.
///
/// A receipt over this journal attests: "a ZKBoo proof with `num_responses` responses,
/// bound to `binding`, verified under the Keccak-256 suite against the circuit whose
/// structural fingerprint is `fingerprint` (reconstructed in-guest from the public
/// parameters below) and against `expected_output`". The guest refuses (panics) before
/// committing the journal on any rejecting run; under SP1 a failed execution is still
/// provable but verifies only with an explicit non-success status, which the
/// relying-party gate never grants — so a proof verifying at SUCCESS status only
/// exists for accepting runs — and the `valid` field is
/// committed as well, as a defensive belt for journal consumers.
///
/// ⚠️ `num_responses` is soundness-critical: the ZKBoo verifier accepts a proof with any
/// number of responses (the empty proof included), and a k-response proof has soundness
/// (2/3)^k, so consumers MUST check the count against their required repetition count
/// ([`check_journal`] does).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiftJournal {
    /// Journal layout version; must equal [`LIFT_JOURNAL_VERSION`].
    pub version: u32,
    /// Whether the ZKBoo proof verified (always `true` in a committed journal).
    pub valid: bool,
    /// Structural fingerprint of the verified circuit, computed in-guest via
    /// `zkboo-circuit-hash` over the circuit reconstructed from the public parameters.
    pub fingerprint: S,
    /// Parent chain code (public circuit parameter).
    pub chain_code: Vec<u8>,
    /// Child index (public circuit parameter).
    pub index: u32,
    /// The circuit output the proof was verified against (the derived key material).
    pub expected_output: Words,
    /// The challenge binding the proof was verified under (e.g. a transaction hash).
    pub binding: Vec<u8>,
    /// Number of responses (repetitions) in the verified proof.
    pub num_responses: u32,
    /// The ZKBoo proof format the program was built against (`zkboo::PROOF_FORMAT_ID`): the
    /// layout of the response bytes together with the view-commitment scheme they are checked
    /// against, identified by the digest of a canonical reference proof.
    ///
    /// ⚠️ This cannot be inferred from `fingerprint`: adopting the ZKB++ view commitment
    /// changed every commitment — and hence every Fiat-Shamir challenge — while leaving
    /// circuits and protocol parameters untouched, so the circuit hash is blind to it.
    /// The verifying key does bind it (the format is compiled into the program), but only
    /// implicitly; committing it makes the attested format legible to a journal consumer.
    pub proof_format_id: [u8; 32],
}

/// What a journal consumer requires of a [`LiftJournal`].
pub struct JournalExpectation {
    /// Required circuit fingerprint (computed by the consumer over the circuit built
    /// from the public parameters it expects).
    pub fingerprint: S,
    /// Required parent chain code.
    pub chain_code: Vec<u8>,
    /// Required child index.
    pub index: u32,
    /// Required circuit output.
    pub expected_output: Words,
    /// Required challenge binding.
    pub binding: Vec<u8>,
    /// Minimum acceptable number of responses (e.g. `n_eps(64) == 219` for the
    /// classical-128 target).
    pub min_responses: u32,
    /// Required ZKBoo proof format identity.
    pub proof_format_id: [u8; 32],
}

/// Why a [`LiftJournal`] was refused by [`check_journal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalCheckError {
    /// The journal layout version is not [`LIFT_JOURNAL_VERSION`].
    VersionMismatch,
    /// The committed verdict is not `true`.
    NotValid,
    /// The in-guest circuit fingerprint differs from the required one.
    FingerprintMismatch,
    /// Chain code, index or expected output differ from the required statement.
    StatementMismatch,
    /// The binding differs from the required one.
    BindingMismatch,
    /// Fewer responses than the required minimum (soundness is (2/3)^k in the count;
    /// the ZKBoo verifier itself accepts any count, the empty proof included).
    TooFewResponses,
    /// The attested ZKBoo proof format differs from the required one.
    ProofFormatMismatch,
    /// The expected output carries no u8 words, so it has no assertion word.
    AssertionWordMissing,
    /// The expected output's trailing assertion word is not 1: the circuit's in-circuit
    /// assertions did not hold, so the proof — while perfectly valid — attests nothing.
    AssertionWordNotSet,
}

/// Checks a [`LiftJournal`] against a [`JournalExpectation`].
///
/// ⚠️ **Mandatory precondition: the proof carrying this journal must first be verified
/// against the expected verifying key.** The journal alone does not identify the crypto
/// suite (`H`/`PV`/`S`) or the circuit family — those are fixed at program compile time
/// and bound only by the verifying key, so a variant program (weaker hash, different
/// circuit, lower standards) could commit an identical-shaped journal. This function
/// checks the journal *contents* only; the host-side `check_lifted_proof` helper fuses
/// the two steps, and the on-chain verifier takes the verifying key as an explicit
/// argument for the same reason.
///
/// 🚨 **The assertion word is checked here, unconditionally.** A prover may supply advice — a
/// value it computes from the witness and feeds in as an ordinary secret input — which is
/// unconstrained on its own; what pins it down is the in-circuit assertions, accumulated by
/// conjunction into the trailing word of the circuit's output. A prover running the honest
/// protocol on a witness that does *not* satisfy the
/// circuit obtains a perfectly valid proof whose assertion word is **0** — a genuine proof of
/// *"I ran this circuit and its assertions did not hold"*, which anyone can produce for any
/// claimed output with no witness knowledge whatsoever.
///
/// That word being 1 is part of the statement, not a caller's preference: it is fixed, public,
/// and the same for every circuit in the suite. So this function requires it **of the journal**
/// and does not rely on the consumer having built a correct expectation — an expectation that
/// agreed with the journal in every other field would otherwise sail through. A gate comparing
/// only the meaningful bytes, the twenty address bytes say, while letting the assertion word be
/// 0, is forgeable outright.
///
/// 🪤 It is the last **u8** word, not the last byte of the blob: outputs mixing word widths keep
/// the assertion word in the u8 section.
///
/// Given those preconditions, this is the reference consumer-side gate: everything a
/// relying party must additionally check before treating a proof over this journal as
/// an authorisation. The on-chain verifier contract must enforce exactly these checks —
/// **including the assertion-word requirement above, which no code here can check for it.**
pub fn check_journal(
    journal: &LiftJournal,
    expectation: &JournalExpectation,
) -> Result<(), JournalCheckError> {
    if journal.version != LIFT_JOURNAL_VERSION {
        return Err(JournalCheckError::VersionMismatch);
    }
    if !journal.valid {
        return Err(JournalCheckError::NotValid);
    }
    if journal.fingerprint != expectation.fingerprint {
        return Err(JournalCheckError::FingerprintMismatch);
    }
    if journal.chain_code != expectation.chain_code
        || journal.index != expectation.index
        || journal.expected_output != expectation.expected_output
    {
        return Err(JournalCheckError::StatementMismatch);
    }
    if journal.binding != expectation.binding {
        return Err(JournalCheckError::BindingMismatch);
    }
    if journal.num_responses < expectation.min_responses {
        return Err(JournalCheckError::TooFewResponses);
    }
    if journal.proof_format_id != expectation.proof_format_id {
        return Err(JournalCheckError::ProofFormatMismatch);
    }
    // Unconditional, and deliberately not driven by the expectation: a proof whose in-circuit
    // assertions did not hold is valid, attests nothing, and anyone can produce one.
    match journal.expected_output.as_vec::<u8>().last() {
        None => return Err(JournalCheckError::AssertionWordMissing),
        Some(&w) if w != 1 => return Err(JournalCheckError::AssertionWordNotSet),
        Some(_) => {}
    }
    return Ok(());
}

#[cfg(test)]
mod assertion_word_tests {
    use super::*;
    use alloc::vec;

    /// A journal and an expectation agreeing on every field, with the trailing u8 output word
    /// set to `assertion`.
    fn pair(assertion: u8) -> (LiftJournal, JournalExpectation) {
        let mut output = zkboo::word::Words::new();
        output.as_vec_mut::<u8>().extend_from_slice(&[0xde, 0xad, assertion]);
        let journal = LiftJournal {
            version: LIFT_JOURNAL_VERSION,
            valid: true,
            fingerprint: [7u8; 32],
            chain_code: vec![1u8; 32],
            index: 0,
            expected_output: output.clone(),
            binding: vec![],
            num_responses: 219,
            proof_format_id: REQUIRED_PROOF_FORMAT_ID,
        };
        let expectation = JournalExpectation {
            fingerprint: [7u8; 32],
            chain_code: vec![1u8; 32],
            index: 0,
            expected_output: output,
            binding: vec![],
            min_responses: 219,
            proof_format_id: REQUIRED_PROOF_FORMAT_ID,
        };
        return (journal, expectation);
    }

    #[test]
    fn a_set_assertion_word_is_accepted() {
        let (journal, expectation) = pair(1);
        assert!(check_journal(&journal, &expectation).is_ok());
    }

    /// The forgery this exists for. The expectation **agrees with the journal in every field**,
    /// so every other clause passes; only the unconditional assertion-word check stands between
    /// a relying party and a valid proof that asserts nothing.
    #[test]
    fn a_clear_assertion_word_is_refused_even_when_the_expectation_agrees() {
        let (journal, expectation) = pair(0);
        assert_eq!(
            check_journal(&journal, &expectation),
            Err(JournalCheckError::AssertionWordNotSet)
        );
    }
}

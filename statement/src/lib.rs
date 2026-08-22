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

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use zkboo::{
    backend::{Backend, Frontend},
    circuit::Circuit,
    crypto::{HashPRG, Hasher, Keccak256Hasher},
    prover::proof::Proof,
    word::Words,
};
use zkboo_bip32::hardened_child_key;

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
        let (child_priv, child_chain_code) =
            hardened_child_key(frontend.allocator(), chain_code, parent_priv, self.index);
        frontend.output(child_priv); // 4x u64 (little-endian word order)
        child_chain_code
            .into_iter()
            .for_each(|w| frontend.output(w)); // 32x u8
    }
}

/// Version tag of the [`LiftInput`] postcard frame.
///
/// Postcard is not self-describing and the layout of [`Words`] depends on which zkboo
/// word features are enabled, so the word features are pinned explicitly in this
/// crate's manifest, and any change to them — or to the shape of [`LiftInput`] —
/// must bump this constant.
pub const LIFT_FRAME_VERSION: u32 = 1;

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
pub const LIFT_JOURNAL_VERSION: u32 = 2;

/// The ZKBoo proof format this statement is defined over (`zkboo::PROOF_FORMAT_VERSION`
/// at the revision this crate was written against).
///
/// Declared here rather than read from the linked `zkboo`, deliberately. Deriving the
/// requirement from whatever version a consumer happens to link would make the check
/// track the consumer instead of constraining it: a consumer that downgraded its `zkboo`
/// would silently begin *requiring* the older format rather than refusing it. Pinning it
/// means a format change is a decision someone makes here, and until they do, proofs
/// carrying the new format are refused.
pub const REQUIRED_PROOF_FORMAT_VERSION: u32 = 2;

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
    /// The ZKBoo proof format the program was built against (`zkboo::PROOF_FORMAT_VERSION`):
    /// the layout of the response bytes together with the view-commitment scheme they are
    /// checked against.
    ///
    /// ⚠️ This cannot be inferred from `fingerprint`: adopting the ZKB++ view commitment
    /// changed every commitment — and hence every Fiat-Shamir challenge — while leaving
    /// circuits and protocol parameters untouched, so the circuit hash is blind to it.
    /// The verifying key does bind it (the format is compiled into the program), but only
    /// implicitly; committing it makes the attested format legible to a journal consumer.
    pub proof_format_version: u32,
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
    /// Required ZKBoo proof format version.
    pub proof_format_version: u32,
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
/// Given that precondition, this is the reference consumer-side gate: everything a
/// relying party must additionally check before treating a proof over this journal as
/// an authorisation. The on-chain verifier contract must enforce exactly these checks.
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
    if journal.proof_format_version != expectation.proof_format_version {
        return Err(JournalCheckError::ProofFormatMismatch);
    }
    return Ok(());
}

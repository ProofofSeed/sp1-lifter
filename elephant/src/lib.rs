// SPDX-License-Identifier: LGPL-3.0-or-later

//! The elephant statement: an Ethereum address derived from a seed-derived private key.
//!
//! This is the statement that actually binds a proof of seed to an Ethereum account —
//! `address = Keccak256(x_be ‖ y_be)[12:]` where `Q = (x, y) = d·G` on secp256k1 — and so
//! it is the one an ERC-4337 migration would have to lift. It is also, by a wide margin,
//! the most expensive: the in-circuit secp256k1 scalar multiplication dominates
//! everything else in the proof-of-seed suite, which is why it is probed on its own
//! before anything is built on it.
//!
//! It is kept out of the production statement crate deliberately: that crate's shape
//! determines the guest ELF and hence the verifying key pinned by the on-chain suite,
//! and a probe must not move it.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use sp1_lifter_statement::LiftProof;

/// The crypto suite, re-exported so consumers of this statement need not also depend on
/// the production statement crate: the two must use the same suite or nothing composes.
pub use sp1_lifter_statement::{H, PS, PV, S};
use zkboo::{
    backend::{Backend, Frontend},
    circuit::{Assertions, Circuit},
    word::{CompositeWord, Words},
};
use zkboo_bip32::{
    PublicKeyAdvice, be_bytes_to_word, ethereum_address, public_key_advice,
    public_key_advice_shape,
};

/// Byte length of a secp256k1 private key.
pub const PRIV_KEY_LEN: usize = 32;

/// The private key as a host word: 32 big-endian bytes as four `u64` limbs.
fn be_words(bytes: &[u8]) -> CompositeWord<u64, 4> {
    let mut limbs = [0u64; 4];
    for (i, chunk) in bytes.chunks(8).enumerate() {
        limbs[i] = u64::from_be_bytes(chunk.try_into().expect("eight bytes"));
    }
    return CompositeWord::from_be_words(limbs);
}

/// Ethereum-address derivation as a ZKBoo circuit.
///
/// No public parameters: the witness is the private key and the address is the output, followed
/// by the assertion flag.
///
/// The affine comb inside `d·G` runs on prover advice — one chord slope per window, plus a
/// tangent at the top — which the prover computes on the host from the witness and feeds in as
/// ordinary secret inputs. Advice constrains nothing by itself; the in-circuit assertions the
/// comb raises are what pin it down, and they end up in the flag this circuit outputs last.
/// Ignore that flag and the statement is vacuous: see `sp1_lifter_statement::check_journal`.
pub struct EthereumAddressCircuit {
    private_key: Vec<u8>,
    advice: PublicKeyAdvice,
}

impl EthereumAddressCircuit {
    /// Creates the circuit with a genuine witness, for proving.
    pub fn with_witness(private_key: Vec<u8>) -> Self {
        assert_eq!(private_key.len(), PRIV_KEY_LEN, "private key must be 32 bytes");
        let advice = public_key_advice(be_words(&private_key));
        return Self {
            private_key,
            advice,
        };
    }

    /// Creates the circuit for verifying: the replay backend sources input values from the
    /// opened views, so only the number of allocated inputs matters here.
    ///
    /// The advice slots are part of that count, so they are allocated at the shape a genuine
    /// witness would produce and filled with zeros.
    pub fn public_only() -> Self {
        return Self {
            private_key: alloc::vec![0u8; PRIV_KEY_LEN],
            advice: public_key_advice_shape(),
        };
    }
}

impl Circuit for EthereumAddressCircuit {
    fn exec<B: Backend>(&self, frontend: &Frontend<B>) {
        let bytes = self
            .private_key
            .iter()
            .map(|&b| frontend.input(b))
            .collect::<Vec<_>>();
        let d = be_bytes_to_word(&bytes);
        let mut assertions = Assertions::new();
        ethereum_address(frontend, d, &self.advice, &mut assertions)
            .into_iter()
            .for_each(|w| frontend.output(w));
        assertions.output(frontend);
    }
}

/// Version tag of the [`ElephantInput`] frame.
pub const ELEPHANT_FRAME_VERSION: u32 = 1;

/// The verification input frame passed from host to guest (postcard-encoded).
#[derive(Serialize, Deserialize)]
pub struct ElephantInput {
    /// Frame version; must equal [`ELEPHANT_FRAME_VERSION`].
    pub version: u32,
    /// Expected circuit output: the 20-byte Ethereum address.
    pub expected_output: Words,
    /// Challenge binding absorbed into the Fiat-Shamir entropy.
    pub binding: Vec<u8>,
    /// The ZKBoo proof to verify.
    pub proof: LiftProof,
}

/// Version tag of the [`ElephantJournal`] layout; bump on any change to its shape.
pub const ELEPHANT_JOURNAL_VERSION: u32 = 1;

/// The journal an elephant lift commits: the complete lifted statement.
///
/// The hardened child's `LiftJournal` without a chain code or an index, because this statement
/// has no public circuit parameters — the witness is a private key and the output is the
/// address that key controls. Everything else is the same, for the same reasons, and
/// `num_responses` is soundness-critical here exactly as it is there: `zkboo::verify` accepts a
/// proof with any number of responses, the empty proof included, and a k-response proof has
/// soundness (2/3)^k.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElephantJournal {
    /// Journal layout version; must equal [`ELEPHANT_JOURNAL_VERSION`].
    pub version: u32,
    /// Whether the ZKBoo proof verified (always `true` in a committed journal).
    pub valid: bool,
    /// Structural fingerprint of the verified circuit, computed in-guest.
    pub fingerprint: S,
    /// The circuit output the proof was verified against: the 20-byte address, and the
    /// assertion word the circuit emits last.
    pub expected_output: Words,
    /// The challenge binding the proof was verified under.
    pub binding: Vec<u8>,
    /// Number of responses (repetitions) in the verified proof.
    pub num_responses: u32,
    /// The ZKBoo proof format the program was built against.
    pub proof_format_version: u32,
}

/// The journal an elephant **root** commits: the lift, plus the leaf key it admitted under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElephantRootJournal {
    /// Layout version.
    pub version: u32,
    /// The lifted statement, in the shape a monolithic elephant lift would commit.
    pub lift: ElephantJournal,
    /// The verifying key every admitted leaf proof carried.
    ///
    /// ✅ **Compiled into the root** since 2026-08-28: it refuses any claimed key disagreeing
    /// with `sp1_lifter_pinned_keys::LEAF_ELEPHANT_VK` before admitting a proof, so the
    /// root's own verifying key says which leaf program it admits and this field is a
    /// self-description rather than a load-bearing input. (This doc previously said it was
    /// prover-chosen and had to be checked by a relying party; that stopped being true when
    /// the pin landed.) See `knowledge/split-and-streaming.md`.
    pub leaf_vk: [u32; 8],
}

/// What a journal consumer requires of an [`ElephantJournal`].
pub struct ElephantExpectation {
    /// Required circuit fingerprint.
    pub fingerprint: S,
    /// Required circuit output.
    pub expected_output: Words,
    /// Required challenge binding.
    pub binding: Vec<u8>,
    /// Minimum acceptable number of responses.
    pub min_responses: u32,
    /// Required ZKBoo proof format version.
    pub proof_format_version: u32,
}

/// Why an [`ElephantJournal`] was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElephantCheckError {
    /// The journal layout version is not [`ELEPHANT_JOURNAL_VERSION`].
    VersionMismatch,
    /// The committed verdict is not `true`.
    NotValid,
    /// The in-guest circuit fingerprint differs from the required one.
    FingerprintMismatch,
    /// The expected output differs from the required statement.
    StatementMismatch,
    /// The binding differs from the required one.
    BindingMismatch,
    /// Fewer responses than the required minimum.
    TooFewResponses,
    /// The attested ZKBoo proof format differs from the required one.
    ProofFormatMismatch,
    /// The expected output carries no u8 words, so it has no assertion word.
    AssertionWordMissing,
    /// The expected output's trailing assertion word is not 1: the circuit's in-circuit
    /// assertions did not hold, so the proof — while valid — attests nothing.
    AssertionWordNotSet,
}

/// Checks an [`ElephantJournal`] against an [`ElephantExpectation`].
///
/// ⚠️ **Mandatory precondition: the proof carrying this journal must first be verified against
/// the expected verifying key**, which alone binds the crypto suite and the circuit family. The
/// same precondition, for the same reason, as the hardened child's `check_journal`.
pub fn check_elephant_journal(
    journal: &ElephantJournal,
    expectation: &ElephantExpectation,
) -> Result<(), ElephantCheckError> {
    if journal.version != ELEPHANT_JOURNAL_VERSION {
        return Err(ElephantCheckError::VersionMismatch);
    }
    if !journal.valid {
        return Err(ElephantCheckError::NotValid);
    }
    if journal.fingerprint != expectation.fingerprint {
        return Err(ElephantCheckError::FingerprintMismatch);
    }
    if journal.expected_output != expectation.expected_output {
        return Err(ElephantCheckError::StatementMismatch);
    }
    if journal.binding != expectation.binding {
        return Err(ElephantCheckError::BindingMismatch);
    }
    if journal.num_responses < expectation.min_responses {
        return Err(ElephantCheckError::TooFewResponses);
    }
    if journal.proof_format_version != expectation.proof_format_version {
        return Err(ElephantCheckError::ProofFormatMismatch);
    }
    // Unconditional; see `sp1_lifter_statement::check_journal`. A proof whose in-circuit
    // assertions did not hold is valid, attests nothing, and anyone can produce one for any
    // address without knowing a seed.
    match journal.expected_output.as_vec::<u8>().last() {
        None => return Err(ElephantCheckError::AssertionWordMissing),
        Some(&w) if w != 1 => return Err(ElephantCheckError::AssertionWordNotSet),
        Some(_) => {}
    }
    return Ok(());
}

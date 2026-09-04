// SPDX-License-Identifier: LGPL-3.0-or-later

//! Split-and-aggregate probe: the per-response leaf statement and the cross-leaf gate.
//!
//! The production lifter verifies all `N_ε` responses in one zkVM program. This crate
//! defines the alternative shape — one proof per response, aggregated at the end — so
//! that its cost can be measured against the monolithic run.
//!
//! **Why the split is sound at all.** `zkboo`'s own `par_verify` already has this
//! structure: each response is replayed independently to yield its three view
//! commitments, and only then are the commitments absorbed *in order* into the challenge
//! hasher, the challenge sequence re-derived, and each response's declared challenge
//! party checked against it. The per-response replay never consults the global challenge;
//! it consults only the party the response itself declares. So the replay distributes and
//! the challenge check does not.
//!
//! **Why the split is unsound without the root.** The Fiat-Shamir entropy is a hash over
//! the commitments of exactly the responses present, so a subset of leaves re-derives a
//! different challenge sequence and attests nothing about the whole. [`aggregate`] is the
//! gate that puts that back: it is the only thing standing between "219 independent
//! proofs" and "a proof of seed". Everything it checks is load-bearing.
//!
//! This crate is deliberately kept out of the production statement crate: nothing here
//! links into `program/`, so probing cannot move the pinned verifying key.

#![no_std]

extern crate alloc;

use alloc::{vec::Vec, format, string::String};
use serde::{Deserialize, Serialize};
use snark_lifter_statement::{H, S};
use zkboo::{
    crypto::{
        GeneratesRandom, HashPRG, Hasher, PseudoRandomGenerator, TAG_CHALLENGE, absorb_framed,
    },
    prover::challenge::ChallengeGenerator,
    word::Words,
};

/// Version tag of the [`LeafInput`] postcard frame.
pub const LEAF_FRAME_VERSION: u32 = 1;

/// Version tag of the [`LeafJournal`] layout.
pub const LEAF_JOURNAL_VERSION: u32 = 1;

/// Version tag of the [`RootInput`] postcard frame.
pub const ROOT_FRAME_VERSION: u32 = 1;

/// Version tag of the [`RootJournal`] layout.
pub const ROOT_JOURNAL_VERSION: u32 = 1;

/// Domain separator for [`statement_digest`].
const STATEMENT_DOMAIN: &[u8] = b"SNARK-LIFTER-SPLIT-STATEMENT-v1";

/// The digest binding a leaf to the statement it is a response for.
///
/// Every leaf commits this, and [`aggregate`] refuses any leaf whose digest differs from
/// the one the root was asked to attest. Without it, leaves proved against different
/// statements could be aggregated into a single journal naming just one of them.
pub fn statement_digest(chain_code: &[u8], index: u32, expected_output: &Words, binding: &[u8]) -> S {
    let mut params = alloc::vec::Vec::with_capacity(chain_code.len() + 4);
    params.extend_from_slice(chain_code);
    params.extend_from_slice(&index.to_le_bytes());
    return statement_digest_generic(&params, expected_output, binding);
}

/// The statement digest over an opaque encoding of a circuit's public parameters.
///
/// Every statement in the suite has a different set of them — the hardened child has a chain
/// code and an index, the Ethereum address has none at all — but the cross-leaf gate does not
/// care what they are, only that every leaf agrees on them. Encoding them as one length-framed
/// blob is what lets one gate serve every statement.
///
/// ⚠️ The encoding is the caller's, and two statements whose parameters encode identically would
/// share a digest. Each statement's leaf and root programs are separate ELFs with separate
/// verifying keys, so a leaf cannot be presented to another statement's root regardless; this
/// framing is what makes the digest unambiguous *within* one of them.
pub fn statement_digest_generic(public_params: &[u8], expected_output: &Words, binding: &[u8]) -> S {
    let mut hasher = H::new();
    hasher.update(STATEMENT_DOMAIN);
    hasher.update(&(public_params.len() as u64).to_le_bytes());
    hasher.update(public_params);
    expected_output.update_hasher(&mut hasher);
    hasher.update(&(binding.len() as u64).to_le_bytes());
    hasher.update(binding);
    return hasher.finalize();
}

/// Version tag of the elephant leaf/root frames.
pub const ELEPHANT_SPLIT_FRAME_VERSION: u32 = 1;

/// The frame passed to the **elephant** leaf program: one response of an Ethereum-address proof.
///
/// The statement has no public circuit parameters — the witness is a private key and the output is
/// the address — so where the hardened-child frame carries a chain code and an index, this carries
/// nothing, and its statement digest is taken over an empty parameter blob.
#[derive(Serialize, Deserialize)]
pub struct ElephantLeafInput {
    /// Frame version; must equal [`ELEPHANT_SPLIT_FRAME_VERSION`].
    pub version: u32,
    /// Expected circuit output: the 20-byte address, and the assertion word.
    pub expected_output: Words,
    /// Challenge binding (empty for unbound).
    pub binding: Vec<u8>,
    /// This response's position in the proof, counted from zero.
    pub position: u32,
    /// The single response this leaf replays.
    pub response: zkboo::prover::proof::Response<S, S>,
}

/// The frame passed to the **elephant** root program.
#[derive(Serialize, Deserialize)]
pub struct ElephantRootInput {
    /// Frame version; must equal [`ELEPHANT_SPLIT_FRAME_VERSION`].
    pub version: u32,
    /// Expected circuit output.
    pub expected_output: Words,
    /// Challenge binding.
    pub binding: Vec<u8>,
    /// The verifying key every leaf proof must carry.
    pub leaf_vk: [u32; 8],
    /// Each leaf's committed public values, in response order.
    pub leaf_public_values: Vec<Vec<u8>>,
}

/// The frame passed to the leaf program: the statement, and ONE response of the proof.
#[derive(Serialize, Deserialize)]
pub struct LeafInput {
    /// Frame version; must equal [`LEAF_FRAME_VERSION`].
    pub version: u32,
    /// Parent chain code (public circuit parameter).
    pub chain_code: Vec<u8>,
    /// Child index (public circuit parameter).
    pub index: u32,
    /// Expected circuit output (the public statement).
    pub expected_output: Words,
    /// Challenge binding (empty for unbound).
    pub binding: Vec<u8>,
    /// This response's position in the proof, counted from zero.
    pub position: u32,
    /// The single response this leaf replays.
    pub response: zkboo::prover::proof::Response<S, S>,
}

/// One party's view commitment, as the challenge hasher consumes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafCommitment {
    /// The commitment digest.
    pub digest: S,
    /// The party's share of the circuit output.
    pub output_share: Words,
}

/// What a leaf proof attests: one response replayed, and the commitments it produced.
///
/// ⚠️ A leaf attests **nothing about soundness on its own**. A single response has
/// soundness 2/3, and its challenge party is whatever the response declared — no leaf
/// can know whether that party is the one Fiat-Shamir actually demands, because that
/// depends on every other response. Only [`aggregate`] turns a set of these into a
/// statement about a proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafJournal {
    /// Journal layout version; must equal [`LEAF_JOURNAL_VERSION`].
    pub version: u32,
    /// This response's position in the proof.
    pub position: u32,
    /// [`statement_digest`] of the statement this leaf was replayed against.
    pub statement_digest: S,
    /// The challenge party the response declared (0, 1 or 2).
    pub party: u8,
    /// The three view commitments the replay produced, in party order.
    pub commitments: [LeafCommitment; 3],
}

/// The frame passed to the root program.
///
/// `leaf_public_values` carries the leaf journals as bytes because that is what the root
/// must hash to invoke SP1's proof-composition syscall: the syscall is addressed by
/// `(verifying key digest, SHA-256 of public values)`, so the root re-hashes the exact
/// bytes it then decodes.
#[derive(Serialize, Deserialize)]
pub struct RootInput {
    /// Frame version; must equal [`ROOT_FRAME_VERSION`].
    pub version: u32,
    /// Parent chain code (public circuit parameter).
    pub chain_code: Vec<u8>,
    /// Child index (public circuit parameter).
    pub index: u32,
    /// Expected circuit output (the public statement).
    pub expected_output: Words,
    /// Challenge binding (empty for unbound).
    pub binding: Vec<u8>,
    /// The verifying-key digest every leaf proof must have been produced under.
    ///
    /// ✅ **Both roots compile this key in** (`snark_lifter_pinned_keys`), since 2026-08-28,
    /// and refuse a claimed value that disagrees before admitting any proof — so the pin lives
    /// inside the root's own verifying key, and pinning that key gets this for free. The field
    /// is kept so a journal stays self-describing.
    ///
    /// ⚠️ It is still an *input*, so treat it as prover-chosen in any root that has not
    /// compiled it in: alone it binds nothing, since an attacker supplies both the key and the
    /// proofs under it.
    pub leaf_vk: [u32; 8],
    /// The leaf journals, as the exact public-value bytes their proofs commit, in
    /// response order.
    pub leaf_public_values: Vec<Vec<u8>>,
}

/// What a root proof attests: the full lifted statement, plus the leaf key it trusted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootJournal {
    /// Journal layout version; must equal [`ROOT_JOURNAL_VERSION`].
    pub version: u32,
    /// The lifted statement, in exactly the layout the monolithic lifter commits — so
    /// `check_journal` and the on-chain fixture guards apply to it unchanged.
    pub lift: snark_lifter_statement::LiftJournal,
    /// The leaf verifying-key digest the root verified every leaf proof against. A
    /// relying party MUST pin this; see [`RootInput::leaf_vk`].
    pub leaf_vk: [u32; 8],
}

/// Why [`aggregate`] refused a set of leaves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AggregateError {
    /// A leaf's journal layout version is not [`LEAF_JOURNAL_VERSION`].
    LeafVersionMismatch(usize),
    /// A leaf's `position` is not its index in the sequence.
    ///
    /// Well-formedness, not soundness: `position` never enters the challenge hash, so a
    /// mislabelled leaf is refused only here — while a dropped, duplicated or reordered
    /// *response* is caught by the challenge check whether or not this one runs
    /// (demonstrated by removing it and watching the matrix stay green). Kept because a
    /// leaf that lies about which response it is has no business being aggregated.
    PositionMismatch { at: usize, found: u32 },
    /// A leaf attests a different statement from the one the root was asked for.
    StatementMismatch(usize),
    /// A leaf declares a party outside `{0, 1, 2}`.
    PartyOutOfRange { at: usize, found: u8 },
    /// The re-derived Fiat-Shamir challenge sequence disagrees with the parties the
    /// leaves declared. This is the global gate: it fires on a dropped, duplicated,
    /// reordered or substituted response, and on any tampering that changes a commitment.
    ChallengeMismatch { at: usize, expected: u8, found: u8 },
    /// No leaves were supplied. The empty proof "verifies" in ZKBoo by design, so the
    /// count is part of the statement, never an implicit success.
    NoLeaves,
}

impl AggregateError {
    /// A short human-readable tag, for guest diagnostics.
    pub fn tag(&self) -> String {
        return match self {
            Self::LeafVersionMismatch(i) => format!("leaf {i}: journal version mismatch"),
            Self::PositionMismatch { at, found } => format!("leaf {at}: position {found}"),
            Self::StatementMismatch(i) => format!("leaf {i}: statement mismatch"),
            Self::PartyOutOfRange { at, found } => format!("leaf {at}: party {found}"),
            Self::ChallengeMismatch { at, expected, found } => {
                format!("leaf {at}: challenge {found} != {expected}")
            }
            Self::NoLeaves => format!("no leaves"),
        };
    }
}

/// The cross-leaf global Fiat-Shamir gate: the whole soundness content of the split.
///
/// Given the leaf journals in response order and the statement digest the root is
/// attesting, this re-runs the second half of `zkboo::par_verify` — absorb every view
/// commitment in order into the challenge hasher, re-derive the challenge sequence from
/// the resulting entropy, and require it to be exactly the sequence of parties the
/// responses declared.
///
/// The leaves must additionally be a complete, in-order, duplicate-free cover of the
/// proof, all against one statement; without that, "N leaves verified" would not mean
/// "an N-response proof verified".
///
/// ⚠️ This function assumes each journal has already been attested by a verified leaf
/// proof under the expected leaf verifying key. It checks the *contents* of a leaf set
/// only, exactly as `check_journal` checks the contents of a journal only.
pub fn aggregate(
    leaves: &[LeafJournal],
    statement_digest: &S,
    binding: &[u8],
) -> Result<(), AggregateError> {
    if leaves.is_empty() {
        return Err(AggregateError::NoLeaves);
    }
    let mut challenge_hasher = H::new();
    // The framed binding that opens the challenge transcript. This must match zkboo's prover and
    // verifier byte for byte or the re-derived challenge sequence is not the one the proof was
    // made against, so the tag and the framing helper are taken from zkboo itself rather than
    // copied. They were copied until 1.2.0, under the tag `ZKBOO-BINDING-v1`, which that release
    // replaced with `TAG_CHALLENGE` while also domain-separating the view commitments and the
    // PRG — a change a copied constant would have survived silently, re-deriving a challenge
    // sequence that no honest proof could ever match.
    absorb_framed(&mut challenge_hasher, TAG_CHALLENGE, binding);
    for (i, leaf) in leaves.iter().enumerate() {
        if leaf.version != LEAF_JOURNAL_VERSION {
            return Err(AggregateError::LeafVersionMismatch(i));
        }
        if leaf.position as usize != i {
            return Err(AggregateError::PositionMismatch { at: i, found: leaf.position });
        }
        if &leaf.statement_digest != statement_digest {
            return Err(AggregateError::StatementMismatch(i));
        }
        if leaf.party > 2 {
            return Err(AggregateError::PartyOutOfRange { at: i, found: leaf.party });
        }
        for commitment in &leaf.commitments {
            challenge_hasher.update(commitment.digest.as_ref());
            commitment.output_share.update_hasher(&mut challenge_hasher);
        }
    }
    let challenge_entropy = challenge_hasher.finalize();
    let mut challenge_generator =
        ChallengeGenerator::new(HashPRG::<H>::new(challenge_entropy.as_ref()));
    for (i, leaf) in leaves.iter().enumerate() {
        let expected = challenge_generator.next().index() as u8;
        if expected != leaf.party {
            return Err(AggregateError::ChallengeMismatch {
                at: i,
                expected,
                found: leaf.party,
            });
        }
    }
    return Ok(());
}

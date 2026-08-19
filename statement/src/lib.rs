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
/// word features are enabled, which cargo resolves independently for the host workspace
/// and the guest's own workspace. The word features are therefore pinned explicitly in
/// this crate's manifest, and any change to them — or to the shape of [`LiftInput`] —
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

/// The journal committed by the cycle-probe guest.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProbeJournal {
    /// Whether the ZKBoo proof verified.
    pub valid: bool,
    /// Number of responses (repetitions) in the verified proof.
    pub num_responses: u32,
    /// Guest cycles spent inside `zkboo::verifier::verify` alone.
    pub verify_cycles: u64,
}

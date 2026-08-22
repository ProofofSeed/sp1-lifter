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
use snark_lifter_statement::LiftProof;

/// The crypto suite, re-exported so consumers of this statement need not also depend on
/// the production statement crate: the two must use the same suite or nothing composes.
pub use snark_lifter_statement::{H, PS, PV, S};
use zkboo::{
    backend::{Backend, Frontend},
    circuit::Circuit,
    word::Words,
};
use zkboo_bip32::{be_bytes_to_word, ethereum_address};

/// Byte length of a secp256k1 private key.
pub const PRIV_KEY_LEN: usize = 32;

/// Ethereum-address derivation as a ZKBoo circuit.
///
/// No public parameters: the witness is the private key and the address is the output.
pub struct EthereumAddressCircuit {
    private_key: Vec<u8>,
}

impl EthereumAddressCircuit {
    /// Creates the circuit with a genuine witness, for proving.
    pub fn with_witness(private_key: Vec<u8>) -> Self {
        assert_eq!(private_key.len(), PRIV_KEY_LEN, "private key must be 32 bytes");
        return Self { private_key };
    }

    /// Creates the circuit for verifying: the replay backend sources input values from the
    /// opened views, so only the number of allocated inputs matters here.
    pub fn public_only() -> Self {
        return Self {
            private_key: alloc::vec![0u8; PRIV_KEY_LEN],
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
        ethereum_address(frontend, d)
            .into_iter()
            .for_each(|w| frontend.output(w));
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

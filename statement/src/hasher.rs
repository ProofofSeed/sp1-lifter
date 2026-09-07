// SPDX-License-Identifier: LGPL-3.0-or-later

//! Keccak-256 as a [`Hasher`]: the original Keccak (padding byte `0x01`), not NIST SHA3-256.
//!
//! Copied from `zkboo-harness`, which is where `zkboo` 1.2.0 moved the implementation when it
//! removed its `keccak` feature and stopped shipping any concrete `Hasher` at all. That crate
//! is `publish = false` and pulls in `std`, so a `no_std` guest cannot depend on it, and the
//! choice of hash belongs to the consumer in any case. The bytes are unchanged, so digests
//! match every other Keccak-256 consumer of the ecosystem.

use core::fmt::{self, Debug};

use tiny_keccak::{Hasher as _, Keccak};
use zeroize::Zeroize;
use zkboo::crypto::Hasher;

/// A [`Hasher`] backed by Keccak-256 (Ethereum's `keccak256`), producing 32-byte digests.
pub struct Keccak256Hasher {
    inner: Keccak,
}

impl Debug for Keccak256Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return f.write_str("Keccak256Hasher");
    }
}

impl Hasher for Keccak256Hasher {
    type Digest = [u8; 32];
    const DIGEST_SIZE: usize = 32;

    fn new() -> Self {
        return Self {
            inner: Keccak::v256(),
        };
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finalize_into(&mut self, out: &mut Self::Digest) {
        // `Keccak::finalize` consumes the sponge; swap in a fresh state so the hasher stays
        // reusable after finalization (as the [`Hasher`] contract requires).
        let inner = core::mem::replace(&mut self.inner, Keccak::v256());
        inner.finalize(out);
    }
}

impl Zeroize for Keccak256Hasher {
    fn zeroize(&mut self) {
        // Drop the current sponge state by replacing it with a fresh one.
        self.inner = Keccak::v256();
    }
}

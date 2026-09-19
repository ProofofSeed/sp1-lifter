// SPDX-License-Identifier: LGPL-3.0-or-later

//! The leaf verifying keys the root programs compile in.
//!
//! # Why a root pins its leaf key rather than taking it as input
//!
//! A root admits leaf proofs through SP1's proof-composition syscall, which is addressed by a
//! verifying-key digest. If the root takes that digest as an *input*, then the root's own
//! verifying key says nothing about which leaf program it admitted: two roots with identical
//! keys can attest leaves from entirely different programs — one of which might skip the
//! well-formedness check, or replay a different circuit. The journal names the key it used, so
//! a relying party *can* check it, which is the problem: it is a check that can be
//! forgotten, sitting outside everything `check_journal` covers.
//!
//! Compiling the digest in moves the check inside the root's own verifying key. Verifying a
//! root proof against the expected root key then says which leaf program produced the leaves,
//! and there is nothing left for a consumer to forget.
//!
//! # Why this is its own crate
//!
//! It must be linked by the root programs and the host, and **not** by the leaf programs. A
//! leaf that linked it would change its own verifying key whenever that key was recorded here,
//! which does not converge. Keeping it out of `pofseed-sp1-lift-split` — which the leaves do link —
//! is what makes the two-stage build terminate: build the leaf, read its key, record it here,
//! build the root.
//!
//! # Maintenance
//!
//! A constant checked in by hand is a constant that can go stale, and a stale one fails at
//! SP1's artifact lookup with a message about a missing artifact rather than about a wrong key.
//! `cargo run --bin vkeys` recomputes both keys from the leaf ELFs and refuses if either has
//! drifted, printing the correct value. Run it after anything that touches a leaf program or
//! what it links.

// Re-derived 2026-09-19 against zkboo 1.3.0, whose BIP-32 derivations assert the validity
// conditions BIP-32 states and whose `verify` requires the repetition count to be named. Both
// keys moved, as does anything a guest links — a crate's version alone is enough. `vkeys` is what
// produced them and what refuses if they drift again.

/// The verifying key of `program-leaf`, the hardened-child leaf.
pub const LEAF_VK: [u32; 8] = [
    0x3179ac07, 0x455c0db2, 0x33b6cd53, 0x4476f937, 0x559f2bf2, 0x61c0b1ce, 0x5be0157c, 0x701d7e1f,
];

/// The verifying key of `program-leaf-elephant`, the Ethereum-address leaf.
pub const LEAF_ELEPHANT_VK: [u32; 8] = [
    0x02f7b8f2, 0x4d395188, 0x0c778374, 0x0379c9eb, 0x67af8e5f, 0x286b32c5, 0x71ee3c73, 0x579396af,
];

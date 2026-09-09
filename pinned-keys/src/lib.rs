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
//! which does not converge. Keeping it out of `sp1-lifter-split` — which the leaves do link —
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

// Re-derived 2026-09-07 against zkboo 1.2.0 at the tip of the local trees, after the assertion,
// advice and proof-format rework. Both moved; the previous values belonged to the withdrawn 2.0.0
// line. `vkeys` is what produced them and what refuses if they drift again.

/// The verifying key of `program-leaf`, the hardened-child leaf.
pub const LEAF_VK: [u32; 8] = [
    0x0c330b47, 0x0a902c06, 0x48f18375, 0x50840bde, 0x337caee5, 0x4aa137a4, 0x4a166ba0, 0x36104263,
];

/// The verifying key of `program-leaf-elephant`, the Ethereum-address leaf.
pub const LEAF_ELEPHANT_VK: [u32; 8] = [
    0x2e310e62, 0x0159baa0, 0x108edcdb, 0x62577359, 0x5263c7b0, 0x43fddafc, 0x4b14891b, 0x2ed1c162,
];

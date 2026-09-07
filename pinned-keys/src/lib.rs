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
//! a relying party *can* check it, and that is exactly the problem: it is a check that can be
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
//! # Keeping it honest
//!
//! A constant checked in by hand is a constant that can go stale, and a stale one fails at
//! SP1's artifact lookup with a message about a missing artifact rather than about a wrong key.
//! `cargo run --bin vkeys` recomputes both keys from the leaf ELFs and refuses if either has
//! drifted, printing the correct value. Run it after anything that touches a leaf program or
//! what it links.

// 🚨 BOTH VALUES BELOW ARE STALE, KNOWINGLY, as of the move to zkboo 1.2.0.
//
// That release changes the circuits both leaves link — assertions became ordinary gates, and the
// affine comb's advice became ordinary inputs — so both keys have moved and neither has been
// recomputed yet, because doing so means building the guests. `cargo run --bin vkeys` refuses
// while they disagree and prints the values to put here; nothing else in the workspace will
// produce a proof that verifies until it does. The staleness is loud rather than silent, which
// is the arrangement this crate exists to keep.

/// The verifying key of `program-leaf`, the hardened-child leaf.
pub const LEAF_VK: [u32; 8] = [
    0x55123cc9, 0x7c91e427, 0x63539ee8, 0x2f975740, 0x076e89c9, 0x0aac5262, 0x1b87f9ea, 0x0c92a711,
];

/// The verifying key of `program-leaf-elephant`, the Ethereum-address leaf.
pub const LEAF_ELEPHANT_VK: [u32; 8] = [
    0x18cefb20, 0x21e2d458, 0x5c534b80, 0x20fea17f, 0x1823fa72, 0x24cad652, 0x2f91acb8, 0x13cc4763,
];

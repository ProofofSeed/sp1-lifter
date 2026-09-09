// SPDX-License-Identifier: LGPL-3.0-or-later

//! What the two lifted statements cost, in each of the three units they can be counted in.
//!
//! The statements are ~50× apart by one measure and ~260× apart by another, so a ratio between
//! them is undefined until the unit is named.
//!
//! The three units, and what each one is the right denominator for:
//!
//! - **AND-message words** — the nonlinear cost, and the only one that shows up in proof *bytes*.
//!   It is the unit this project's cost model is built around, which is exactly why it gets
//!   reached for when it is not the operative one: field arithmetic is overwhelmingly linear, so
//!   it says nothing about work that touches every gate.
//! - **Machine words** — `zkboo-profiling` sums `+= N` per operation, so a `CompositeWord<W, N>`
//!   counts N. The right denominator for a mechanism that touches every storage bit.
//! - **Fingerprint bytes** — what `zkboo-circuit-hash` actually absorbs while traversing the
//!   circuit. 🪤 This is *not* a per-operation count, though it is easy to describe as one: each
//!   operation ingests a fixed header (opcode, word width, composite width) and then **16·N bytes
//!   per index operand**, so it is a blend of the two units above. It is reported because it is
//!   not a proxy for anything — it is the work the guest's fixed term literally performs.
//!
//! Measured with a `Hasher` that counts the bytes it absorbs, which needs no change to any zkboo
//! crate: the trait is small and the choice of hash belongs to the caller.

use core::sync::atomic::{AtomicU64, Ordering};

use pofseed_sp1_lift_elephant::EthereumAddressCircuit;
use pofseed_sp1_lift_host::{HARDENED_OFFSET, bip32_vector_1};
use pofseed_sp1_lift_statement::HardenedChildCircuit;
use zkboo::{circuit::Circuit, crypto::Hasher};
use zkboo_profiling::{GateCounts, profile};

/// Bytes absorbed since the last reset, across every instance of [`CountingHasher`].
///
/// A static rather than a field because `hash_circuit` constructs its own hasher and returns only
/// a digest, so there is no instance to inspect afterwards; counting globally also survives an
/// implementation that uses more than one.
static ABSORBED: AtomicU64 = AtomicU64::new(0);

/// A [`Hasher`] that computes nothing and counts what it is given.
#[derive(Debug, Default)]
struct CountingHasher;

impl zeroize::Zeroize for CountingHasher {
    fn zeroize(&mut self) {}
}

impl Hasher for CountingHasher {
    type Digest = [u8; 32];
    const DIGEST_SIZE: usize = 32;

    fn new() -> Self {
        return CountingHasher;
    }

    fn update(&mut self, data: &[u8]) {
        ABSORBED.fetch_add(data.len() as u64, Ordering::Relaxed);
    }

    fn finalize_into(&mut self, out: &mut Self::Digest) {
        *out = [0u8; 32];
    }
}

/// Bytes the circuit fingerprint absorbs over one traversal of `circuit`.
fn fingerprint_bytes<C: Circuit>(circuit: &C) -> u64 {
    ABSORBED.store(0, Ordering::Relaxed);
    let _ = zkboo_circuit_hash::hash_circuit::<_, CountingHasher>(circuit);
    return ABSORBED.load(Ordering::Relaxed);
}

/// Every gate count summed, in machine words.
fn machine_words(g: &GateCounts) -> u64 {
    // `cast` is shaped by word-type *pair* rather than by word type, so it is folded in separately
    // rather than skipped: leaving a term out of a total that is then used as a denominator is the
    // same class of error this binary exists to stop.
    let mut total: usize = 0;
    for shape in [
        &g.input, &g.alloc, &g.constant, &g.from_le_words, &g.to_le_words, &g.output, &g.not,
        &g.bitxor, &g.bitand, &g.bitxor_const, &g.bitand_const, &g.unbounded_shl, &g.unbounded_shr,
        &g.rotate_left, &g.rotate_right, &g.reverse_bits, &g.swap_bytes, &g.carry,
    ] {
        total += shape.sum();
    }
    total += g.cast.reduce(|acc, s| acc + s.sum(), 0usize);
    return total as u64;
}

fn row(name: &str, and_words: u64, words: u64, fp_bytes: u64) {
    println!("  {name:<22} {and_words:>12} {words:>14} {fp_bytes:>16}");
}

fn main() {
    let (_, chain_code) = bip32_vector_1();
    let child = HardenedChildCircuit::public_only(chain_code, HARDENED_OFFSET);
    let elephant = EthereumAddressCircuit::public_only();

    let child_prof = profile(&child);
    let elephant_prof = profile(&elephant);
    let child_and = child_prof.and_msg_size().sum() as u64;
    let elephant_and = elephant_prof.and_msg_size().sum() as u64;
    let child_words = machine_words(child_prof.gate_counts());
    let elephant_words = machine_words(elephant_prof.gate_counts());
    let child_fp = fingerprint_bytes(&child);
    let elephant_fp = fingerprint_bytes(&elephant);

    println!("── statement cost, in three units ──");
    println!("  zkboo: {}", pofseed_sp1_lift_host::zkboo_provenance());
    println!();
    println!("  {:<22} {:>12} {:>14} {:>16}", "", "AND words", "machine words", "fingerprint B");
    row("hardened child", child_and, child_words, child_fp);
    row("ethereum address", elephant_and, elephant_words, elephant_fp);
    println!();
    println!(
        "  ratio                  {:>11.1}x {:>13.1}x {:>15.1}x",
        elephant_and as f64 / child_and as f64,
        elephant_words as f64 / child_words as f64,
        elephant_fp as f64 / child_fp as f64,
    );
    println!();
    // The fixed term of each statement's guest instruction law is the in-guest fingerprint
    // traversal, so its ratio is a measured quantity that the fingerprint-byte ratio predicts.
    // Printed together: reading either alone has produced wrong attributions.
    println!("  measured guest fixed-term ratio (elephant 5.0986e9 / child 3.147e7): 162.0x");
    println!("  ↑ compare with the fingerprint-byte ratio above, which is what that term does.");
}

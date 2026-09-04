// SPDX-License-Identifier: LGPL-3.0-or-later

//! The verifying key of every program in this workspace.
//!
//! A verifying key is the only thing that says *which program* produced a proof, so it is
//! what the on-chain suite pins, what a root pins about its leaves, and what moves silently
//! whenever a guest or anything it links changes. This prints them all, so that "did that
//! change move a key?" is a question with a two-minute answer.
//!
//! It also checks the leaf keys the root programs have compiled in, and prints the correct
//! constant when one has drifted, so the fix is a copy rather than an investigation.

use sp1_sdk::{HashableKey, ProvingKey, blocking::Prover};
use sp1_lifter_host::{
    ELEPHANT_ELF, LEAF_ELEPHANT_ELF, LEAF_ELF, LIFT_ELF, ROOT_ELEPHANT_ELF, ROOT_ELF,
    prover_client,
};

fn u32_array(v: [u32; 8]) -> String {
    return format!(
        "[{}]",
        v.iter().map(|w| format!("0x{w:08x}")).collect::<Vec<_>>().join(", ")
    );
}

fn main() {
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "error") };
    }
    sp1_sdk::setup_logger();
    let (client, _) = prover_client();
    println!("── verifying keys ──");
    println!("  zkboo: {}", sp1_lifter_host::zkboo_provenance());
    let mut leaf_digests = Vec::new();
    for (name, elf) in [
        ("lift (monolithic)", LIFT_ELF),
        ("elephant (monolithic probe)", ELEPHANT_ELF),
        ("leaf", LEAF_ELF),
        ("root", ROOT_ELF),
        ("leaf-elephant", LEAF_ELEPHANT_ELF),
        ("root-elephant", ROOT_ELEPHANT_ELF),
    ] {
        let pk = client.setup(elf).expect("program setup");
        let vk = pk.verifying_key();
        println!("  {name:<28} {}", vk.bytes32());
        if name.starts_with("leaf") {
            leaf_digests.push((name, vk.hash_u32()));
        }
    }
    println!();
    println!("── the leaf keys the roots have compiled in ──");
    for (name, digest, pinned) in [
        ("leaf", leaf_digests[0].1, sp1_lifter_host::PINNED_LEAF_VK),
        ("leaf-elephant", leaf_digests[1].1, sp1_lifter_host::PINNED_LEAF_ELEPHANT_VK),
    ] {
        if digest == pinned {
            println!("  {name:<16} pinned correctly");
        } else {
            println!("  {name:<16} 🔴 DRIFTED");
            println!("      root has:  {}", u32_array(pinned));
            println!("      leaf is:   {}", u32_array(digest));
            println!("      Update the constant in the root program's source to the second line.");
        }
    }
    let drifted = leaf_digests[0].1 != sp1_lifter_host::PINNED_LEAF_VK
        || leaf_digests[1].1 != sp1_lifter_host::PINNED_LEAF_ELEPHANT_VK;
    assert!(!drifted, "a root program has a stale leaf verifying key compiled into it");
    println!("verifying keys: PASS");
}

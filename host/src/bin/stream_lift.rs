// SPDX-License-Identifier: LGPL-3.0-or-later

//! A lift that starts before the proof exists.
//!
//! The monolithic lifter reads a whole ZKBoo proof and verifies it in one zkVM program, so
//! the device's streaming time and the lifter's proving time add up. This drives the split
//! as a pipeline instead: a leaf proof is dispatched the moment its response is available,
//! and the root fires when the last leaf lands. Nothing in SP1, `zkboo` or the contract
//! changes; a leaf is an ordinary `prove` call whose input is complete on arrival.
//!
//! The root's output is the same `LiftJournal` the monolithic lifter commits, so it goes
//! through the same relying-party gate, and this driver runs that gate rather than trusting
//! the pipeline that produced it.
//!
//! Usage: `stream-lift [n] [pool] [arrival-interval-seconds]`
//! (default: 8 responses, 1 prover, no modelled interval).
//!
//! ⚠️ Without an arrival interval this machine produces a hardened-child response in
//! milliseconds, where a Ledger Flex takes about thirty seconds. The pipeline is then
//! starved and the overlap it reports is near zero — correctly, because there is nothing to
//! overlap. Pass an interval to model the device before reading anything into the ratio.
//!
//! ⚠️ One SP1 prover peaks near 59 GB on this machine, so a pool above 2 does not fit in
//! 137 GB. The pool stands in for machines, not for cores.

use std::time::{Duration, Instant};

use sp1_sdk::{HashableKey, ProvingKey, SP1Proof, SP1Stdin, blocking::{ProveRequest, Prover}};
use snark_lifter_host::{
    HARDENED_OFFSET, LEAF_ELF, ROOT_ELF, bip32_vector_1, decode_root_journal,
    orchestrator::{StreamReport, hardened_child_statement, stream_lift},
    prover_backend, prover_client,
};
use snark_lifter_split::RootInput;
use snark_lifter_statement::{JournalExpectation, REQUIRED_PROOF_FORMAT_VERSION, check_journal};

const SEED_ENTROPY: &[u8] = b"snark-lifter streaming orchestrator entropy";
const BINDING: &[u8] = b"snark-lifter streaming orchestrator tx binding";

fn secs(d: Duration) -> String {
    return format!("{:.2}s", d.as_secs_f64());
}

fn report(r: &StreamReport) {
    println!("── timeline (n={}, pool={}) ──", r.num_responses, r.pool);
    println!("  challenge phase (global, before any response can exist): {}", secs(r.challenge_wall));
    println!("  last response finished by the producer:                   {}", secs(r.last_produced));
    println!("  leaf proving, summed:                                    {}", secs(r.leaf_total()));
    println!("  root:                                                    {}", secs(r.root_wall));
    println!("  end to end:                                              {}", secs(r.end_to_end));
    println!();
    println!("  provers idle, waiting for a response to exist:            {}", secs(r.pool_idle));
    println!("  finished responses queued, waiting for a prover:          {}", secs(r.queue_delay));
    println!(
        "  ⇒ the {} side is the bottleneck",
        if r.pool_idle > r.queue_delay { "producing" } else { "proving" },
    );
    println!();
    println!("── what the pipeline is worth ──");
    if r.queue_delay > r.pool_idle {
        println!(
            "  ⚠️ proving is the bottleneck, so the overlap below is bounded by how fast leaves"
        );
        println!(
            "     are proved, not by how fast responses arrive. With {} prover(s) against a leaf",
            r.pool,
        );
        println!(
            "     of ~{}, one machine keeps pace only with an arrival interval above that.",
            secs(r.leaf_total() / r.num_responses as u32),
        );
    }
    println!("  produced, then lifted (no overlap):                       {}", secs(r.serial_counterfactual()));
    println!("  streamed (this run):                                      {}", secs(r.end_to_end));
    println!(
        "  overlap saved:                                            {} ({:.2}x)",
        secs(r.serial_counterfactual().saturating_sub(r.end_to_end)),
        r.serial_counterfactual().as_secs_f64() / r.end_to_end.as_secs_f64(),
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().map(|a| a.parse().expect("n must be a positive integer")).unwrap_or(8);
    let pool: usize = args.next().map(|a| a.parse().expect("pool must be a positive integer")).unwrap_or(1);
    let arrival_interval: Option<Duration> = args
        .next()
        .map(|a| Duration::from_secs_f64(a.parse().expect("arrival interval must be seconds")));
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "error") };
    }
    sp1_sdk::setup_logger();
    let (client, config) = prover_client();
    assert_ne!(
        prover_backend(),
        "mock",
        "refusing to run under SP1_PROVER=mock: a mock proof would pass the gate having proved nothing"
    );
    println!(
        "── streamed lift (n={n}, pool={pool}, arrival interval {}) ──",
        match arrival_interval {
            Some(d) => format!("{:.1}s (modelled device cadence)", d.as_secs_f64()),
            None => "none (as fast as this machine produces them)".to_string(),
        }
    );
    println!("  prover config: {config}");
    let (parent_priv, chain_code) = bip32_vector_1();
    let (circuit, statement) =
        hardened_child_statement(chain_code, parent_priv, HARDENED_OFFSET, BINDING);

    let leaf_pk = client.setup(LEAF_ELF).expect("leaf program setup");
    let leaf_vk = leaf_pk.verifying_key().clone();
    // The root's proving key is built AFTER the leaves are proved, deliberately. Setting it
    // up first cost 55% on every leaf — 88.6 s against 57.0 s for the identical proof in
    // `split-probe` — which is a big enough regression to be worth the awkward ordering.

    let (mut r, leaf_proofs) = stream_lift(
        &client, &circuit, &statement, n, SEED_ENTROPY, pool, arrival_interval, &leaf_pk, &leaf_vk,
    );

    // The root, over the leaves in response order. The orchestrator sorted them; the
    // cross-leaf gate inside the root is what catches it if that sort were ever wrong.
    let mut root_stdin = SP1Stdin::new();
    let mut leaf_public_values = Vec::with_capacity(n);
    for (i, proof) in leaf_proofs.into_iter().enumerate() {
        leaf_public_values.push(proof.public_values.as_slice().to_vec());
        let SP1Proof::Compressed(compressed) = proof.proof else {
            panic!("leaf {i} is not compressed");
        };
        root_stdin.write_proof(*compressed, leaf_vk.vk.clone());
    }
    let root_pk = client.setup(ROOT_ELF).expect("root program setup");
    let root_input = RootInput {
        version: snark_lifter_split::ROOT_FRAME_VERSION,
        chain_code: statement.chain_code.clone(),
        index: statement.index,
        expected_output: statement.expected_output.clone(),
        binding: statement.binding.clone(),
        leaf_vk: leaf_vk.hash_u32(),
        leaf_public_values,
    };
    root_stdin.write_vec(postcard::to_allocvec(&root_input).expect("serialize root frame"));
    let t = Instant::now();
    let root_proof = client
        .prove(&root_pk, root_stdin)
        .compressed()
        .run()
        .expect("root proving");
    r.root_wall = t.elapsed();
    r.end_to_end += r.root_wall;

    // The gate, not the pipeline, is what says this attests anything.
    client.verify(&root_proof, root_pk.verifying_key(), None).expect("root verification");
    let journal = decode_root_journal(root_proof.public_values.as_slice()).expect("decode root journal");
    let expectation = JournalExpectation {
        fingerprint: journal.lift.fingerprint,
        chain_code: statement.chain_code.clone(),
        index: statement.index,
        expected_output: statement.expected_output.clone(),
        binding: statement.binding.clone(),
        min_responses: n as u32,
        proof_format_version: REQUIRED_PROOF_FORMAT_VERSION,
    };
    check_journal(&journal.lift, &expectation).expect("root journal must pass the relying-party gate");
    assert_eq!(journal.leaf_vk, leaf_vk.hash_u32(), "root attests a different leaf key");
    assert_eq!(journal.lift.num_responses, n as u32);
    println!("  root journal passed the relying-party gate ({} responses)", journal.lift.num_responses);
    println!();
    report(&r);
    println!("streamed lift: PASS");
}

// SPDX-License-Identifier: LGPL-3.0-or-later

//! Split-and-aggregate probe: what does one-proof-per-response actually cost?
//!
//! The production lifter verifies all responses in a single zkVM program, so its proving
//! time is one indivisible run. The alternative shape — a leaf proof per response, a root
//! proof that re-derives the Fiat-Shamir challenge over all of them — is what makes the
//! work distributable across machines, and is also what makes it *streamable*: a leaf can
//! be proved the moment its response arrives. This driver measures the price of that
//! shape against the monolithic one at the same repetition count.
//!
//! Three things are measured and must be reported separately, because they answer
//! different questions:
//!
//! - **sequential total** (all leaves + root on one machine) — the cost of the split. It
//!   is expected to be *worse* than monolithic; the question is by how much.
//! - **critical path** (slowest leaf + root) — the wall time with unlimited machines.
//! - **root cost alone** — the part that never distributes, and therefore the floor.
//!
//! Usage: `split-probe [n] [both|mono|split|gate|leaves|root|forge]`
//! (default: 4 responses, both). `gate` runs only the cross-leaf refusal matrix, which
//! needs no proving.
//!
//! Leaf proofs are expensive and reusable, so `leaves` proves and caches them under
//! `fixtures/leaves/n<N>/` and `root` measures a root over the `n` cached leaves. That
//! separation is what makes a root cost law over a wide range of `n` affordable: the
//! leaves are proved once, and every root point after that is minutes rather than hours.
//! The cache is per response count because a leaf is not portable between counts — see
//! the comment at the cache path.
//! The `forge-*` modes re-run the root over cached leaves with the leaf/proof
//! correspondence broken, and must FAIL:
//!
//! - `forge-vk`    — the root claims a corrupted leaf verifying key.
//! - `forge-count` — the root claims `n` leaves but supplies `n-1` proofs.
//! - `forge-order` — the proofs are supplied in an order the claims do not match.
//!
//! ⚠️ **A failure is not automatically the failure being tested, and the prove-time error
//! cannot tell you which failure it was.** All three surface as "artifact not found", so each
//! mode also *executes* the same frame and asks the guest directly. That distinguishes them:
//!
//! - `forge-vk` is refused **by the root guest itself**, at the pinned-key assertion, before
//!   any deferred proof is looked up. Its prove-time error still says "artifact not found" —
//!   downstream wreckage of the failed run, not the refuser.
//! - `forge-count` and `forge-order` are **invisible to the guest** by construction: they
//!   change only which proofs are supplied alongside unchanged claims. Something downstream of
//!   the guest refuses them, and **this harness cannot say what.**
//!
//! 🔴 **Corrected 2026-09-08, and the correction is the point.** These two cases were reported
//! as "refused by SP1's host-side deferred-proof bookkeeping". That was an inference from "the
//! guest accepted", and it does not follow — "not the guest" does not name a refuser. Two facts
//! killed it:
//!
//! - **The observable is identical in a case where the refuser is known.** All three modes end
//!   with the same recursion-VM fatal, `attempted to perform base field division N/0` in a
//!   `DivF` instruction inside a Reduction task, followed by the same `artifact not found`.
//!   `forge-vk` produces it too — and there the guest demonstrably panics first. An error that
//!   appears when the guest is the refuser cannot be evidence that the guest was not.
//! - **The host does not do the bookkeeping it was credited with.** In 6.4.0,
//!   `sp1_prover::worker::controller::deferred::DeferredInputs::new` walks `stdin.proofs` in
//!   order and folds them with `hash_deferred_proofs`; it never matches them against the
//!   guest's claims. The matching is the compress circuit's, against the digest the guest
//!   committed.
//!
//! So the honest statement is: **the guest is not the refuser, the refusal happens inside the
//! recursion layer, and which mechanism inside it — the deferred-digest constraint, or a
//! degenerate witness crashing the executor before any constraint has a verdict — is not
//! distinguished by anything this harness can see.** It may well be exercising the in-circuit
//! binding; it may equally be crashing short of it. Neither reading is licensed.
//!
//! A deployment must therefore not read these passes as evidence about the in-circuit binding.
//! The evidence for that binding is a reading of the 6.4.0 sources, and separating the two
//! would need SP1 to report a constraint verdict where it currently reports a division.

use std::time::{Duration, Instant};

use sp1_sdk::{HashableKey, ProvingKey, SP1Proof, SP1Stdin, blocking::{ProveRequest, Prover}};
use sp1_lifter_host::{
    HARDENED_OFFSET, LEAF_ELF, LIFT_ELF, ROOT_ELF, bip32_vector_1, decode_root_journal,
    expectation_for, leaf_input_for, leaf_journal_native, leaf_stdin, lift_input_for, lift_stdin,
    root_input_for, vector_1_proof,
};
use sp1_lifter_split::{AggregateError, LeafJournal, aggregate, statement_digest};
use sp1_lifter_statement::{LiftInput, check_journal};

const SEED_ENTROPY: &[u8] = b"snark-lifter split probe entropy";
const BINDING: &[u8] = b"snark-lifter split probe tx binding";

fn expect_refusal(name: &str, leaves: &[LeafJournal], digest: &sp1_lifter_statement::S, expected: fn(&AggregateError) -> bool) {
    match aggregate(leaves, digest, BINDING) {
        Ok(()) => panic!("{name}: the gate ACCEPTED a leaf set it must refuse"),
        Err(e) if expected(&e) => println!("  {name}: refused ({}) ✓", e.tag()),
        Err(e) => panic!("{name}: refused for the wrong reason: {}", e.tag()),
    }
}

/// The smallest leaf count at which the refusal matrix means anything.
///
/// The challenge-based cases refuse because a re-derived party sequence stops matching the
/// declared one — and a party is one of three, so a tampered set of `k` leaves can still
/// match **by chance with probability 3^-k**. At n = 2 that is one in three, and
/// `split-probe 2 gate` duly reports G1 *accepting* a set it must refuse: not a defect in
/// the gate but the gate's soundness being probabilistic, exactly as ZKBoo's own is.
///
/// At 24 leaves a chance pass is below 3e-12, which is small enough that a green matrix is
/// evidence rather than a coin toss. The matrix costs no proving, so running it at this
/// floor regardless of the proving count is free.
const MIN_GATE_LEAVES: usize = 24;

/// The cross-leaf refusal matrix, on real leaf journals, without any proving.
///
/// Each case is a way of assembling leaves that must not yield a lifted statement. The
/// challenge-mismatch cases are the load-bearing ones: they are what stops 219 separate
/// proofs from being reassembled into a claim their prover never earned.
fn gate_matrix(input: &LiftInput, n: usize) {
    println!("── cross-leaf gate matrix (n={n}, no proving) ──");
    let leaves: Vec<LeafJournal> = (0..n).map(|i| leaf_journal_native(input, i)).collect();
    let digest = statement_digest(&input.chain_code, input.index, &input.expected_output, &input.binding);
    aggregate(&leaves, &digest, BINDING).expect("control: the genuine leaf set must aggregate");
    println!("  control (all {n} leaves, in order): accepted ✓");

    // G1: a dropped response. The challenge is a hash over the commitments of exactly the
    // responses present, so removing one re-derives an entirely different sequence.
    let mut truncated = leaves.clone();
    truncated.pop();
    expect_refusal("G1 dropped response", &truncated, &digest, |e| {
        matches!(e, AggregateError::ChallengeMismatch { .. })
    });

    // G2: a substituted response — leaf 1 replaced by a copy of leaf 0's commitments,
    // keeping its position. Structurally impeccable, cryptographically dead.
    let mut substituted = leaves.clone();
    substituted[1].commitments = leaves[0].commitments.clone();
    substituted[1].party = leaves[0].party;
    expect_refusal("G2 substituted response", &substituted, &digest, |e| {
        matches!(e, AggregateError::ChallengeMismatch { .. })
    });

    // G3b: a mislabelled leaf — content correct for its slot, `position` field wrong.
    // This is the ONLY case the position check is load-bearing for: G3 below is refused
    // by the challenge check too, since duplicating a response changes the transcript.
    // The position field never enters the challenge hash, so nothing else constrains it.
    let mut mislabelled = leaves.clone();
    mislabelled[1].position = 99;
    expect_refusal("G3b mislabelled position", &mislabelled, &digest, |e| {
        matches!(e, AggregateError::PositionMismatch { .. })
    });

    // G3: a duplicated response — the same leaf twice, which would let one response's
    // work be claimed as n responses' soundness.
    let mut duplicated = leaves.clone();
    duplicated[1] = leaves[0].clone();
    expect_refusal("G3 duplicated response", &duplicated, &digest, |e| {
        matches!(e, AggregateError::PositionMismatch { .. })
    });

    // G4: reordered responses. Order is part of the transcript, not a convention.
    let mut reordered = leaves.clone();
    reordered.swap(0, 1);
    reordered[0].position = 0;
    reordered[1].position = 1;
    expect_refusal("G4 reordered responses", &reordered, &digest, |e| {
        matches!(e, AggregateError::ChallengeMismatch { .. })
    });

    // G5: a leaf proved against a different statement, aggregated into this one.
    let mut foreign = leaves.clone();
    foreign[1].statement_digest[0] ^= 1;
    expect_refusal("G5 foreign statement", &foreign, &digest, |e| {
        matches!(e, AggregateError::StatementMismatch(_))
    });

    // G6: a tampered commitment — one bit of one digest.
    let mut tampered = leaves.clone();
    tampered[2 % n].commitments[0].digest[0] ^= 1;
    expect_refusal("G6 tampered commitment", &tampered, &digest, |e| {
        matches!(e, AggregateError::ChallengeMismatch { .. })
    });

    // G7: no leaves at all. The empty ZKBoo proof verifies by design, so an empty
    // aggregation must not be an implicit success.
    expect_refusal("G7 empty leaf set", &[], &digest, |e| {
        matches!(e, AggregateError::NoLeaves)
    });

    // G8: the root asked to attest a statement none of the leaves was proved against.
    let mut other = digest;
    other[0] ^= 1;
    expect_refusal("G8 wrong root statement", &leaves, &other, |e| {
        matches!(e, AggregateError::StatementMismatch(_))
    });

    println!("cross-leaf gate matrix: PASS");
    println!();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args
        .next()
        .map(|a| a.parse().expect("repetition count must be a positive integer"))
        .unwrap_or(4);
    assert!(n >= 2, "the probe needs at least two responses to say anything");
    let mode = args.next().unwrap_or_else(|| "both".to_string());
    assert!(
        matches!(
            mode.as_str(),
            "both" | "mono" | "split" | "gate" | "leaves" | "root"
                | "forge-vk" | "forge-count" | "forge-order"
        ),
        "mode must be one of both|mono|split|gate|leaves|root|forge-vk|forge-count|forge-order, \
         got `{mode}`"
    );
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "info") };
    }
    sp1_sdk::setup_logger();
    let (client, config) = sp1_lifter_host::prover_client();
    assert_ne!(
        sp1_lifter_host::prover_backend(),
        "mock",
        "a mock proof carries no evidence: this probe's timings would be meaningless"
    );
    println!("── split-and-aggregate probe (n={n}, mode={mode}) ──");
    println!("  prover config: {config}");
    let t = Instant::now();
    let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
    println!("  proved {n} ZKBoo responses natively in {:.2?}", t.elapsed());
    let (_, chain_code) = bip32_vector_1();
    let input = lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof);

    // Wire sizes, measured rather than assumed: the per-response figure sets how fast the
    // secure element can deliver responses, and hence how much leaf throughput is needed
    // to keep pace with it.
    let lift_frame = postcard::to_allocvec(&input).expect("serialize lift frame").len();
    let leaf_frames: Vec<usize> = (0..n)
        .map(|i| postcard::to_allocvec(&leaf_input_for(&input, i)).expect("serialize leaf frame").len())
        .collect();
    let leaf_frame_max = *leaf_frames.iter().max().expect("at least one leaf");
    let statement_overhead = leaf_frame_max
        - (lift_frame - postcard::to_allocvec(&{
            let mut empty = lift_input_for(input.chain_code.clone(), input.index, input.expected_output.clone(), &input.binding, Vec::new());
            empty.version = input.version;
            empty
        }).expect("serialize empty frame").len()) / n;
    println!(
        "  wire: lift frame {lift_frame} B for {n} responses ⇒ {:.1} KiB/response; \
         leaf frame ≤ {leaf_frame_max} B (statement overhead ~{statement_overhead} B)",
        (lift_frame as f64 / n as f64) / 1024.0,
    );

    // The matrix runs at its own count, never the proving count: it costs no proving, and
    // below MIN_GATE_LEAVES a tampered set can satisfy the challenge check by luck.
    let gate_n = n.max(MIN_GATE_LEAVES);
    if gate_n != n {
        println!(
            "  (the gate matrix runs at {gate_n} leaves, not {n}: below {MIN_GATE_LEAVES} a \
             tampered set passes by chance with probability 3^-k)"
        );
    }
    let gate_input = if gate_n == n {
        None
    } else {
        let (_, output, proof) = vector_1_proof(gate_n, SEED_ENTROPY, BINDING);
        let (_, chain_code) = bip32_vector_1();
        Some(lift_input_for(chain_code, HARDENED_OFFSET, output, BINDING, proof))
    };
    gate_matrix(gate_input.as_ref().unwrap_or(&input), gate_n);
    drop(gate_input);
    if mode == "gate" {
        return;
    }

    let mut mono_wall = Duration::ZERO;
    if mode == "both" || mode == "mono" {
        println!("── monolithic baseline (one program, all {n} responses) ──");
        let pk = client.setup(LIFT_ELF).expect("lift program setup");
        let t = Instant::now();
        let lifted = client
            .prove(&pk, lift_stdin(&input))
            .compressed()
            .run()
            .expect("monolithic proving");
        mono_wall = t.elapsed();
        let journal = sp1_lifter_host::check_lifted_proof(
            &client,
            &lifted,
            pk.verifying_key(),
            &expectation_for(&input, n as u32),
        )
        .expect("monolithic gate");
        assert_eq!(journal.num_responses, n as u32);
        println!("  monolithic compressed proving: {mono_wall:.2?} (gate green)");
        println!();
    }
    if mode == "mono" {
        return;
    }

    let leaf_pk = client.setup(LEAF_ELF).expect("leaf program setup");
    let leaf_vk = leaf_pk.verifying_key().clone();
    assert_eq!(
        leaf_vk.hash_u32(),
        sp1_lifter_host::PINNED_LEAF_VK,
        "the pinned leaf key is stale against the built leaf ELF — run `vkeys` and update \
         `pinned-keys`"
    );
    let leaf_vk_digest = leaf_vk.hash_u32();
    // Keyed by response count, and that is not tidiness. A leaf is response `i` of an
    // `n`-response proof, and the responses themselves depend on the global challenge,
    // which depends on how many there are — so a leaf cached at one `n` is not a leaf at
    // another. Sharing one directory between counts would leave the stale proof verifying
    // perfectly well under the leaf key and being refused by the cross-leaf gate for a
    // challenge mismatch, which is precisely the message a real soundness failure gives.
    let cache = std::path::PathBuf::from(format!("fixtures/leaves/n{n}"));
    let cache = cache.as_path();

    // ── leaf proofs: proved once, cached, reused by every root measurement ──
    let mut leaf_walls: Vec<Duration> = Vec::new();
    if mode == "leaves" || mode == "both" || mode == "split" {
        std::fs::create_dir_all(cache).expect("leaf cache dir");
        println!("── {n} leaf proofs ──");
        for i in 0..n {
            let path = cache.join(format!("leaf-{i}.compressed.bin"));
            let leaf_input = leaf_input_for(&input, i);
            let t = Instant::now();
            let leaf_proof = client
                .prove(&leaf_pk, leaf_stdin(&leaf_input))
                .compressed()
                .run()
                .unwrap_or_else(|e| panic!("leaf {i} proving: {e:#}"));
            let wall = t.elapsed();
            client
                .verify(&leaf_proof, &leaf_vk, None)
                .unwrap_or_else(|e| panic!("leaf {i} verification: {e:#}"));
            let pv = leaf_proof.public_values.as_slice().to_vec();
            let decoded: LeafJournal = postcard::from_bytes(&pv).expect("decode leaf journal");
            assert_eq!(decoded, leaf_journal_native(&input, i), "leaf {i}: guest journal differs from native");
            leaf_proof.save(&path).expect("save leaf proof");
            println!("  leaf {i}: {wall:.2?}, {} bytes of public values → {}", pv.len(), path.display());
            leaf_walls.push(wall);
        }
        println!();
    }
    if mode == "leaves" {
        let total: Duration = leaf_walls.iter().sum();
        println!("  {n} leaves cached, {total:.2?} total, {:.2?} mean", total / n as u32);
        println!("split-and-aggregate probe: PASS");
        return;
    }

    // ── the root, over cached leaf proofs ──
    let forge = mode.starts_with("forge-");
    println!(
        "── root over {n} leaves{} ──",
        match mode.as_str() {
            "forge-vk" => " (FORGED leaf key — must fail)",
            "forge-count" => " (one proof WITHHELD — must fail)",
            "forge-order" => " (proofs REORDERED against the claims — must fail)",
            _ => "",
        }
    );
    let mut leaf_public_values: Vec<Vec<u8>> = Vec::with_capacity(n);
    let mut compressed_leaves = Vec::with_capacity(n);
    let mut root_stdin = SP1Stdin::new();
    for i in 0..n {
        let path = cache.join(format!("leaf-{i}.compressed.bin"));
        let leaf_proof = sp1_sdk::SP1ProofWithPublicValues::load(&path)
            .unwrap_or_else(|e| panic!("no cached leaf {i} at {}: run `split-probe {n} leaves` first ({e:#})", path.display()));
        // A cached proof outlives the program that produced it, so a stale cache would be
        // aggregated into a root under a verifying key it never came from. Re-verify.
        client
            .verify(&leaf_proof, &leaf_vk, None)
            .unwrap_or_else(|e| panic!("cached leaf {i} does not verify under the current leaf key — stale cache: {e:#}"));
        let pv = leaf_proof.public_values.as_slice().to_vec();
        let decoded: LeafJournal = postcard::from_bytes(&pv).expect("decode leaf journal");
        assert_eq!(decoded, leaf_journal_native(&input, i), "cached leaf {i} is for a different proof: stale cache");
        leaf_public_values.push(pv);
        let SP1Proof::Compressed(compressed) = leaf_proof.proof else {
            panic!("cached leaf {i} is not compressed");
        };
        compressed_leaves.push(*compressed);
    }
    // The claims (the public values in the frame) and the proofs (the deferred-proof
    // channel) are two separate streams. SP1 binds them twice over, and the two bindings
    // are not the same thing:
    //
    //  - **In the recursion layer**, in circuit: each claim's (vk digest, pv digest) is
    //    folded into a Poseidon2 chain that the recursion circuit must reconstruct from the
    //    proofs actually supplied, starting from zero. This is the binding that matters,
    //    because it is the one a prover cannot simply decline to run.
    //  - **On the host**, at execution time: the `SubproofVerifier` matches what the guest
    //    asked for against what the caller supplied, and the artifact store is keyed by
    //    digest. SP1's own comment on that trait calls it a sanity check "that the user
    //    passed in the correct proof", and says in as many words that "the actual
    //    constraints happen in the recursion layer".
    //
    // The forgeries below break the correspondence in each of the three ways it can be
    // broken — and every one of them is caught by the host layer, which is what a caller
    // using this SDK will hit first. See the note where the refusal is classified.
    match mode.as_str() {
        "forge-count" => {
            compressed_leaves.pop().expect("at least one leaf to withhold");
        }
        "forge-order" => compressed_leaves.swap(0, 1),
        _ => {}
    }
    for compressed in compressed_leaves {
        root_stdin.write_proof(compressed, leaf_vk.vk.clone());
    }
    let mut claimed_vk = leaf_vk_digest;
    if mode == "forge-vk" {
        claimed_vk[0] ^= 1;
    }
    let root_input = root_input_for(&input, claimed_vk, leaf_public_values);
    root_stdin.write_vec(postcard::to_allocvec(&root_input).expect("serialize root frame"));
    let root_pk = client.setup(ROOT_ELF).expect("root program setup");
    let t = Instant::now();
    let result = client.prove(&root_pk, root_stdin).compressed().run();
    let root_wall = t.elapsed();
    if forge {
        match result {
            Ok(_) => panic!(
                "{mode}: the root PROVED with the leaf claims and the leaf proofs out of \
                 correspondence — SP1's recursion layer did not bind them"
            ),
            Err(e) => {
                let msg = format!("{e:#}");
                println!("  root refused in {root_wall:.2?}: {msg}");
                // Where it refused matters more than that it refused, and the host's error
                // string cannot answer that: it says "artifact not found" for all three,
                // including the one whose guest ran and refused on its own. So ask the guest
                // directly, by executing the same frame — execution runs guest code and
                // reports its panic, and costs seconds rather than the prove's minutes.
                //
                // 🪤 Classifying on the prove error alone got this wrong once. `forge-vk`
                // was recorded as refusing at host bookkeeping when its stderr showed the
                // root panicking at the pinned-key assertion first; the "artifact not found"
                // is downstream wreckage of the failed run, not the refuser. The tell was in
                // the timings all along — 18.7 s against 105.8 s and 108.8 s — and it was
                // tabulated without being asked about.
                let mut probe_stdin = SP1Stdin::new();
                probe_stdin.write_vec(
                    postcard::to_allocvec(&root_input).expect("serialize root frame"),
                );
                let guest = client.execute(ROOT_ELF, probe_stdin).run();
                let guest_refused = match &guest {
                    Err(e) => Some(format!("{e:#}")),
                    Ok((_, report)) if report.exit_code != 0 => {
                        Some(format!("guest exited {}", report.exit_code))
                    }
                    Ok(_) => None,
                };
                match (mode.as_str(), &guest_refused) {
                    ("forge-vk", Some(why)) => println!(
                        "{mode}: PASS — refused BY THE ROOT GUEST's pinned-key assertion ({why}). \
                         The prove-time error names the host layer, but the guest refuses first."
                    ),
                    ("forge-vk", None) => panic!(
                        "{mode}: the root guest ACCEPTED a leaf key that is not the pinned one — \
                         the in-guest pin check did not fire"
                    ),
                    (_, Some(why)) => panic!(
                        "{mode}: the guest refused ({why}), but this tamper is invisible to the \
                         guest by construction — it touches only which proofs are supplied. \
                         Something else is wrong."
                    ),
                    (_, None) => println!(
                        "{mode}: PASS — the guest accepts this frame (the tamper is invisible to \
                         it, as expected) and something inside SP1's recursion layer refuses it. \
                         🔴 WHICH mechanism is NOT established: the prove-time observable is the \
                         same base-field division by zero that `forge-vk` produces, and there the \
                         guest is the refuser. So this neither demonstrates the in-circuit \
                         deferred-proof digest nor rules it out. See the module comment."
                    ),
                }
                return;
            }
        }
    }
    let root_proof = result.expect("root proving");
    client
        .verify(&root_proof, root_pk.verifying_key(), None)
        .expect("root verification");
    let root_journal =
        decode_root_journal(root_proof.public_values.as_slice()).expect("decode root journal");
    check_journal(&root_journal.lift, &expectation_for(&input, n as u32))
        .expect("root journal must pass the same relying-party gate as the monolithic lift");
    assert_eq!(root_journal.leaf_vk, leaf_vk_digest, "root attests a different leaf key");
    assert_eq!(root_journal.lift.num_responses, n as u32);
    println!(
        "  root: {root_wall:.2?} (gate green, {} responses, {} bytes of public values)",
        root_journal.lift.num_responses,
        root_proof.public_values.as_slice().len(),
    );
    println!();

    println!("── result (n={n}) ──");
    if mono_wall > Duration::ZERO {
        println!("  monolithic, one machine        {:>10.2?}", mono_wall);
    }
    println!("  root                           {:>10.2?}", root_wall);
    if !leaf_walls.is_empty() {
        let leaf_total: Duration = leaf_walls.iter().sum();
        let leaf_max = *leaf_walls.iter().max().expect("at least one leaf");
        let sequential = leaf_total + root_wall;
        let critical_path = leaf_max + root_wall;
        println!("  leaves, summed ({n} × ~{:.2?})    {:>10.2?}", leaf_max, leaf_total);
        println!("  split, one machine             {:>10.2?}", sequential);
        println!("  split, critical path           {:>10.2?}  (slowest leaf + root)", critical_path);
        if mono_wall > Duration::ZERO {
            println!("  split overhead on one machine  {:>10.2}×", sequential.as_secs_f64() / mono_wall.as_secs_f64());
            println!("  speedup with {n} machines        {:>10.2}×", mono_wall.as_secs_f64() / critical_path.as_secs_f64());
            println!("  root share of critical path    {:>10.1}%", 100.0 * root_wall.as_secs_f64() / critical_path.as_secs_f64());
        }
    }
    println!("split-and-aggregate probe: PASS");
}

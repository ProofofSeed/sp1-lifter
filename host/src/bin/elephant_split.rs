// SPDX-License-Identifier: LGPL-3.0-or-later

//! The split, on the statement that actually binds a seed to an Ethereum account.
//!
//! Everything the split has ever been measured on is the hardened-child statement, which is
//! tens of kilobytes per response. The figures that matter about the Ethereum-address
//! statement — a leaf near an hour and a half, a wall clock near five hours given enough
//! machines — are projections: the refitted instruction law applied at a proving rate
//! measured on a different statement. This measures them.
//!
//! Modes:
//!   `gate`   — the cross-leaf refusal matrix on elephant leaf journals, no proving.
//!   `leaves` — prove and cache `n` elephant leaves.
//!   `root`   — a root over the cached leaves, with the relying-party gate.
//!   `both`   — all three.
//!
//! Usage: `elephant-split [n] [gate|leaves|root|both]` (default: 2, both).
//!
//! ⚠️ A leaf is projected at ~1.5 h and the root's fingerprint pass at ~3 h, so `both` at
//! n=2 is a six-hour run. Start with `gate`, which costs seconds.

use std::time::Instant;

use sp1_sdk::{HashableKey, ProvingKey, SP1Proof, SP1Stdin, blocking::{ProveRequest, Prover}};
use snark_lifter_elephant::{
    ELEPHANT_JOURNAL_VERSION, ElephantRootJournal, EthereumAddressCircuit,
    H, PS, PV, S, check_elephant_journal,
};
use snark_lifter_host::{
    LEAF_ELEPHANT_ELF, ROOT_ELEPHANT_ELF, WP, WTP, elephant_expectation_for, prover_backend,
    prover_client,
};
use snark_lifter_split::{
    AggregateError, ELEPHANT_SPLIT_FRAME_VERSION, ElephantLeafInput, ElephantRootInput,
    LEAF_JOURNAL_VERSION, LeafCommitment, LeafJournal, aggregate, statement_digest_generic,
};
use zkboo::{
    circuit::Circuit,
    executor::{ExecOptions, exec},
    prover::par_prove,
    verifier::replay::{OwnedFlexibleWordPairPool, ViewReplayerBackend},
    word::Words,
};

type WPP = OwnedFlexibleWordPairPool<usize>;

const SEED_ENTROPY: &[u8] = b"snark-lifter elephant split entropy";
const BINDING: &[u8] = b"snark-lifter elephant split tx binding";
const NO_PUBLIC_PARAMS: &[u8] = &[];

/// See `elephant_probe.rs`: a private key with no special structure, used only as a witness.
const PRIV_KEY: [u8; 32] = [
    0x18, 0xe1, 0x4a, 0x7b, 0x6a, 0x30, 0x7f, 0x42, 0x6a, 0x94, 0xf8, 0x11, 0x47, 0x01, 0xe7, 0xc8,
    0xe7, 0x74, 0xe7, 0xf9, 0xa4, 0x7e, 0x2c, 0x20, 0x35, 0xdb, 0x29, 0xa2, 0x06, 0x32, 0x17, 0x25,
];

/// The floor the refusal matrix runs at, for the reason `split_probe.rs` gives: a tampered set
/// of `k` leaves satisfies the challenge check by chance with probability 3^-k, so a matrix at
/// small `k` is a coin toss rather than evidence.
const MIN_GATE_LEAVES: usize = 24;

/// Replays one response natively — the same work the leaf guest does, without the zkVM.
fn leaf_journal_native(
    expected_output: &Words,
    binding: &[u8],
    position: usize,
    response: &zkboo::prover::proof::Response<S, S>,
) -> LeafJournal {
    let circuit = EthereumAddressCircuit::public_only();
    let replayer = ViewReplayerBackend::<H, PV, S, WPP>::new(response).into_view_replayer();
    circuit.exec(&replayer);
    let commitments = replayer.finalize_with_arg(expected_output).expect("native replay");
    return LeafJournal {
        version: LEAF_JOURNAL_VERSION,
        position: position as u32,
        statement_digest: statement_digest_generic(NO_PUBLIC_PARAMS, expected_output, binding),
        party: response.challenge().index() as u8,
        commitments: commitments.map(|c| LeafCommitment {
            digest: *c.digest(),
            output_share: c.output_share().clone(),
        }),
    };
}

fn expect_refusal(name: &str, leaves: &[LeafJournal], digest: &S, expected: fn(&AggregateError) -> bool) {
    match aggregate(leaves, digest, BINDING) {
        Ok(()) => panic!("{name}: the gate ACCEPTED a leaf set it must refuse"),
        Err(e) if expected(&e) => println!("  {name}: refused ({}) ✓", e.tag()),
        Err(e) => panic!("{name}: refused for the wrong reason: {}", e.tag()),
    }
}

/// The cross-leaf refusal matrix, on real elephant leaf journals, without any proving.
fn gate_matrix(expected_output: &Words, responses: &[zkboo::prover::proof::Response<S, S>]) {
    let n = responses.len();
    println!("── cross-leaf gate matrix on the elephant statement (n={n}, no proving) ──");
    let leaves: Vec<LeafJournal> = responses
        .iter()
        .enumerate()
        .map(|(i, r)| leaf_journal_native(expected_output, BINDING, i, r))
        .collect();
    let digest = statement_digest_generic(NO_PUBLIC_PARAMS, expected_output, BINDING);
    aggregate(&leaves, &digest, BINDING).expect("control: the genuine leaf set must aggregate");
    println!("  control (all {n} leaves, in order): accepted ✓");

    let mut dropped = leaves.clone();
    dropped.remove(n / 2);
    for (i, leaf) in dropped.iter_mut().enumerate() {
        leaf.position = i as u32;
    }
    expect_refusal("G1 dropped response", &dropped, &digest, |e| matches!(e, AggregateError::ChallengeMismatch { .. }));

    // G2: a substituted response — leaf 1 carrying leaf 0's commitments at its own
    // position. Structurally impeccable, cryptographically dead.
    let mut substituted = leaves.clone();
    substituted[1].commitments = leaves[0].commitments.clone();
    substituted[1].party = leaves[0].party;
    expect_refusal("G2 substituted response", &substituted, &digest, |e| matches!(e, AggregateError::ChallengeMismatch { .. }));

    // G3b: a mislabelled leaf — content right for its slot, `position` field wrong. The
    // only case the position check is load-bearing for, since `position` never enters the
    // challenge hash and nothing else constrains it.
    let mut mislabelled = leaves.clone();
    mislabelled[1].position = 99;
    expect_refusal("G3b mislabelled position", &mislabelled, &digest, |e| matches!(e, AggregateError::PositionMismatch { .. }));

    // G3: a duplicated response, which would let one response's work be claimed as n
    // responses' soundness.
    let mut duplicated = leaves.clone();
    duplicated[1] = leaves[0].clone();
    expect_refusal("G3 duplicated response", &duplicated, &digest, |e| matches!(e, AggregateError::PositionMismatch { .. }));

    let mut reordered = leaves.clone();
    reordered.swap(0, 1);
    for (i, leaf) in reordered.iter_mut().enumerate() {
        leaf.position = i as u32;
    }
    expect_refusal("G4 reordered responses", &reordered, &digest, |e| matches!(e, AggregateError::ChallengeMismatch { .. }));

    // G5: a leaf proved against a different statement, aggregated into this one. Distinct
    // from G8 below: there the *root's* statement is wrong, here one leaf's is.
    let mut foreign_leaf = leaves.clone();
    foreign_leaf[1].statement_digest[0] ^= 1;
    expect_refusal("G5 foreign statement", &foreign_leaf, &digest, |e| matches!(e, AggregateError::StatementMismatch(_)));

    let mut tampered = leaves.clone();
    tampered[0].commitments[0].digest[0] ^= 1;
    expect_refusal("G6 tampered commitment", &tampered, &digest, |e| matches!(e, AggregateError::ChallengeMismatch { .. }));

    expect_refusal("G7 empty leaf set", &[], &digest, |e| matches!(e, AggregateError::NoLeaves));

    let foreign = statement_digest_generic(NO_PUBLIC_PARAMS, expected_output, b"a different binding");
    expect_refusal("G8 wrong root statement", &leaves, &foreign, |e| matches!(e, AggregateError::StatementMismatch(_)));
    println!("cross-leaf gate matrix: PASS");
    println!();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().map(|a| a.parse().expect("n must be a positive integer")).unwrap_or(2);
    let mode = args.next().unwrap_or_else(|| "both".to_string());
    assert!(
        matches!(mode.as_str(), "gate" | "leaves" | "root" | "both" | "execute" | "execute-root"),
        "bad mode `{mode}`"
    );
    if std::env::var_os("RUST_LOG").is_none() {
        unsafe { std::env::set_var("RUST_LOG", "error") };
    }
    sp1_sdk::setup_logger();
    let (client, config) = prover_client();
    assert_ne!(prover_backend(), "mock", "refusing to run under SP1_PROVER=mock");
    println!("── elephant split (n={n}, mode={mode}) ──");
    println!("  prover config: {config}");
    println!("  zkboo: {}", snark_lifter_host::zkboo_provenance());

    let circuit = EthereumAddressCircuit::with_witness(PRIV_KEY.to_vec());
    let expected_output = exec::<_, WP, _>(&circuit, ExecOptions::new());

    if mode == "gate" || mode == "both" {
        // The matrix runs at its own floor, never the proving count. Native proving of an
        // elephant response is ~0.3 s, so 24 of them costs seconds.
        let gate_n = n.max(MIN_GATE_LEAVES);
        let t = Instant::now();
        let responses = par_prove::<_, H, PS, PV, S, WTP>(&circuit, gate_n, SEED_ENTROPY, BINDING);
        println!("  proved {gate_n} elephant responses natively in {:.2?}", t.elapsed());
        gate_matrix(&expected_output, &responses);
        if mode == "gate" {
            return;
        }
    }

    let t = Instant::now();
    let responses = par_prove::<_, H, PS, PV, S, WTP>(&circuit, n, SEED_ENTROPY, BINDING);
    println!("  proved {n} elephant responses natively in {:.2?}", t.elapsed());
    // `execute` turns the leaf's proving rate from an inference into a measurement. Without
    // it, instructions-per-second has to be computed by assuming the leaf guest costs the
    // marginal term of the monolithic instruction law — plausible, since a leaf replays one
    // response and computes no fingerprint, but the leaf also parses its own frame and
    // encodes a journal, and nothing had checked how much that is worth.
    if mode == "execute" {
        println!("── executing one elephant leaf ──");
        let input = ElephantLeafInput {
            version: ELEPHANT_SPLIT_FRAME_VERSION,
            expected_output: expected_output.clone(),
            binding: BINDING.to_vec(),
            position: 0,
            response: responses[0].clone(),
        };
        let mut stdin = SP1Stdin::new();
        stdin.write_vec(postcard::to_allocvec(&input).expect("serialize leaf frame"));
        let t = Instant::now();
        let (_, report) = client.execute(LEAF_ELEPHANT_ELF, stdin).run().expect("elephant leaf execute");
        assert_eq!(report.exit_code, 0, "leaf guest execution failed");
        let instructions = report.total_instruction_count();
        println!("  leaf instructions: {instructions}");
        println!("  executor wall: {:.2?}", t.elapsed());
        println!(
            "  monolithic law's marginal term: 2.4627e9 ⇒ leaf is {:.3}x of it",
            instructions as f64 / 2.4627e9
        );
        return;
    }
    let leaf_pk = client.setup(LEAF_ELEPHANT_ELF).expect("elephant leaf setup");
    let leaf_vk = leaf_pk.verifying_key().clone();
    // Pin freshness, checked on every run rather than by a tool someone must remember to run.
    // The root compiles the leaf key in, so a pin that has gone stale against the leaf ELF makes
    // the root refuse — deep inside proving, minutes later, with an error about a claimed key.
    // Saying it here costs nothing and names the actual problem.
    assert_eq!(
        leaf_vk.hash_u32(),
        snark_lifter_host::PINNED_LEAF_ELEPHANT_VK,
        "the pinned elephant leaf key is stale against the built leaf ELF — run `vkeys` and \
         update `pinned-keys`"
    );
    let cache = std::path::PathBuf::from(format!("fixtures/elephant-leaves/n{n}"));

    if mode == "leaves" || mode == "both" {
        std::fs::create_dir_all(&cache).expect("leaf cache dir");
        println!("── {n} elephant leaf proofs ──");
        for position in 0..n {
            let input = ElephantLeafInput {
                version: ELEPHANT_SPLIT_FRAME_VERSION,
                expected_output: expected_output.clone(),
                binding: BINDING.to_vec(),
                position: position as u32,
                response: responses[position].clone(),
            };
            let mut stdin = SP1Stdin::new();
            stdin.write_vec(postcard::to_allocvec(&input).expect("serialize leaf frame"));
            let t = Instant::now();
            let proof = client
                .prove(&leaf_pk, stdin)
                .compressed()
                .run()
                .unwrap_or_else(|e| panic!("elephant leaf {position} proving: {e:#}"));
            let wall = t.elapsed();
            client.verify(&proof, &leaf_vk, None).expect("elephant leaf verification");
            let pv = proof.public_values.as_slice().to_vec();
            let decoded: LeafJournal = postcard::from_bytes(&pv).expect("decode leaf journal");
            assert_eq!(
                decoded,
                leaf_journal_native(&expected_output, BINDING, position, &responses[position]),
                "elephant leaf {position}: the guest journal differs from the native replay"
            );
            let path = cache.join(format!("leaf-{position}.compressed.bin"));
            proof.save(&path).expect("save leaf proof");
            println!("  leaf {position}: {wall:.2?}, {} bytes of public values", pv.len());
        }
        println!();
        if mode == "leaves" {
            println!("elephant split: PASS");
            return;
        }
    }

    if mode != "execute-root" {
        println!("── elephant root over {n} leaves ──");
    }
    let mut root_stdin = SP1Stdin::new();
    let mut leaf_public_values = Vec::with_capacity(n);
    for position in 0..n {
        let path = cache.join(format!("leaf-{position}.compressed.bin"));
        let proof = sp1_sdk::SP1ProofWithPublicValues::load(&path)
            .unwrap_or_else(|e| panic!("no cached elephant leaf {position}: run `leaves` first ({e:#})"));
        client
            .verify(&proof, &leaf_vk, None)
            .unwrap_or_else(|e| panic!("cached elephant leaf {position} does not verify under the current key — stale cache: {e:#}"));
        leaf_public_values.push(proof.public_values.as_slice().to_vec());
        let SP1Proof::Compressed(compressed) = proof.proof else {
            panic!("cached elephant leaf {position} is not compressed");
        };
        root_stdin.write_proof(*compressed, leaf_vk.vk.clone());
    }
    let root_input = ElephantRootInput {
        version: ELEPHANT_SPLIT_FRAME_VERSION,
        expected_output: expected_output.clone(),
        binding: BINDING.to_vec(),
        leaf_vk: leaf_vk.hash_u32(),
        leaf_public_values,
    };
    root_stdin.write_vec(postcard::to_allocvec(&root_input).expect("serialize root frame"));

    // Executing the root turns its cost *composition* into a measurement. The wall clock says
    // how long the root takes; only this says how much of it is the one-off secp256k1 fingerprint
    // traversal, and "essentially all of it" was an inference from the monolithic law's fixed
    // term until this existed.
    if mode == "execute-root" {
        println!("── executing the elephant root over {n} leaves ──");
        let t = Instant::now();
        let (_, report) =
            client.execute(ROOT_ELEPHANT_ELF, root_stdin).run().expect("elephant root execute");
        assert_eq!(report.exit_code, 0, "root guest execution failed");
        let instructions = report.total_instruction_count();
        println!("  root instructions: {instructions}");
        println!("  executor wall: {:.2?}", t.elapsed());
        println!(
            "  monolithic law's fixed term 5.1004e9 ⇒ root is {:.3}x of it",
            instructions as f64 / 5.1004e9
        );
        println!(
            "  one leaf is 2,395,132,641 ⇒ root is {:.2} leaves of guest work",
            instructions as f64 / 2_395_132_641.0
        );
        return;
    }
    let root_pk = client.setup(ROOT_ELEPHANT_ELF).expect("elephant root setup");
    let t = Instant::now();
    let root_proof = client.prove(&root_pk, root_stdin).compressed().run().expect("elephant root proving");
    let root_wall = t.elapsed();
    client.verify(&root_proof, root_pk.verifying_key(), None).expect("elephant root verification");
    let (journal, rest): (ElephantRootJournal, _) =
        postcard::take_from_bytes(root_proof.public_values.as_slice()).expect("decode root journal");
    assert!(rest.is_empty(), "root journal decode left {} trailing bytes", rest.len());
    // Built by the constructor, NOT field by field: an expectation whose fingerprint is copied
    // out of the journal under test cannot fail its own fingerprint check, and one whose format
    // version is read from the linked zkboo tracks the consumer instead of constraining it.
    let expectation = elephant_expectation_for(&expected_output, BINDING, n as u32)
        .expect("expected output must carry a set assertion word");
    check_elephant_journal(&journal.lift, &expectation).expect("root journal must pass the relying-party gate");
    assert_eq!(journal.lift.version, ELEPHANT_JOURNAL_VERSION);
    // The OUTER version too: it was committed by the root and checked by nobody, and postcard
    // is not self-describing, so on a layout change an unchecked version misparses silently.
    assert_eq!(
        journal.version,
        snark_lifter_split::ROOT_JOURNAL_VERSION,
        "elephant root journal layout version"
    );
    assert_eq!(journal.leaf_vk, leaf_vk.hash_u32(), "root attests a different leaf key");
    println!(
        "  root: {root_wall:.2?} (gate green, {} responses, fingerprint {}…)",
        journal.lift.num_responses,
        journal.lift.fingerprint[..4].iter().map(|b| format!("{b:02x}")).collect::<String>(),
    );
    println!("elephant split: PASS");
}

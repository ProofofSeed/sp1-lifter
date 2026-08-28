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
    ELEPHANT_JOURNAL_VERSION, ElephantExpectation, ElephantRootJournal, EthereumAddressCircuit,
    H, PS, PV, S, check_elephant_journal,
};
use snark_lifter_host::{LEAF_ELEPHANT_ELF, ROOT_ELEPHANT_ELF, WP, WTP, prover_backend, prover_client};
use snark_lifter_split::{
    AggregateError, ELEPHANT_SPLIT_FRAME_VERSION, ElephantLeafInput, ElephantRootInput,
    LEAF_JOURNAL_VERSION, LeafCommitment, LeafJournal, aggregate, statement_digest_generic,
};
use zkboo::{
    circuit::Circuit,
    executor::exec,
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

    let mut reordered = leaves.clone();
    reordered.swap(0, 1);
    for (i, leaf) in reordered.iter_mut().enumerate() {
        leaf.position = i as u32;
    }
    expect_refusal("G4 reordered responses", &reordered, &digest, |e| matches!(e, AggregateError::ChallengeMismatch { .. }));

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
    assert!(matches!(mode.as_str(), "gate" | "leaves" | "root" | "both"), "bad mode `{mode}`");
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
    let expected_output = exec::<_, WP>(&circuit);

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
    let leaf_pk = client.setup(LEAF_ELEPHANT_ELF).expect("elephant leaf setup");
    let leaf_vk = leaf_pk.verifying_key().clone();
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

    println!("── elephant root over {n} leaves ──");
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
    let root_pk = client.setup(ROOT_ELEPHANT_ELF).expect("elephant root setup");
    let t = Instant::now();
    let root_proof = client.prove(&root_pk, root_stdin).compressed().run().expect("elephant root proving");
    let root_wall = t.elapsed();
    client.verify(&root_proof, root_pk.verifying_key(), None).expect("elephant root verification");
    let (journal, rest): (ElephantRootJournal, _) =
        postcard::take_from_bytes(root_proof.public_values.as_slice()).expect("decode root journal");
    assert!(rest.is_empty(), "root journal decode left {} trailing bytes", rest.len());
    let expectation = ElephantExpectation {
        fingerprint: journal.lift.fingerprint,
        expected_output: expected_output.clone(),
        binding: BINDING.to_vec(),
        min_responses: n as u32,
        proof_format_version: zkboo::PROOF_FORMAT_VERSION,
    };
    check_elephant_journal(&journal.lift, &expectation).expect("root journal must pass the relying-party gate");
    assert_eq!(journal.lift.version, ELEPHANT_JOURNAL_VERSION);
    assert_eq!(journal.leaf_vk, leaf_vk.hash_u32(), "root attests a different leaf key");
    println!(
        "  root: {root_wall:.2?} (gate green, {} responses, fingerprint {}…)",
        journal.lift.num_responses,
        journal.lift.fingerprint[..4].iter().map(|b| format!("{b:02x}")).collect::<String>(),
    );
    println!("elephant split: PASS");
}

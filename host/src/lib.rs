// SPDX-License-Identifier: LGPL-3.0-or-later

//! Shared host-side helpers for the SNARK lifter: the development statement fixture,
//! the program input framing, and SP1 execution/verification drivers.

pub mod orchestrator;

use sp1_core_executor::SP1CoreOpts;
use sp1_sdk::{
    Elf, SP1ProofWithPublicValues, SP1Stdin, SP1VerifyingKey,
    blocking::{EnvProver, Prover},
    include_elf,
};
use sp1_lifter_statement::{H, HardenedChildCircuit, LiftInput, PS, PV, S};
use zkboo::{
    executor::{ExecOptions, OwnedFlexibleWordPool, exec},
    prover::{par_prove, proof::Response, views::OwnedFlexibleWordTriplePool},
    verifier::replay::OwnedFlexibleWordPairPool,
    word::Words,
};
pub use zkboo_bip32::HARDENED_OFFSET;

/// The lift program ELF, built by `sp1-build` from `program/`.
pub const LIFT_ELF: Elf = include_elf!("sp1-lifter-program");

/// The elephant probe program ELF (secp256k1 Ethereum-address statement).
pub const ELEPHANT_ELF: Elf = include_elf!("sp1-lifter-program-elephant");

/// The split-and-aggregate probe's leaf program ELF (one response per proof).
pub const LEAF_ELF: Elf = include_elf!("sp1-lifter-program-leaf");

/// The split-and-aggregate probe's root program ELF (cross-leaf global Fiat-Shamir gate).
pub const ROOT_ELF: Elf = include_elf!("sp1-lifter-program-root");

/// The elephant leaf program ELF (one secp256k1 response per proof).
pub const LEAF_ELEPHANT_ELF: Elf = include_elf!("sp1-lifter-program-leaf-elephant");

/// The elephant root program ELF.
pub const ROOT_ELEPHANT_ELF: Elf = include_elf!("sp1-lifter-program-root-elephant");

/// The leaf verifying key `program-root` has compiled in.
pub use sp1_lifter_pinned_keys::LEAF_VK as PINNED_LEAF_VK;

/// The leaf verifying key `program-root-elephant` has compiled in.
pub use sp1_lifter_pinned_keys::LEAF_ELEPHANT_VK as PINNED_LEAF_ELEPHANT_VK;

/// Executor-side word pool.
pub type WP = OwnedFlexibleWordPool<usize>;
/// Prover-side word-triple pool.
pub type WTP = OwnedFlexibleWordTriplePool<usize>;
/// Verifier-side word-pair pool.
pub type WPP = OwnedFlexibleWordPairPool<usize>;

/// What the zkboo crates in this binary actually are, taken from the lockfile at build time.
///
/// Printed by every driver next to the prover configuration, for the same reason the prover
/// configuration is printed: a measurement means nothing without the thing it measured. The
/// build refuses outright unless every zkboo crate resolves at the required version from a path
/// override — see `build.rs` for the failure this guards against, and for why a path is what a
/// correct build now looks like. Each crate is reported by the git revision of its checkout,
/// with a `-dirty` suffix if that checkout had uncommitted work when this binary was built.
pub fn zkboo_provenance() -> &'static str {
    return env!("ZKBOO_PROVENANCE");
}

/// Repetitions needed for `(2/3)^n <= 2^-2b`, where `b` is a post-quantum security level in bits.
///
/// The project convention that a post-quantum target needs twice the classical bits, so that
/// Grover on the grinding is priced in: `n_eps(128) == 438` for PQ-128, and `n_eps(64) == 219`,
/// which is the classical-128 figure this repository lifts at.
///
/// Vendored from `zkboo-harness`, where zkboo 1.2.0 moved it out of the core crate. That crate is
/// `publish = false`, so depending on it would make this workspace unpublishable too. The
/// arithmetic is unchanged.
pub const fn n_eps(pq_security_bits: usize) -> usize {
    // 1 / log2(3/2) ~ NUM / DEN.
    const NUM: u128 = 1_709_511_291_351_455;
    const DEN: u128 = 1_000_000_000_000_000;
    let two_b = (2 * pq_security_bits) as u128;
    // ceil(two_b * NUM / DEN)
    return ((two_b * NUM + (DEN - 1)) / DEN) as usize;
}

/// Decodes a hex string (must have even length and valid digits).
pub fn from_hex(s: &str) -> Vec<u8> {
    return (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect();
}

/// BIP-32 test vector 1, chain m → m/0H: `(parent_priv, chain_code)`.
///
/// A public test vector, not a secret.
pub fn bip32_vector_1() -> (Vec<u8>, Vec<u8>) {
    let parent_priv = from_hex("e8f32e723decf4051aefac8e2c93c9c5b214313817cdb01a1494b917c8436b35");
    let chain_code = from_hex("873dff81c02f525623fd1fe5167eac3a55a049de3d314bb42ee227ffed37d508");
    return (parent_priv, chain_code);
}

/// Builds the development statement fixture: the vector-1 hardened-child circuit (with
/// witness), its executed output, and a genuine proof at `num_iters` repetitions bound
/// to `binding`.
pub fn vector_1_proof(
    num_iters: usize,
    seed_entropy: &[u8],
    binding: &[u8],
) -> (HardenedChildCircuit, Words, Vec<Response<S, S>>) {
    let (parent_priv, chain_code) = bip32_vector_1();
    let circuit = HardenedChildCircuit::with_witness(chain_code, parent_priv, HARDENED_OFFSET);
    let output = exec::<_, WP, _>(&circuit, ExecOptions::new());
    let proof = par_prove::<_, H, PS, PV, S, WTP>(&circuit, num_iters, seed_entropy, binding);
    return (circuit, output, proof);
}

/// Assembles a [`LiftInput`] frame for the hardened-child statement.
pub fn lift_input_for(
    chain_code: Vec<u8>,
    index: u32,
    output: Words,
    binding: &[u8],
    proof: sp1_lifter_statement::LiftProof,
) -> LiftInput {
    return LiftInput {
        version: sp1_lifter_statement::LIFT_FRAME_VERSION,
        chain_code,
        index,
        expected_output: output,
        binding: binding.to_vec(),
        proof,
    };
}

/// Builds the program stdin carrying the postcard-encoded [`LiftInput`] frame.
pub fn lift_stdin(input: &LiftInput) -> SP1Stdin {
    let frame = postcard::to_allocvec(input).expect("serialize input frame");
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(frame);
    return stdin;
}

/// The reference consumer expectation matching a [`LiftInput`], with the fingerprint
/// recomputed host-side over the public-only circuit.
pub fn expectation_for(
    input: &LiftInput,
    min_responses: u32,
) -> sp1_lifter_statement::JournalExpectation {
    let public_circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    return sp1_lifter_statement::JournalExpectation {
        fingerprint: zkboo_circuit_hash::hash_circuit::<_, H>(&public_circuit),
        chain_code: input.chain_code.clone(),
        index: input.index,
        expected_output: input.expected_output.clone(),
        binding: input.binding.clone(),
        min_responses,
        proof_format_id: sp1_lifter_statement::REQUIRED_PROOF_FORMAT_ID,
    };
}

/// The reference consumer expectation for the hardened-child statement, from its parts.
///
/// The same rule as [`expectation_for`]: the fingerprint is **recomputed** over the public-only
/// circuit, never copied from the journal under test, because a copied fingerprint makes the
/// gate's fingerprint clause unreachable.
pub fn expectation_for_statement(
    chain_code: &[u8],
    index: u32,
    expected_output: &zkboo::word::Words,
    binding: &[u8],
    min_responses: u32,
) -> sp1_lifter_statement::JournalExpectation {
    let public_circuit = HardenedChildCircuit::public_only(chain_code.to_vec(), index);
    return sp1_lifter_statement::JournalExpectation {
        fingerprint: zkboo_circuit_hash::hash_circuit::<_, H>(&public_circuit),
        chain_code: chain_code.to_vec(),
        index,
        expected_output: expected_output.clone(),
        binding: binding.to_vec(),
        min_responses,
        proof_format_id: sp1_lifter_statement::REQUIRED_PROOF_FORMAT_ID,
    };
}

/// The ZKBoo proof format the elephant statement requires.
///
/// Pinned rather than read from the linked `zkboo`, for the reason
/// `sp1_lifter_statement::REQUIRED_PROOF_FORMAT_ID` gives: an expectation taken from whatever a
/// consumer happens to link tracks that consumer instead of constraining it, so a consumer that
/// downgraded would silently begin *requiring* the older format. The elephant driver used to pass
/// `zkboo::PROOF_FORMAT_VERSION` straight through, which is exactly the check-that-cannot-fail
/// the other crate documents itself as existing to prevent.
///
/// It is the statement crate's constant rather than a second copy of the same digest. Both
/// statements are lifted from the same ZKBoo proof format, and two literals of it would be two
/// records of one fact — which is the arrangement that let `PROOF_FORMAT_VERSION` sit at `3`
/// across an incompatible change. This crate already links the statement crate, so naming it
/// here adds no dependency and moves no guest key.
pub const REQUIRED_ELEPHANT_PROOF_FORMAT_ID: [u8; 32] =
    sp1_lifter_statement::REQUIRED_PROOF_FORMAT_ID;

/// The reference consumer expectation for the elephant statement, with the fingerprint
/// **recomputed** over the public-only circuit rather than copied from the journal under test.
///
/// 🚨 Build expectations with this, never field by field from the journal being checked. A
/// fingerprint copied out of that journal makes the gate's `FingerprintMismatch` arm
/// unreachable, so it reports green against precisely the circuit drift it exists to catch.
///
/// 🚨 It also enforces a requirement nothing else in this repository states: **the last output
/// word is the circuit's assertion word, and it must be 1.** Prover advice — the affine comb's
/// slopes, which the comb mirrors on the host from the scalar and feeds in as secret inputs — is
/// unconstrained except by the in-circuit assertions accumulated into that word, so a proof of
/// `(any address the prover likes, assertion = 0)` is honestly obtainable by anyone with no seed
/// knowledge at all. A relying party that checks only the twenty address bytes, or that builds an
/// expectation ending in 0, is forgeable for any address. Every expectation this repository has
/// built happens to satisfy it, because they all come from executing a genuine witness — and
/// "happens to" is not "is required to".
///
/// This lives in the host crate on purpose. The obvious home is `sp1-lifter-elephant`, but the
/// elephant guests link that crate, and a change to it moves their verifying keys — invalidating
/// the pinned leaf key and every cached leaf proof, at over an hour each to rebuild.
pub fn elephant_expectation_for(
    expected_output: &zkboo::word::Words,
    binding: &[u8],
    min_responses: u32,
) -> Result<sp1_lifter_elephant::ElephantExpectation, String> {
    let assertion = expected_output
        .as_vec::<u8>()
        .last()
        .copied()
        .ok_or_else(|| "expected output carries no u8 words, so it has no assertion word".to_string())?;
    if assertion != 1 {
        return Err(format!(
            "expected output's trailing assertion word is {assertion}, not 1: the circuit's \
             in-circuit assertions are not required to hold and the statement proves nothing"
        ));
    }
    return Ok(sp1_lifter_elephant::ElephantExpectation {
        fingerprint: zkboo_circuit_hash::hash_circuit::<_, H>(
            &sp1_lifter_elephant::EthereumAddressCircuit::public_only(),
        ),
        expected_output: expected_output.clone(),
        binding: binding.to_vec(),
        min_responses,
        proof_format_id: REQUIRED_ELEPHANT_PROOF_FORMAT_ID,
    });
}

/// Decodes the program's committed public values into a [`LiftJournal`].
///
/// The decode is exact-length: trailing bytes after the journal are refused, so the
/// public values cannot smuggle data past the decoder. Any Solidity-side decoder of
/// the same public values must enforce the same full-consumption rule.
pub fn decode_journal(public_values: &[u8]) -> Result<sp1_lifter_statement::LiftJournal, String> {
    let (journal, rest) = postcard::take_from_bytes(public_values)
        .map_err(|e| format!("journal decode failed: {e}"))?;
    if !rest.is_empty() {
        return Err(format!("journal decode failed: {} trailing bytes", rest.len()));
    }
    return Ok(journal);
}

/// Executes the lift program on the input, enforcing a SUCCESS exit.
///
/// SP1 reports panicked executions as `Ok` with a non-zero exit code (they are even
/// provable, under an explicit non-success status), so success must be checked
/// explicitly. On a refusal, the program commits a diagnostic tag as its public
/// values; that tag becomes the error string here, naming the refuser.
pub fn execute_lift(
    client: &EnvProver,
    input: &LiftInput,
) -> Result<(sp1_lifter_statement::LiftJournal, sp1_sdk::ExecutionReport), String> {
    let (public_values, report) = client
        .execute(LIFT_ELF, lift_stdin(input))
        .run()
        .map_err(|e| format!("{e:#}"))?;
    // Success is exit code 0 and nothing else; never rewrite this as an equality test
    // against a particular failure code — SP1 merges shard exit codes bitwise (OR), so
    // a failure's reported code is not guaranteed to be any single canonical value.
    if report.exit_code != 0 {
        return Err(format!(
            "program refused (exit code {}): {}",
            report.exit_code,
            String::from_utf8_lossy(public_values.as_slice()),
        ));
    }
    let journal = decode_journal(public_values.as_slice())?;
    return Ok((journal, report));
}

/// Builds the prover client for a run, together with a one-line record of the proving
/// configuration it will use.
///
/// `SP1CoreOpts::default()` reads `SHARD_SIZE`, `ELEMENT_THRESHOLD`, `HEIGHT_THRESHOLD`
/// and `TRACE_CHUNK_SLOTS` from the environment, so proving cost is tunable without a
/// rebuild. That cuts both ways: a timing is meaningless unless the configuration it was
/// measured under is recorded next to it, so every driver that proves prints this
/// summary first. (`s1-acceptance` and `cycle-probe` only execute, and print nothing.)
pub fn prover_client() -> (EnvProver, String) {
    let opts = SP1CoreOpts::default();
    let summary = format!(
        "backend={} shard_size={} element_threshold={} height_threshold={} \
         trace_chunk_slots={} memory_limit={} rayon_threads={}",
        prover_backend_report(),
        opts.shard_size,
        opts.sharding_threshold.element_threshold,
        opts.sharding_threshold.height_threshold,
        opts.trace_chunk_slots,
        opts.memory_limit,
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "unset".to_string()),
    );
    let summary = format!("{summary}\n  zkboo: {}", zkboo_provenance());
    return (EnvProver::from_env_with_opts(Some(opts)), summary);
}

/// The prover backend `SP1_PROVER` selects, defaulting to `cpu`, together with whether
/// this binary was built with GPU proving compiled in and which of its phases are armed.
///
/// ⚠️ This is the setting that decides whether a run means anything: under `mock` the
/// prover fabricates a proof and its verifier only compares public inputs, so a driver
/// would print the same success line and write the same artefact paths as a real run.
/// The core options above are handed only to the CPU prover: `from_env_with_opts`
/// passes them to the `cpu` arm and drops them for every other backend, so a summary
/// omitting the backend could be describing settings that were not in force.
///
/// 🪤 `SP1_PROVER` alone is **not** enough, and reporting it alone was a defect. Metal
/// proving is a compile-time feature inside the `cpu` arm, so a GPU run and a CPU run
/// both print `cpu`, and the two differ by more than a factor of two — the largest
/// single lever on every timing this repository records. Worse, `SP1_METAL_MERKLE=0` is
/// mandatory for an unattended run (see `knowledge/gpu-corruption.md`), and a timing
/// taken without it is not comparable to one taken with it. Both now appear here, so a
/// summary line pins the configuration a number was actually measured under.
pub fn prover_backend() -> String {
    return std::env::var("SP1_PROVER").unwrap_or_else(|_| "cpu".to_string());
}

/// The same, decorated with the GPU configuration, **for the summary line only**.
///
/// 🚨 Keep this separate from `prover_backend`. Every driver guards with
/// `assert_ne!(prover_backend(), "mock", ...)`, and folding the decoration into that function
/// made the guard compare `"mock/metal-not-built"` against `"mock"` — so it could no longer
/// fire, in all four drivers at once, including the one that writes on-chain fixtures. The
/// bug was introduced by the very change that added this reporting, and it is exactly the
/// failure the guard exists to prevent: a fabricated proof printing the same success line as
/// a real one. A reporting string must never be the thing a check compares.
pub fn prover_backend_report() -> String {
    let selected = prover_backend();
    if !cfg!(feature = "metal") {
        return format!("{selected}/metal-not-built");
    }
    let mut switches: Vec<String> = std::env::vars()
        .filter(|(name, _)| name.starts_with("SP1_METAL"))
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    if switches.is_empty() {
        return format!("{selected}/metal-built-all-phases-default");
    }
    switches.sort();
    return format!("{selected}/metal-built[{}]", switches.join(" "));
}

/// The complete relying-party gate over a lifted SP1 proof: verifies the proof against
/// the expected verifying key (which alone binds the crypto suite and circuit family —
/// the image-id analogue), decodes the journal from the public values, and passes it
/// through `check_journal`. Consumers of on-disk proof fixtures must use this — never
/// trust a stored proof or decode its public values blindly.
pub fn check_lifted_proof(
    client: &EnvProver,
    proof: &SP1ProofWithPublicValues,
    vk: &SP1VerifyingKey,
    expectation: &sp1_lifter_statement::JournalExpectation,
) -> Result<sp1_lifter_statement::LiftJournal, String> {
    client
        .verify(proof, vk, None)
        .map_err(|e| format!("proof verification failed: {e:#}"))?;
    let journal = decode_journal(proof.public_values.as_slice())?;
    sp1_lifter_statement::check_journal(&journal, expectation)
        .map_err(|e| format!("journal gate refused: {e:?}"))?;
    return Ok(journal);
}

/// Returns a copy of `response` with one bit of its AND-message payload flipped — a
/// minimal structured tamper for refusal tests.
pub fn tamper_and_msg(response: &Response<S, S>) -> Response<S, S> {
    let mut msg = response.and_msg_next_party().clone();
    if !msg.u64.is_empty() {
        msg.u64[0] ^= 1;
    } else if !msg.u32.is_empty() {
        msg.u32[0] ^= 1;
    } else {
        msg.u8[0] ^= 1;
    }
    return rebuild_response(response, msg);
}

/// Returns a copy of `response` with one extra word appended to its AND-message payload —
/// a structurally malformed response (shape mismatch) for refusal tests.
pub fn oversize_and_msg(response: &Response<S, S>) -> Response<S, S> {
    let mut msg = response.and_msg_next_party().clone();
    msg.u64.push(0);
    return rebuild_response(response, msg);
}

fn rebuild_response(response: &Response<S, S>, msg: Words) -> Response<S, S> {
    let seeds = response.seeds();
    let digest = *response.commitment_digest_unopened();
    return match response.challenge().index() {
        0 => Response::new_0(*seeds[0], *seeds[1], digest, msg),
        1 => Response::new_1(
            digest,
            *seeds[0],
            *seeds[1],
            msg,
            response.input_share_2().unwrap().clone(),
        ),
        _ => Response::new_2(
            *seeds[1],
            digest,
            *seeds[0],
            msg,
            response.input_share_2().unwrap().clone(),
        ),
    };
}


// ─── split-and-aggregate probe ────────────────────────────────────────────────────

/// Assembles a [`LeafInput`](sp1_lifter_split::LeafInput) frame for one response.
pub fn leaf_input_for(
    input: &LiftInput,
    position: usize,
) -> sp1_lifter_split::LeafInput {
    return sp1_lifter_split::LeafInput {
        version: sp1_lifter_split::LEAF_FRAME_VERSION,
        chain_code: input.chain_code.clone(),
        index: input.index,
        expected_output: input.expected_output.clone(),
        binding: input.binding.clone(),
        position: position as u32,
        response: input.proof[position].clone(),
    };
}

/// Builds the leaf program's stdin.
pub fn leaf_stdin(input: &sp1_lifter_split::LeafInput) -> SP1Stdin {
    let frame = postcard::to_allocvec(input).expect("serialize leaf frame");
    let mut stdin = SP1Stdin::new();
    stdin.write_vec(frame);
    return stdin;
}

/// Computes a leaf journal natively — the same replay the leaf guest performs.
///
/// Used to exercise the cross-leaf gate without paying for zkVM proofs, and to check
/// that the guest's journal is the one the host expects.
pub fn leaf_journal_native(
    input: &LiftInput,
    position: usize,
) -> sp1_lifter_split::LeafJournal {
    use zkboo::{circuit::Circuit, verifier::replay::ViewReplayerBackend};
    let response = &input.proof[position];
    assert!(response.is_well_formed(), "response {position} is malformed");
    let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    let replayer = ViewReplayerBackend::<H, PV, S, WPP>::new(response).into_view_replayer();
    circuit.exec(&replayer);
    let commitments = replayer
        .finalize_with_arg(&input.expected_output)
        .expect("native replay");
    return sp1_lifter_split::LeafJournal {
        version: sp1_lifter_split::LEAF_JOURNAL_VERSION,
        position: position as u32,
        statement_digest: sp1_lifter_split::statement_digest(
            &input.chain_code,
            input.index,
            &input.expected_output,
            &input.binding,
        ),
        party: response.challenge().index() as u8,
        commitments: commitments.map(|c| sp1_lifter_split::LeafCommitment {
            digest: *c.digest(),
            output_share: c.output_share().clone(),
        }),
    };
}

/// Assembles the root frame from the leaf journals' public-value bytes.
pub fn root_input_for(
    input: &LiftInput,
    leaf_vk: [u32; 8],
    leaf_public_values: Vec<Vec<u8>>,
) -> sp1_lifter_split::RootInput {
    return sp1_lifter_split::RootInput {
        version: sp1_lifter_split::ROOT_FRAME_VERSION,
        chain_code: input.chain_code.clone(),
        index: input.index,
        expected_output: input.expected_output.clone(),
        binding: input.binding.clone(),
        leaf_vk,
        leaf_public_values,
    };
}

/// Decodes the root program's committed public values.
pub fn decode_root_journal(
    public_values: &[u8],
) -> Result<sp1_lifter_split::RootJournal, String> {
    let (journal, rest): (sp1_lifter_split::RootJournal, _) =
        postcard::take_from_bytes(public_values)
            .map_err(|e| format!("root journal decode failed: {e}"))?;
    if !rest.is_empty() {
        return Err(format!("root journal decode failed: {} trailing bytes", rest.len()));
    }
    // The outer version was committed by every root and checked by nobody. Postcard is not
    // self-describing, so on the first layout change an unchecked version is a silent misparse
    // rather than an error: the fields would decode into the wrong shape and the gate would run
    // on nonsense. Only version 1 exists today, which is exactly when this is free to add.
    if journal.version != sp1_lifter_split::ROOT_JOURNAL_VERSION {
        return Err(format!(
            "root journal version is {}, not {}",
            journal.version,
            sp1_lifter_split::ROOT_JOURNAL_VERSION
        ));
    }
    return Ok(journal);
}

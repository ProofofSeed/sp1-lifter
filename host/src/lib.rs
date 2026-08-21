// SPDX-License-Identifier: LGPL-3.0-or-later

//! Shared host-side helpers for the SNARK lifter: the development statement fixture,
//! the program input framing, and SP1 execution/verification drivers.

use sp1_core_executor::SP1CoreOpts;
use sp1_sdk::{
    Elf, SP1ProofWithPublicValues, SP1Stdin, SP1VerifyingKey,
    blocking::{EnvProver, Prover},
    include_elf,
};
use snark_lifter_statement::{H, HardenedChildCircuit, LiftInput, PS, PV, S};
use zkboo::{
    executor::{OwnedFlexibleWordPool, exec},
    prover::{par_prove, proof::Response, views::OwnedFlexibleWordTriplePool},
    verifier::replay::OwnedFlexibleWordPairPool,
    word::Words,
};
pub use zkboo_bip32::HARDENED_OFFSET;

/// The lift program ELF, built by `sp1-build` from `program/`.
pub const LIFT_ELF: Elf = include_elf!("snark-lifter-program");

/// Executor-side word pool.
pub type WP = OwnedFlexibleWordPool<usize>;
/// Prover-side word-triple pool.
pub type WTP = OwnedFlexibleWordTriplePool<usize>;
/// Verifier-side word-pair pool.
pub type WPP = OwnedFlexibleWordPairPool<usize>;

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
    let output = exec::<_, WP>(&circuit);
    let proof = par_prove::<_, H, PS, PV, S, WTP>(&circuit, num_iters, seed_entropy, binding);
    return (circuit, output, proof);
}

/// Assembles a [`LiftInput`] frame for the hardened-child statement.
pub fn lift_input_for(
    chain_code: Vec<u8>,
    index: u32,
    output: Words,
    binding: &[u8],
    proof: snark_lifter_statement::LiftProof,
) -> LiftInput {
    return LiftInput {
        version: snark_lifter_statement::LIFT_FRAME_VERSION,
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
) -> snark_lifter_statement::JournalExpectation {
    let public_circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
    return snark_lifter_statement::JournalExpectation {
        fingerprint: zkboo_circuit_hash::hash_circuit::<_, H>(&public_circuit),
        chain_code: input.chain_code.clone(),
        index: input.index,
        expected_output: input.expected_output.clone(),
        binding: input.binding.clone(),
        min_responses,
        proof_format_version: snark_lifter_statement::REQUIRED_PROOF_FORMAT_VERSION,
    };
}

/// Decodes the program's committed public values into a [`LiftJournal`].
///
/// The decode is exact-length: trailing bytes after the journal are refused, so the
/// public values cannot smuggle data past the decoder. Any Solidity-side decoder of
/// the same public values must enforce the same full-consumption rule.
pub fn decode_journal(public_values: &[u8]) -> Result<snark_lifter_statement::LiftJournal, String> {
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
) -> Result<(snark_lifter_statement::LiftJournal, sp1_sdk::ExecutionReport), String> {
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
        prover_backend(),
        opts.shard_size,
        opts.sharding_threshold.element_threshold,
        opts.sharding_threshold.height_threshold,
        opts.trace_chunk_slots,
        opts.memory_limit,
        std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "unset".to_string()),
    );
    return (EnvProver::from_env_with_opts(Some(opts)), summary);
}

/// The prover backend `SP1_PROVER` selects, defaulting to `cpu`.
///
/// ⚠️ This is the setting that decides whether a run means anything: under `mock` the
/// prover fabricates a proof and its verifier only compares public inputs, so a driver
/// would print the same success line and write the same artefact paths as a real run.
/// The core options above also reach proving only on the CPU path — no other backend
/// proves with them (CUDA forwards them to its execution node but not to the GPU
/// prover) — so a summary omitting the backend could describe settings not in force.
pub fn prover_backend() -> String {
    return std::env::var("SP1_PROVER").unwrap_or_else(|_| "cpu".to_string());
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
    expectation: &snark_lifter_statement::JournalExpectation,
) -> Result<snark_lifter_statement::LiftJournal, String> {
    client
        .verify(proof, vk, None)
        .map_err(|e| format!("proof verification failed: {e:#}"))?;
    let journal = decode_journal(proof.public_values.as_slice())?;
    snark_lifter_statement::check_journal(&journal, expectation)
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

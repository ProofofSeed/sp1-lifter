// SPDX-License-Identifier: LGPL-3.0-or-later

//! Shared host-side helpers for the SNARK lifter: the development statement fixture,
//! the guest input framing, and executor drivers.

use risc0_zkvm::{ExecutorEnv, SessionInfo, default_executor};
use snark_lifter_statement::{H, HardenedChildCircuit, LiftInput, PS, PV, S};
use zkboo::{
    executor::{OwnedFlexibleWordPool, exec},
    prover::{par_prove, proof::Response, views::OwnedFlexibleWordTriplePool},
    verifier::replay::OwnedFlexibleWordPairPool,
    word::Words,
};
pub use zkboo_bip32::HARDENED_OFFSET;

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

/// Packs a [`LiftInput`] into the guest's length-prefixed little-endian u32-word frame.
pub fn pack_frame(input: &LiftInput) -> (u32, Vec<u32>) {
    let frame = postcard::to_allocvec(input).expect("serialize input frame");
    let mut padded = frame.clone();
    padded.resize(frame.len().div_ceil(4) * 4, 0);
    let words: Vec<u32> = padded
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    return (frame.len() as u32, words);
}

/// Builds the executor environment carrying the packed [`LiftInput`] frame.
pub fn guest_env(input: &LiftInput) -> Result<ExecutorEnv<'static>, String> {
    return guest_env_po2(input, None);
}

/// [`guest_env`] with an optional segment size limit (log2 of cycles per segment).
pub fn guest_env_po2(
    input: &LiftInput,
    segment_limit_po2: Option<u32>,
) -> Result<ExecutorEnv<'static>, String> {
    let (frame_len, words) = pack_frame(input);
    let mut builder = ExecutorEnv::builder();
    builder
        .write(&frame_len)
        .map_err(|e| format!("{e:#}"))?
        .write_slice(&words);
    if let Some(po2) = segment_limit_po2 {
        builder.segment_limit_po2(po2);
    }
    return builder.build().map_err(|e| format!("{e:#}"));
}

/// Runs the given guest ELF on the input in the RISC Zero executor (no proving).
pub fn run_guest(elf: &[u8], input: &LiftInput) -> Result<SessionInfo, String> {
    return default_executor()
        .execute(guest_env(input)?, elf)
        .map_err(|e| format!("{e:#}"));
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
    };
}

/// The complete relying-party gate over a lifted receipt: verifies the receipt against
/// the expected guest image id (which alone binds the crypto suite and circuit family),
/// decodes the journal, and passes it through [`check_journal`]. Consumers of on-disk
/// receipt fixtures must use this — never trust a stored receipt or decode its journal
/// blindly.
pub fn check_lifted_receipt(
    receipt: &risc0_zkvm::Receipt,
    image_id: [u32; 8],
    expectation: &snark_lifter_statement::JournalExpectation,
) -> Result<snark_lifter_statement::LiftJournal, String> {
    receipt
        .verify(image_id)
        .map_err(|e| format!("receipt verification failed: {e:#}"))?;
    let journal: snark_lifter_statement::LiftJournal = receipt
        .journal
        .decode()
        .map_err(|e| format!("journal decode failed: {e:#}"))?;
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

/// Returns a copy of `response` with one extra word appended to its AND-message payload —
/// a structurally malformed response (shape mismatch) for refusal tests.
pub fn oversize_and_msg(response: &Response<S, S>) -> Response<S, S> {
    let mut msg = response.and_msg_next_party().clone();
    msg.u64.push(0);
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

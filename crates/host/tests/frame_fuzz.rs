// SPDX-License-Identifier: LGPL-3.0-or-later

//! Mutated input frames never verify as anything but the honest statement.
//!
//! Mirrors the lift guest natively: decode a `LiftInput` frame, rebuild the circuit from its
//! public parameters, verify the proof it carries and read off what the journal would commit.
//! Every mutated frame must be refused by the decoder, refused by the verifier or end in a
//! panic; the one other outcome allowed is an accepting verification of exactly the honest
//! statement, which a mutation of bytes the decoder never reads can produce.
//!
//! The proof carries 32 responses, so that a mutated proof passing the challenge match by
//! chance has probability 3^-32: at three responses it is 1 in 27, and a run at that count
//! accepted a substituted byte on its 25th round.

use std::panic::{AssertUnwindSafe, catch_unwind};

use pofseed_sp1_lift_host::{HARDENED_OFFSET, WPP, lift_input_for, vector_1_proof};
use pofseed_sp1_lift_statement::{
    H, HardenedChildCircuit, LIFT_FRAME_VERSION, LiftInput, PV, S,
};
use zkboo::{
    Repetitions,
    verifier::{VerifyOptions, verify},
    word::Words,
};
use zkboo_circuit_hash::hash_circuit;

const NUM_ITERS: usize = 32;
const SEED_ENTROPY: &[u8] = b"snark-lifter frame fuzz entropy";
const BINDING: &[u8] = b"snark-lifter frame fuzz binding";
const ROUNDS: usize = 256;
const HEADER_BYTES: usize = 96;

/// What the lift guest commits about a frame it accepts.
#[derive(Debug, PartialEq, Eq)]
struct Statement {
    fingerprint: S,
    chain_code: Vec<u8>,
    index: u32,
    expected_output: Words,
    binding: Vec<u8>,
    num_responses: usize,
}

/// How the guest's logic disposes of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    DecodeRefused,
    VersionRefused,
    EmptyRefused,
    VerifyRefused,
    Panicked,
    AcceptedHonest,
    Forged,
}

/// Deterministic xorshift64 generator, so a failure names a reproducible round.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        return x;
    }

    fn below(&mut self, n: usize) -> usize {
        return (self.next() % (n as u64)) as usize;
    }
}

/// Runs the guest's decode-and-verify logic on a frame, natively.
fn evaluate(frame: &[u8], honest: &Statement) -> Outcome {
    let input: LiftInput = match postcard::from_bytes(frame) {
        Ok(input) => input,
        Err(_) => return Outcome::DecodeRefused,
    };
    if input.version != LIFT_FRAME_VERSION {
        return Outcome::VersionRefused;
    }
    if input.proof.is_empty() {
        return Outcome::EmptyRefused;
    }
    let verdict = catch_unwind(AssertUnwindSafe(|| {
        let circuit = HardenedChildCircuit::public_only(input.chain_code.clone(), input.index);
        let fingerprint = hash_circuit::<_, H>(&circuit);
        let valid = verify::<_, H, PV, S, WPP, _>(
            &circuit,
            &input.expected_output,
            &input.proof,
            &input.binding,
            Repetitions::exactly(input.proof.len()),
            VerifyOptions::new(),
        );
        (fingerprint, valid)
    }));
    return match verdict {
        Err(_) => Outcome::Panicked,
        Ok((_, Err(_))) | Ok((_, Ok(false))) => Outcome::VerifyRefused,
        Ok((fingerprint, Ok(true))) => {
            let statement = Statement {
                fingerprint,
                chain_code: input.chain_code,
                index: input.index,
                expected_output: input.expected_output,
                binding: input.binding,
                num_responses: input.proof.len(),
            };
            if &statement == honest {
                Outcome::AcceptedHonest
            } else {
                Outcome::Forged
            }
        }
    };
}

/// Applies one random mutation to a copy of the frame, returning it with the mutation's name.
fn mutate(rng: &mut Rng, frame: &[u8]) -> (Vec<u8>, &'static str) {
    let mut out = frame.to_vec();
    let kind = rng.below(7);
    let name = match kind {
        0 => {
            let i = rng.below(out.len());
            out[i] ^= 1 << rng.below(8);
            "bit flip"
        }
        1 => {
            let i = rng.below(HEADER_BYTES.min(out.len()));
            out[i] ^= 1 << rng.below(8);
            "header bit flip"
        }
        2 => {
            let i = rng.below(out.len());
            out[i] = rng.next() as u8;
            "byte substitution"
        }
        3 => {
            out.truncate(rng.below(out.len()));
            "truncation"
        }
        4 => {
            let i = rng.below(out.len() + 1);
            out.insert(i, rng.next() as u8);
            "byte insertion"
        }
        5 => {
            let i = rng.below(out.len());
            out.remove(i);
            "byte deletion"
        }
        _ => {
            let i = rng.below(out.len());
            let run = (1 + rng.below(16)).min(out.len() - i);
            out[i..i + run].fill(0);
            "zero run"
        }
    };
    return (out, name);
}

#[test]
fn mutated_frames_never_verify_as_another_statement() {
    std::panic::set_hook(Box::new(|_| {}));
    let (circuit, output, proof) = vector_1_proof(NUM_ITERS, SEED_ENTROPY, BINDING);
    let public = HardenedChildCircuit::public_only(circuit.chain_code().to_vec(), HARDENED_OFFSET);
    let honest = Statement {
        fingerprint: hash_circuit::<_, H>(&public),
        chain_code: circuit.chain_code().to_vec(),
        index: HARDENED_OFFSET,
        expected_output: output.clone(),
        binding: BINDING.to_vec(),
        num_responses: NUM_ITERS,
    };
    let input = lift_input_for(circuit.chain_code().to_vec(), HARDENED_OFFSET, output, BINDING, proof);
    let frame = postcard::to_allocvec(&input).expect("serialize input frame");
    // Controls: the honest frame is accepted, and the decoder's tolerance of trailing bytes is
    // what makes "accepted as the honest statement" a possible outcome of a mutation.
    assert_eq!(evaluate(&frame, &honest), Outcome::AcceptedHonest);
    let mut trailing = frame.clone();
    trailing.extend_from_slice(b"trailing");
    assert_eq!(evaluate(&trailing, &honest), Outcome::AcceptedHonest);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut tally: Vec<(&'static str, Outcome, bool)> = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let (mutated, name) = mutate(&mut rng, &frame);
        let outcome = evaluate(&mutated, &honest);
        tally.push((name, outcome, mutated.starts_with(&frame)));
    }
    let _ = std::panic::take_hook();
    for (round, (name, outcome, extends_honest)) in tally.iter().enumerate() {
        assert_ne!(*outcome, Outcome::Forged, "round {round} ({name}) verified as another statement");
        // Accepting the honest statement is benign only for bytes the decoder never reads.
        assert!(
            *outcome != Outcome::AcceptedHonest || *extends_honest,
            "round {round} ({name}) is accepted although it differs within the decoded bytes"
        );
    }
    for outcome in [
        Outcome::DecodeRefused,
        Outcome::VersionRefused,
        Outcome::EmptyRefused,
        Outcome::VerifyRefused,
        Outcome::Panicked,
        Outcome::AcceptedHonest,
    ] {
        let n = tally.iter().filter(|(_, o, _)| *o == outcome).count();
        println!("{outcome:?}: {n}");
    }
}

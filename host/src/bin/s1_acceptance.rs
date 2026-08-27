// SPDX-License-Identifier: LGPL-3.0-or-later

//! Acceptance run for full-scale lifted verification and the reject matrix, on SP1.
//!
//! Positive case: a genuine 219-response hardened-child proof (the classical-128
//! repetition count) verifies in the lift program, and the committed journal passes the
//! reference consumer gate (`check_journal`) against host-computed expectations —
//! fingerprint included, recomputed host-side over the public-only circuit.
//!
//! Reject matrix (small repetition counts; each case names its refuser):
//! - R1 tampered proof (one AND-message bit flipped) → program refuses ("ZKBoo proof rejected")
//! - R2 wrong binding (proved for tx A, verified under tx B) → program refuses
//! - R3 wrong fingerprint expectation → `check_journal` FingerprintMismatch
//! - R4 too few responses (8 < 219) → `check_journal` TooFewResponses
//! - R5 empty proof (accepted by the ZKBoo verifier by design) → `check_journal` TooFewResponses
//! - R8 wrong proof format version → `check_journal` ProofFormatMismatch
//! - R6 wrong statement expectation → `check_journal` StatementMismatch
//! - R7 structurally malformed response (oversized AND-message) → replay shape check
//!   ("view replay error"), fail-closed
//!
//! Usage: `s1-acceptance [full|rejects|both]` (default `both`). `rejects` skips the
//! full-scale positive run, e.g. for check-removal validation of the refusal tests.

use std::time::Instant;

use sp1_sdk::blocking::{EnvProver, ProverClient};
use snark_lifter_host::{
    HARDENED_OFFSET, bip32_vector_1, execute_lift, expectation_for, lift_input_for,
    oversize_and_msg, tamper_and_msg, vector_1_proof,
};
use snark_lifter_statement::{
    JournalCheckError, JournalExpectation, LiftInput, LiftJournal, check_journal,
};
use zkboo::params::n_eps;

const SEED_ENTROPY: &[u8] = b"snark-lifter S1 acceptance entropy";
const BINDING: &[u8] = b"snark-lifter S1 acceptance tx binding";

fn lift_input(
    chain_code: Vec<u8>,
    output: zkboo::word::Words,
    binding: &[u8],
    proof: snark_lifter_statement::LiftProof,
) -> LiftInput {
    return lift_input_for(chain_code, HARDENED_OFFSET, output, binding, proof);
}

fn run_program(client: &EnvProver, input: &LiftInput) -> Result<LiftJournal, String> {
    return execute_lift(client, input).map(|(journal, _)| journal);
}

// The refusal error string carries both the program's committed diagnostic tag (its
// public values) and, via SP1's executor error path, the panic message — the needle
// matches whichever surfaces, and both name the same refuser by construction.
fn expect_program_refusal(client: &EnvProver, name: &str, input: &LiftInput, needle: &str) {
    match run_program(client, input) {
        Ok(_) => panic!("{name}: program ACCEPTED an input it must refuse"),
        Err(e) => {
            assert!(
                e.contains(needle),
                "{name}: program failed, but not via the named refuser ({needle}); error: {e}"
            );
            println!("  {name}: refused by program ({needle}) ✓");
        }
    }
}

fn expect_consumer_refusal(
    name: &str,
    journal: &LiftJournal,
    exp: &JournalExpectation,
    expected_err: JournalCheckError,
) {
    match check_journal(journal, exp) {
        Err(e) if e == expected_err => println!("  {name}: refused by check_journal ({e:?}) ✓"),
        other => panic!("{name}: expected Err({expected_err:?}), got {other:?}"),
    }
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "both".into());
    let full = matches!(mode.as_str(), "full" | "both");
    let rejects = matches!(mode.as_str(), "rejects" | "both");
    assert!(full || rejects, "usage: s1-acceptance [full|rejects|both]");
    let client = ProverClient::from_env();

    if rejects {
        println!("── reject matrix (n=8) ──");
        println!("  zkboo: {}", snark_lifter_host::zkboo_provenance());
        let n = 8;
        let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        let (_, chain_code) = bip32_vector_1();

        // R1: tampered proof — flip one AND-message bit in one response.
        let mut tampered = proof.clone();
        tampered[3] = tamper_and_msg(&tampered[3]);
        let input = lift_input(chain_code.clone(), output.clone(), BINDING, tampered);
        expect_program_refusal(&client, "R1 tampered proof", &input, "ZKBoo proof rejected");

        // R2: wrong binding — proof bound to tx A, verified under tx B.
        let input = lift_input(chain_code.clone(), output.clone(), b"some other tx", proof.clone());
        expect_program_refusal(&client, "R2 wrong binding", &input, "ZKBoo proof rejected");

        // Accepting small run, reused by the consumer-side refusals.
        let input = lift_input(chain_code.clone(), output.clone(), BINDING, proof.clone());
        let journal = run_program(&client, &input).expect("control run must accept");
        check_journal(&journal, &expectation_for(&input, n as u32))
            .expect("control gate must pass");
        println!("  control (n=8, matching expectation): accepted ✓");

        // R3: wrong fingerprint expectation.
        let mut exp = expectation_for(&input, n as u32);
        exp.fingerprint[0] ^= 1;
        expect_consumer_refusal(
            "R3 wrong fingerprint",
            &journal,
            &exp,
            JournalCheckError::FingerprintMismatch,
        );

        // R4: too few responses — the consumer requires the classical-128 count.
        let exp = expectation_for(&input, n_eps(64) as u32);
        expect_consumer_refusal(
            "R4 too few responses",
            &journal,
            &exp,
            JournalCheckError::TooFewResponses,
        );

        // R5: empty proof — the ZKBoo verifier accepts it BY DESIGN; the count gate
        // must be what refuses it.
        let input_empty = lift_input(chain_code.clone(), output.clone(), BINDING, Vec::new());
        let empty_journal = run_program(&client, &input_empty)
            .expect("empty proof is accepted by the ZKBoo verifier (documented)");
        assert_eq!(empty_journal.num_responses, 0);
        let exp = expectation_for(&input_empty, 1);
        expect_consumer_refusal(
            "R5 empty proof",
            &empty_journal,
            &exp,
            JournalCheckError::TooFewResponses,
        );

        // R6: wrong statement expectation (different expected output).
        let mut exp = expectation_for(&input, 0);
        exp.expected_output.u8[0] ^= 1;
        expect_consumer_refusal(
            "R6 wrong statement",
            &journal,
            &exp,
            JournalCheckError::StatementMismatch,
        );

        // R8: wrong proof format — the circuit fingerprint is blind to a change of the
        // view-commitment scheme, so the format the program attests must be checked in
        // its own right. Note the limit of what this can demonstrate: only one guest
        // build exists here, so mutating the expectation shows that the comparison
        // fires, not that a genuinely mis-formatted proof is caught. The threat it
        // guards is a consumer that upgrades zkboo while still pinning an older
        // verifying key, which no single-build test can stage.
        let mut exp = expectation_for(&input, n as u32);
        exp.proof_format_version += 1;
        expect_consumer_refusal(
            "R8 wrong proof format",
            &journal,
            &exp,
            JournalCheckError::ProofFormatMismatch,
        );

        // R7: structurally malformed response (oversized AND-message vector) — refused
        // by the replay shape check, surfacing as a view-replay error. Fail-closed: no
        // proof can exist for it.
        let mut malformed = proof.clone();
        malformed[5] = oversize_and_msg(&malformed[5]);
        let input_malformed = lift_input(chain_code.clone(), output.clone(), BINDING, malformed);
        expect_program_refusal(&client, "R7 malformed response", &input_malformed, "view replay error");
        println!("reject matrix: PASS");
        println!();
    }

    if full {
        let n = n_eps(64);
        println!("── full-scale positive run (n={n}, classical-128) ──");
        let t = Instant::now();
        let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        println!("  proved {n} responses in {:.2?}", t.elapsed());
        let (_, chain_code) = bip32_vector_1();
        let input = lift_input(chain_code, output, BINDING, proof);
        let t = Instant::now();
        let (journal, report) = execute_lift(&client, &input).expect("full-scale program run");
        let wall = t.elapsed();
        check_journal(&journal, &expectation_for(&input, n as u32)).expect("consumer gate");
        assert_eq!(journal.num_responses, n as u32);
        println!(
            "  program accepted, journal passes the consumer gate: fingerprint {}…, \
             {} responses, {} instructions, executor wall {wall:.2?}",
            journal.fingerprint[..4]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            journal.num_responses,
            report.total_instruction_count(),
        );
        println!("full-scale positive run: PASS");
    }
}

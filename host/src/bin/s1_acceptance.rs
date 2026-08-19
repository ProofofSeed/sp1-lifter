// SPDX-License-Identifier: LGPL-3.0-or-later

//! Acceptance run for full-scale lifted verification and the reject matrix.
//!
//! Positive case: a genuine 219-response hardened-child proof (the classical-128
//! repetition count) verifies in the lift guest, and the committed journal passes the
//! reference consumer gate (`check_journal`) against host-computed expectations —
//! fingerprint included, recomputed host-side over the public-only circuit.
//!
//! Reject matrix (small repetition counts; each case names its refuser):
//! - R1 tampered proof (one AND-message bit flipped) → guest refuses ("ZKBoo proof rejected")
//! - R2 wrong binding (proved for tx A, verified under tx B) → guest refuses
//! - R3 wrong fingerprint expectation → `check_journal` FingerprintMismatch
//! - R4 too few responses (8 < 219) → `check_journal` TooFewResponses
//! - R5 empty proof (accepted by the ZKBoo verifier by design) → `check_journal` TooFewResponses
//! - R6 wrong statement expectation → `check_journal` StatementMismatch
//!
//! Usage: `s1-acceptance [full|rejects|both]` (default `both`). `rejects` skips the
//! ~4-minute full-scale positive run, e.g. for check-removal validation of the refusal
//! tests.

use std::time::Instant;

use snark_lifter_host::{expectation_for, run_guest, tamper_and_msg, vector_1_proof};
use snark_lifter_methods::LIFT_ELF;
use snark_lifter_statement::{
    JournalCheckError, JournalExpectation, LiftInput, LiftJournal, check_journal,
};
use zkboo::params::n_eps;
use zkboo_bip32::HARDENED_OFFSET;

const SEED_ENTROPY: &[u8] = b"snark-lifter S1 acceptance entropy";
const BINDING: &[u8] = b"snark-lifter S1 acceptance tx binding";

fn lift_input(
    chain_code: Vec<u8>,
    output: zkboo::word::Words,
    binding: &[u8],
    proof: snark_lifter_statement::LiftProof,
) -> LiftInput {
    return snark_lifter_host::lift_input_for(chain_code, HARDENED_OFFSET, output, binding, proof);
}

fn expectation(input: &LiftInput, min_responses: u32) -> JournalExpectation {
    return expectation_for(input, min_responses);
}

fn expect_guest_refusal(name: &str, input: &LiftInput, needle: &str) {
    match run_guest(LIFT_ELF, input) {
        Ok(_) => panic!("{name}: guest ACCEPTED an input it must refuse"),
        Err(e) => {
            assert!(
                e.contains(needle),
                "{name}: guest failed, but not via the named refuser ({needle}); error: {e}"
            );
            println!("  {name}: refused by guest ({needle}) ✓");
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

    if rejects {
        println!("── reject matrix (n=8) ──");
        let n = 8;
        let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        let (_, chain_code) = snark_lifter_host::bip32_vector_1();

        // R1: tampered proof — flip one AND-message bit in one response.
        let mut tampered = proof.clone();
        tampered[3] = tamper_and_msg(&tampered[3]);
        let input = lift_input(chain_code.clone(), output.clone(), BINDING, tampered);
        expect_guest_refusal("R1 tampered proof", &input, "ZKBoo proof rejected");

        // R2: wrong binding — proof bound to tx A, verified under tx B.
        let input = lift_input(chain_code.clone(), output.clone(), b"some other tx", proof.clone());
        expect_guest_refusal("R2 wrong binding", &input, "ZKBoo proof rejected");

        // Accepting small run, reused by the consumer-side refusals.
        let input = lift_input(chain_code.clone(), output.clone(), BINDING, proof.clone());
        let session = run_guest(LIFT_ELF, &input).expect("control run must accept");
        let journal: LiftJournal = session.journal.decode().expect("journal decode");
        check_journal(&journal, &expectation(&input, n as u32)).expect("control gate must pass");
        println!("  control (n=8, matching expectation): accepted ✓");

        // R3: wrong fingerprint expectation.
        let mut exp = expectation(&input, n as u32);
        exp.fingerprint[0] ^= 1;
        expect_consumer_refusal(
            "R3 wrong fingerprint",
            &journal,
            &exp,
            JournalCheckError::FingerprintMismatch,
        );

        // R4: too few responses — the consumer requires the classical-128 count.
        let exp = expectation(&input, n_eps(64) as u32);
        expect_consumer_refusal(
            "R4 too few responses",
            &journal,
            &exp,
            JournalCheckError::TooFewResponses,
        );

        // R5: empty proof — the ZKBoo verifier accepts it BY DESIGN; the count gate
        // must be what refuses it.
        let input = lift_input(chain_code.clone(), output.clone(), BINDING, Vec::new());
        let session = run_guest(LIFT_ELF, &input)
            .expect("empty proof is accepted by the ZKBoo verifier (documented)");
        let empty_journal: LiftJournal = session.journal.decode().expect("journal decode");
        assert_eq!(empty_journal.num_responses, 0);
        let exp = expectation(&input, 1);
        expect_consumer_refusal(
            "R5 empty proof",
            &empty_journal,
            &exp,
            JournalCheckError::TooFewResponses,
        );

        // R6: wrong statement expectation (different expected output).
        let mut exp = expectation(&input, 0);
        exp.expected_output.u8[0] ^= 1;
        expect_consumer_refusal(
            "R6 wrong statement",
            &journal,
            &exp,
            JournalCheckError::StatementMismatch,
        );
        println!("reject matrix: PASS");
        println!();
    }

    if full {
        let n = n_eps(64);
        println!("── full-scale positive run (n={n}, classical-128) ──");
        let t = Instant::now();
        let (_, output, proof) = vector_1_proof(n, SEED_ENTROPY, BINDING);
        println!("  proved {n} responses in {:.2?}", t.elapsed());
        let (_, chain_code) = snark_lifter_host::bip32_vector_1();
        let input = lift_input(chain_code, output, BINDING, proof);
        let t = Instant::now();
        let session = run_guest(LIFT_ELF, &input).expect("full-scale guest run");
        let wall = t.elapsed();
        let journal: LiftJournal = session.journal.decode().expect("journal decode");
        let total_cycles: u64 = session.segments.iter().map(|s| s.cycles as u64).sum();
        check_journal(&journal, &expectation(&input, n as u32)).expect("consumer gate");
        assert_eq!(journal.num_responses, n as u32);
        println!(
            "  guest accepted, journal passes the consumer gate: fingerprint {}…, \
             {} responses, {} user cycles, executor wall {wall:.2?}",
            journal.fingerprint[..4]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
            journal.num_responses,
            total_cycles,
        );
        println!("full-scale positive run: PASS");
    }
}

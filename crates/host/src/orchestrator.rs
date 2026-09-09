// SPDX-License-Identifier: LGPL-3.0-or-later

//! Proving leaves as responses arrive, rather than after they all have.
//!
//! The monolithic lifter cannot start until the last response exists, so device time and
//! proving time are a **sum**. The split makes them a **max**: a leaf is an independent
//! `prove` call whose input is complete the moment its response lands, so leaf *i* can be
//! proved while response *i+1* is still being produced. This is the orchestration that
//! turns that structural fact into a running pipeline. It needs no change to SP1, to
//! `zkboo`, or to the on-chain contract.
//!
//! What is modelled here, and what is not. The producer is a real ZKBoo prover emitting
//! one response at a time in exactly the order and by exactly the mechanism a device does —
//! the whole challenge phase first, because the Fiat-Shamir challenge is global and no
//! response can be built before it, and then the responses one by one. What is *not*
//! modelled is the device's clock: this machine produces a response in milliseconds where a
//! Ledger Flex takes about thirty seconds, so the pipeline here is starved rather than
//! saturated, and the number to read off is which side waits for which, not the wall clock.
//!
//! The report therefore separates the two idle times. `pool_idle` is how long provers spent
//! waiting for a response to arrive; `queue_delay` is how long responses spent waiting for a
//! prover. Exactly one of them is the bottleneck, and which one it is decides how many
//! machines a deployment needs.

use std::{
    sync::{
        Arc, Mutex,
        mpsc::{SyncSender, sync_channel},
    },
    thread,
    time::{Duration, Instant},
};

use sp1_sdk::{
    SP1ProofWithPublicValues, SP1Stdin, SP1VerifyingKey,
    blocking::{EnvProver, ProveRequest, Prover},
};
use pofseed_sp1_lift_split::LeafInput;
use pofseed_sp1_lift_statement::{H, HardenedChildCircuit, PS, PV, S};
use zkboo::{
    executor::{ExecOptions, exec},
    prover::{
        challenge::{ChallengeOptions, build_challenge_entropy},
        proof::{ProofBuilder, ProofOptions, Response, collectors::OwnedResponseDataCollector},
    },
    word::Words,
};

use crate::{WP, WTP};

/// One response, as it comes off the producer.
struct Arrival {
    position: usize,
    response: Response<S, S>,
    /// When the producer *finished computing* it, relative to the start of the run.
    ///
    /// Deliberately not the time the channel accepted it. The channel is bounded, so a
    /// producer that outruns the pool blocks inside `send`, and stamping there would fold
    /// the pool's backpressure into the producer's clock — which made the first version of
    /// this report claim an overlap saving that was really the producer being throttled by
    /// the thing it was supposed to be overlapping with.
    produced_at: Duration,
}

/// What one leaf proof cost, and when.
#[derive(Debug, Clone)]
pub struct LeafRecord {
    /// Which response this leaf is.
    pub position: usize,
    /// When the producer finished computing the response, relative to the start of the run.
    pub produced_at: Duration,
    /// When a prover picked it up.
    pub started_at: Duration,
    /// Wall-clock of the `prove` call itself.
    pub prove_wall: Duration,
}

/// The timeline of a streamed lift.
#[derive(Debug)]
pub struct StreamReport {
    /// Number of responses.
    pub num_responses: usize,
    /// Concurrent leaf provers.
    pub pool: usize,
    /// The global challenge phase, before any response can exist.
    pub challenge_wall: Duration,
    /// When the last response was finished by the producer, backpressure excluded.
    pub last_produced: Duration,
    /// Per leaf, in position order.
    pub leaves: Vec<LeafRecord>,
    /// Wall-clock of the root proof.
    pub root_wall: Duration,
    /// The whole run, challenge phase included.
    ///
    /// ⚠️ **Excludes the root program's `setup`**, which the driver runs between the streaming
    /// phase and the root timer — deliberately, since holding two proving keys resident costs
    /// 54% per leaf. So this sits a little under the process's own wall clock: on the n=8
    /// cadence run, 770.68 s against 779.55 s real. It also *includes* each leaf's host-side
    /// verification, where `serial_counterfactual` sums only `prove_wall` — a second bias, this
    /// one against the streamed run. Both are about 1%, and neither is hidden.
    pub end_to_end: Duration,
    /// Summed time provers spent waiting for a response to exist.
    pub pool_idle: Duration,
    /// Summed time finished responses spent waiting for a free prover.
    pub queue_delay: Duration,
}

impl StreamReport {
    /// Summed leaf proving, which is what one machine would pay serially.
    pub fn leaf_total(&self) -> Duration {
        return self.leaves.iter().map(|l| l.prove_wall).sum();
    }

    /// What the same work costs when producing and proving cannot overlap: every response
    /// produced first, then the whole lift. The comparison the split exists for.
    ///
    /// ⚠️ Only meaningful when producing is slow enough to be worth overlapping. This
    /// machine produces a hardened-child response in milliseconds, so the counterfactual is
    /// barely above the streamed run and the ratio says nothing; drive it with
    /// `arrival_interval` to model a device before reading anything into it.
    pub fn serial_counterfactual(&self) -> Duration {
        return self.last_produced + self.leaf_total() + self.root_wall;
    }
}

/// Everything the leaf and root programs need that does not change per response.
pub struct Statement {
    /// Parent chain code (public).
    pub chain_code: Vec<u8>,
    /// Child index (public).
    pub index: u32,
    /// The expected circuit output.
    pub expected_output: Words,
    /// The challenge binding.
    pub binding: Vec<u8>,
}

/// Produces responses one at a time, exactly as a streaming prover does.
///
/// The challenge phase runs in full first and is timed separately: it is global over every
/// iteration, so it is a floor on when the first response can exist, and on a device it is
/// the byte-silent stretch that no amount of proving capacity shortens.
fn produce(
    circuit: &HardenedChildCircuit,
    num_responses: usize,
    seed_entropy: &[u8],
    binding: &[u8],
    arrival_interval: Option<Duration>,
    tx: SyncSender<Arrival>,
) -> Duration {
    let t = Instant::now();
    let challenge_entropy = build_challenge_entropy::<_, H, PS, PV, S, _, WTP, _>(
        circuit,
        seed_entropy,
        binding,
        num_responses,
        ChallengeOptions::new(),
    );
    let challenge_wall = t.elapsed();
    let mut builder = ProofBuilder::<H, PS, PV, S, OwnedResponseDataCollector<S, S>, WTP>::new(
        seed_entropy,
        challenge_entropy,
        num_responses,
        ProofOptions::new(),
    );
    // The producer's own clock, which advances only while it is computing or waiting out a
    // modelled arrival interval — never while it is blocked on a full channel.
    let mut produced_at = challenge_wall;
    for position in 0..num_responses {
        let t = Instant::now();
        let iter = builder.next_iter().expect("iteration must be available");
        <HardenedChildCircuit as zkboo::circuit::Circuit>::exec(circuit, iter.view_builder());
        let response = iter.finalize();
        let mut cost = t.elapsed();
        // A device takes about thirty seconds per response where this machine takes
        // milliseconds, so without this the pipeline is starved and measures nothing about a
        // deployment. Sleeping the remainder models the arrival cadence rather than the work.
        if let Some(interval) = arrival_interval {
            if let Some(remaining) = interval.checked_sub(cost) {
                thread::sleep(remaining);
                cost = interval;
            }
        }
        produced_at += cost;
        if tx.send(Arrival { position, response, produced_at }).is_err() {
            break;
        }
    }
    builder.finalize();
    return challenge_wall;
}

/// Streams a lift: leaves are proved as their responses arrive, and the root fires when the
/// last leaf lands.
///
/// `pool` is the number of concurrent leaf provers. ⚠️ On this machine one SP1 prover peaks
/// near 59 GB, so two is the most that fits in 137 GB and three does not; the pool is a
/// stand-in for machines rather than for cores, and a deployment would put each on its own.
pub fn stream_lift(
    client: &EnvProver,
    circuit: &HardenedChildCircuit,
    statement: &Statement,
    num_responses: usize,
    seed_entropy: &[u8],
    pool: usize,
    arrival_interval: Option<Duration>,
    leaf_pk: &sp1_sdk::blocking::EnvProvingKey,
    leaf_vk: &SP1VerifyingKey,
) -> (StreamReport, Vec<SP1ProofWithPublicValues>) {
    assert!(num_responses >= 1, "need at least one response");
    assert!(pool >= 1, "need at least one prover");
    let started = Instant::now();
    let (tx, rx) = sync_channel::<Arrival>(pool);
    let records: Arc<Mutex<Vec<LeafRecord>>> = Arc::new(Mutex::new(Vec::with_capacity(num_responses)));
    let proofs: Arc<Mutex<Vec<(usize, SP1ProofWithPublicValues)>>> =
        Arc::new(Mutex::new(Vec::with_capacity(num_responses)));
    let pool_idle = Arc::new(Mutex::new(Duration::ZERO));
    let queue_delay = Arc::new(Mutex::new(Duration::ZERO));
    let last_produced = Arc::new(Mutex::new(Duration::ZERO));

    let challenge_wall = thread::scope(|scope| {
        // The receiver is owned INSIDE the scope, and the last handle outside the worker set is
        // dropped below once the workers hold their clones.
        //
        // 🪤 It used to live outside the scope, and that deadlocked the run whenever every
        // worker died. A worker panics on a failed prove or a failed self-verification; the
        // producer blocks in `tx.send` as soon as the bounded channel fills, which is the steady
        // state here because proving takes a minute and producing takes milliseconds. `send`
        // unblocks only on space or on the receiver being dropped — and an `Arc` held in the
        // caller's frame kept it alive past the death of every worker. The producer then blocked
        // for ever, its `join` never returned, and the worker's panic was never propagated: the
        // process sat silent at zero CPU. With GPU proving documented to fail its own
        // verification about once in twenty, that turns an overnight run's recoverable failure
        // into an indefinite hang with no message.
        let rx = Arc::new(Mutex::new(rx));
        let producer = scope.spawn(|| {
            produce(
                circuit,
                num_responses,
                seed_entropy,
                &statement.binding,
                arrival_interval,
                tx,
            )
        });
        for _ in 0..pool {
            let rx = Arc::clone(&rx);
            let records = Arc::clone(&records);
            let proofs = Arc::clone(&proofs);
            let pool_idle = Arc::clone(&pool_idle);
            let queue_delay = Arc::clone(&queue_delay);
            let last_produced = Arc::clone(&last_produced);
            scope.spawn(move || {
                loop {
                    // Time spent here is a prover with nothing to do, which is the number
                    // that says a deployment has more machines than it needs.
                    //
                    // ⚠️ This counts waiting for a response *that will come*. The final `recv`,
                    // the one returning Err because the channel closed, breaks below without
                    // being counted — so a worker sitting out the last leaf's prove on an
                    // otherwise finished run contributes nothing here. That is deliberate under
                    // the stated definition, but it means `pool_idle` is an undercount against
                    // the fully-counted `queue_delay` whenever pool > 1, and the printed
                    // comparison between them is biased toward "proving is the bottleneck" by
                    // exactly that much.
                    let idle_from = Instant::now();
                    let arrival = { rx.lock().expect("receiver lock").recv() };
                    let Ok(arrival) = arrival else { break };
                    let idle = idle_from.elapsed();
                    let picked_up = started.elapsed();
                    {
                        let mut g = pool_idle.lock().expect("idle lock");
                        *g += idle;
                        let mut g = queue_delay.lock().expect("queue lock");
                        *g += picked_up.saturating_sub(arrival.produced_at);
                        let mut g = last_produced.lock().expect("produced lock");
                        if arrival.produced_at > *g {
                            *g = arrival.produced_at;
                        }
                    }
                    let leaf_input = LeafInput {
                        version: pofseed_sp1_lift_split::LEAF_FRAME_VERSION,
                        chain_code: statement.chain_code.clone(),
                        index: statement.index,
                        expected_output: statement.expected_output.clone(),
                        binding: statement.binding.clone(),
                        position: arrival.position as u32,
                        response: arrival.response,
                    };
                    let mut stdin = SP1Stdin::new();
                    stdin.write_vec(
                        postcard::to_allocvec(&leaf_input).expect("serialize leaf frame"),
                    );
                    // Fault injection, for demonstrating that a dead worker pool ends the run
                    // instead of hanging it. Set STREAM_LIFT_FAIL_LEAF=<position> to panic a
                    // worker before it proves. Without a way to kill a worker on demand, the
                    // deadlock fix above is a claim rather than a test — and this repository has
                    // learned twice that a refusal path nobody can reach is not evidence.
                    if let Ok(fail_at) = std::env::var("STREAM_LIFT_FAIL_LEAF") {
                        if fail_at.parse::<usize>() == Ok(arrival.position) {
                            panic!("leaf {}: injected fault (STREAM_LIFT_FAIL_LEAF)", arrival.position);
                        }
                    }
                    let t = Instant::now();
                    let proof = client
                        .prove(leaf_pk, stdin)
                        .compressed()
                        .run()
                        .unwrap_or_else(|e| panic!("leaf {} proving: {e:#}", arrival.position));
                    let prove_wall = t.elapsed();
                    client
                        .verify(&proof, leaf_vk, None)
                        .unwrap_or_else(|e| panic!("leaf {} verification: {e:#}", arrival.position));
                    records.lock().expect("records lock").push(LeafRecord {
                        position: arrival.position,
                        produced_at: arrival.produced_at,
                        started_at: picked_up,
                        prove_wall,
                    });
                    proofs.lock().expect("proofs lock").push((arrival.position, proof));
                }
            });
        }
        // Now only the workers hold the receiver: when the last one exits or dies, the channel
        // closes, `send` fails, and the producer returns instead of blocking for ever.
        drop(rx);
        producer.join().expect("producer thread panicked")
    });

    let mut records = Arc::try_unwrap(records).expect("sole owner").into_inner().expect("records");
    records.sort_by_key(|r| r.position);
    assert_eq!(records.len(), num_responses, "every response must have been proved");
    let mut proofs = Arc::try_unwrap(proofs).expect("sole owner").into_inner().expect("proofs");
    // The root's two streams must be in response order, and nothing about a worker pool
    // preserves order — the sort is what does, and the cross-leaf gate is what catches it
    // if this is ever wrong.
    proofs.sort_by_key(|(position, _)| *position);
    for (i, (position, _)) in proofs.iter().enumerate() {
        assert_eq!(*position, i, "leaf proofs are not a complete run from 0");
    }
    let ordered: Vec<SP1ProofWithPublicValues> =
        proofs.into_iter().map(|(_, proof)| proof).collect();

    let report = StreamReport {
        num_responses,
        pool,
        challenge_wall,
        last_produced: *last_produced.lock().expect("produced lock"),
        leaves: records,
        root_wall: Duration::ZERO,
        end_to_end: started.elapsed(),
        pool_idle: *pool_idle.lock().expect("idle lock"),
        queue_delay: *queue_delay.lock().expect("queue lock"),
    };
    return (report, ordered);
}

/// The circuit and its executed output, for a statement the orchestrator can drive.
pub fn hardened_child_statement(
    chain_code: Vec<u8>,
    parent_priv: Vec<u8>,
    index: u32,
    binding: &[u8],
) -> (HardenedChildCircuit, Statement) {
    let circuit = HardenedChildCircuit::with_witness(chain_code.clone(), parent_priv, index);
    let expected_output = exec::<_, WP, _>(&circuit, ExecOptions::new());
    return (
        circuit,
        Statement { chain_code, index, expected_output, binding: binding.to_vec() },
    );
}

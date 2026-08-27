# Splitting the lift, and streaming into it

The production lifter verifies every response of a ZKBoo proof inside one zkVM program, so
its cost is one indivisible run: 6 h 28 m for the hardened-child statement at 219
responses, and an extrapolated ~130 days for the Ethereum-address statement. Two questions
follow from that, and they turn out to have one answer.

- **Can the responses be proved in parallel, and aggregated at the end?** That is the only
  route that scales to the address statement, since no single machine gets it under months.
- **Can the lift start before all 219 responses exist?** The device streams them at **3,234–3,500 B/s**
  (measured 2026-08-26 across 39 verified rows; the ~1.9 KB/s once used here was retracted — its timer
  spanned the consent prompt, so it partly measured the operator),
  so today the whole stream lands before any proving begins.

Both are the same architecture: a **leaf** proof per response and a **root** proof that
re-derives the Fiat-Shamir challenge across all of them. Parallelisation spreads the leaves
over machines; streaming spreads them over time. This document records what that
architecture is, that it is sound, and what it costs — measured, not assumed.

## Why the split is possible

`zkboo::par_verify` already has the shape. Each response is replayed independently to yield
its three view commitments; only afterwards are the commitments absorbed *in order* into the
challenge hasher, the challenge sequence re-derived from the resulting entropy, and each
response's declared challenge party checked against it. The per-response replay never
consults the global challenge — it consults only the party the response itself declares.

So the replay distributes and the challenge check does not. A leaf can be proved with no
knowledge of any other response.

## Why the split is dangerous

The Fiat-Shamir entropy is a hash over the commitments of exactly the responses present. A
subset of leaves re-derives a different challenge sequence and therefore attests nothing
about the whole — the same property that makes a truncated proof fail, and that makes
zkboo's ranged proving safe. Split naively and you have 219 proofs of nothing.

Everything that recovers the statement lives in one function, `snark_lifter_split::aggregate`:

- the leaves are a **complete, in-order, duplicate-free** cover of the proof;
- every leaf attests **the same statement**;
- the challenge sequence **re-derived from all the commitments present** is exactly the
  sequence of parties the responses declared.

The last of these is the global Fiat-Shamir gate — the same correctness condition the STARK
track identified for any lifter, and the whole soundness content of the split.

## The probe

`split/`, `program-leaf/`, `program-root/` and the `split-probe` driver. Deliberately
separate from the production statement crate: nothing here links into `program/`, so probing
cannot move the pinned verifying key.

- **Leaf** — replays ONE response and commits `(position, statement digest, declared party,
  the three view commitments)`. It attests nothing on its own: one response has soundness
  2/3, and no leaf can know whether its party is the one Fiat-Shamir demands.
- **Root** — admits each leaf proof through SP1's proof-composition syscall
  (`verify_sp1_proof`, addressed by the leaf verifying-key digest and the SHA-256 of the
  leaf's public values), runs `aggregate`, and commits the **same `LiftJournal` the
  monolithic lifter commits** — so `check_journal`, the relying-party gate and the on-chain
  fixture guards apply to it unchanged.

The circuit fingerprint is computed **once, in the root**, never per leaf. Computing it in
the leaf would pay a whole-circuit structural traversal 219 times; on the secp256k1
statement that pass is worth about two responses of work (the measured fixed term of
2.007e10 instructions, 66% of an n=1 run), so per-leaf fingerprinting would cost more than
the entire monolithic run. The circuit is compiled into the leaf program and pinned by its
verifying key, so every leaf is on the same circuit by construction.

### The cross-leaf refusal matrix

Nine cases, on real leaf journals, no proving needed (`split-probe <n> gate`):

| case | refused by |
|---|---|
| control — all leaves, in order | *accepted* |
| G1 dropped response | challenge mismatch |
| G2 substituted response (leaf 1 given leaf 0's commitments) | challenge mismatch |
| G3b mislabelled position | position check |
| G3 duplicated response | position check |
| G4 reordered responses, positions renumbered | challenge mismatch |
| G5 foreign statement in one leaf | statement mismatch |
| G6 tampered commitment (one bit of one digest) | challenge mismatch |
| G7 empty leaf set | no-leaves check |
| G8 root attesting a statement no leaf was proved against | statement mismatch |

That the control *accepts* is the load-bearing positive: it means `aggregate` reproduces
zkboo's challenge derivation byte for byte — the framed binding, the absorption order, the
two-bits-at-a-time party generation — against a genuine proof.

**Every check was validated by removing it and re-running the matrix.** Three went red as
required. The fourth did not, and the result is recorded rather than glossed: with the
position check deleted, **G3 (a duplicated response) is still refused**, by the challenge
check, because duplicating a response changes the transcript. The position field never
enters the challenge hash, so the only thing it is load-bearing for is a leaf that lies
about which response it is — which G3b now isolates, and which does go red on removal. The
position check is well-formedness, not soundness, and is documented as such.

### What the probe does NOT establish

- **The leaf-key binding rests on SP1's recursion layer, and the probe now tests it.** How
  it works, read out of the 6.4.0 sources rather than assumed: each `verify_sp1_proof` call
  folds `(vk_digest, pv_digest)` into a running Poseidon2 chain
  (`sp1-zkvm/src/syscalls/verify.rs`), the chain is committed into the program's own public
  values at halt (`COMMIT_DEFERRED_PROOFS` in `halt.rs`), and the recursion circuit asserts
  that the digest it reconstructs from the child proofs actually supplied equals the
  committed one, starting from zero (`sp1-recursion-circuit/src/machine/complete.rs`). The
  chain is order-dependent and length-dependent, so a root cannot claim 219 leaves and
  supply four, nor reorder them, nor substitute one. The `forge` mode exercises the
  key-mismatch direction empirically: it re-runs the root over the cached leaves under a
  corrupted `leaf_vk` and requires proving to FAIL.
- **`leaf_vk` is prover-chosen.** The root takes it as input and commits it, so a relying
  party can pin it — but a production root would compile it in as a constant, moving the pin
  inside the root's own verifying key. As it stands the root's journal must be checked for
  `leaf_vk` as well as everything `check_journal` covers.
- **No Groth16 wrap.** All proofs here are compressed. The wrap is ~171 s and constant, so
  it changes the totals below by a fixed amount and the on-chain gas not at all.

## Does the current lifter need all 219 responses up front?

Yes — but at three different levels, with three different answers, and they must not be
conflated.

### 1. The ZKBoo protocol: no

The verifier is already streamable, and `zkboo-harness`'s `pipe()` demonstrates it: prover
and verifier run in two threads over a bounded channel, and the verifier's memory is
constant regardless of proof size. Its structure is the one this whole document rests on —
replay a response as it arrives, absorb its three commitments into the challenge hasher,
retain two bits (the declared party), discard everything else, and compare the re-derived
challenge sequence at the end. Nothing in the mathematics wants the proof in one piece.

### 2. The lift guest: yes, avoidably

`program/src/main.rs` does a single `sp1_zkvm::io::read_vec()` and postcard-decodes the
whole `LiftInput`, so all 219 responses are resident at once — a ~13 MB frame for the
hardened child, and 2.47 GB for the address statement. Changing the frame to a header plus
N response records and reading them one at a time makes guest memory O(1) in the response
count, with no protocol change and a frame-version bump.

**This buys memory, not latency.** SP1 still needs the complete stdin before proving starts.

### 3. The SP1 host boundary: yes, and not from outside the SDK

`SP1Stdin` is a fully materialised `Vec<Vec<u8>>` passed to `prove` by value, and
`execute_with_options_and_machine` feeds every buffer to the executor before it runs. There
is no incremental input.

SP1 does have the mechanism that would fix this — `SP1ContextBuilder::hook` registers a host
callback the guest can invoke mid-execution, which is exactly a "give me the next response"
channel. But it is reachable on the **execute** path only: the SDK's `ProveRequest` exposes
`cycle_limit`, `deferred_proof_verification`, `expected_exit_code` and `proof_nonce`, and
the `context_builder` behind them is `pub(crate)`. Streaming input into a *proving* run
therefore requires patching SP1, not configuring it.

Note also what SP1 already does internally: the executor emits trace chunks that shard
provers consume as they are produced, so execution and proving are pipelined *within* a run.
The parallelism is not missing — the input boundary is.

## The three options for streaming, in increasing order of what they buy

**A. Stream inside the guest.** Header-plus-records frame, one `read_vec` per response,
driving the streaming verifier. Guest memory becomes constant in the response count.
Roughly a day's work and a frame-version bump. Does not let proving start any earlier.

**B. The leaf/root split.** Prove leaf *i* the moment response *i* arrives. This is the only
option that actually overlaps device time with proving, and it is the same architecture that
distributes across machines — so streaming and parallelisation are bought together, not
traded off. The end-to-end latency becomes

    time for the last response to arrive  +  one leaf  +  the root

instead of

    time for the whole proof to arrive  +  the entire lift

The device streams at **~3,309 B/s** (measured mean; ~1.9 KB/s retracted) and a 219-response
hardened-child proof is **11.32 MB** (51,691 B/response measured, was ~12.7 MB), so a
response lands roughly every 30 s over about 1.9 h. Today those 1.9 h are strictly serial
with the 6 h 28 m lift. Under B they overlap almost entirely, and the leaves keep pace with
the device as long as enough machines are running to absorb one leaf every 30 s.

**C. Patch SP1 to expose hooks on the proving path.** True input streaming into a monolithic
run. It overlaps the device time with proving but does nothing about proving time itself, so
on the hardened child it saves at most ~1.9 h out of ~8.4 h. Comparable effort to B, a
fraction of the benefit, and it carries a fork of the prover stack. **Not recommended.**

## 🚨 Every timing below is PROVISIONAL — the machine was not quiet

Stefano flagged (2026-08-24) that other work was running on this machine while these runs
were taken, so **none of the wall times in this section may be cited**. They are kept as the
record of what was run and what was seen, not as measurements. The campaign that was to firm
them up — `root(64)`, per-phase peak RSS, and the recursion-layer negatives — was stopped
partway and will be relaunched on a quiet machine.

What is *not* affected, because it does not depend on machine load: the cross-leaf gate
matrix and its check-removal validation, the wire sizes, the leaf-journal host/guest
equality check, and the reading of SP1's deferred-proof accounting.

## Measured (2026-08-24, this machine, 12-thread cap, compressed proofs) — PROVISIONAL

| | n=4 | n=8 |
|---|---:|---:|
| monolithic, one program | 606.08 s | *(not run)* |
| leaf, each | ~123 s | ~123 s |
| leaves, summed | 491.81 s | 985.31 s |
| **root** | **226.53 s** | **248.25 s** |
| split, one machine | 718.34 s | 1 233.56 s |
| split, critical path (slowest leaf + root) | 350.73 s | 373.55 s |

Two results, both against expectation.

**The leaf is far cheaper than the fitted lift law predicts.** That law — 103.4 s per response
plus ~315 s fixed — would put a one-response run at ~418 s; leaves measure ~123 s, flat across
both runs. The fixed term belongs to the *lift program*, not to SP1, and a large part of it is
the in-guest circuit-fingerprint pass the leaf deliberately does not perform. At n=4 the four
leaves summed (491.81 s) are already cheaper than the monolithic run they replace (606.08 s).

**The root is near-fixed, not linear.** From the two points:

    root(N) ≈ 204.8 s + 5.43 s × N

The marginal cost of admitting one more leaf proof is **5.43 s — about 4.4% of the leaf itself**.
Extrapolated to the classical-128 count:

| | CPU, this machine | Metal at 5× |
|---|---:|---:|
| root(219) | 1 394 s (23.2 min) | ~4.6 min |
| critical path = one leaf + root | 1 517 s (25.3 min) | ~5.1 min |
| against the monolithic 23 320 s | **15.4×** | — |
| leaves summed at 219 | 26 937 s (7.48 h) | — |

So the split costs **1.16× more total work** than the monolithic lift and returns **15.4× less
wall time** given enough machines. On one machine it is a modest loss; distributed or streamed it
is a large win, and the root is small enough that it does not have to be made incremental.

⚠️ **Provisional, and doubly so.** The slope is measured over 4→8 and applied at 219, a 27×
extrapolation — *and* both points were taken on a machine that was not quiet. This repository has
already been burned once by exactly this move (the `TRACE_CHUNK_SLOTS` gain that was real at n=8
and absent at n=219), and SP1's deferred-proof handling may well batch differently at a few
hundred children than at eight. **Validate `root(N)` at n≈64 before designing to 23 minutes.** The
qualitative finding — that the marginal cost per leaf is a small fraction of a leaf — is robust to
a good deal of slope error; the 23-minute figure is not.

## What this makes possible, and what it costs to build

Because each leaf is an independent `prove` call whose stdin is complete the moment its response
lands, **streaming needs no change to SP1 at all**. The "stdin must be fully materialised"
constraint is per call, and a call carrying one response satisfies it trivially. Option C above
(patching the prove path to expose hooks) is therefore not merely lower-value — it is unnecessary.

End-to-end latency changes character. Monolithic makes device streaming and proving a **sum**,
because the lift cannot start until the last response has landed; the split makes them a **max**:

| | streaming | proving | end to end |
|---|---:|---:|---:|
| monolithic, CPU | 1.64 h | 6.47 h | 8.11 h |
| monolithic, Metal 5× | 1.64 h | 1.30 h | 2.94 h |
| split, Metal 5×, one card | 1.64 h | overlapped | **~1.73 h** |

At **51,297–51,691 B** per response, a response lands every **~29–30 s** — that is measured device
work end to end, not a division: streaming it takes ~16 s at the measured ~3,309 B/s, and the
byte-silent challenge phase adds ~13 s more before the next one appears. (The ~27 s once quoted here
came from dividing by a retracted rate and was close to the right answer for the wrong reason.)
Against a
~24.6 s Metal leaf: one card keeps pace in real time, with **~17–18% headroom** (29–30 s of arrival
against 24.6 s of leaf), better than the ~10% computed here previously. ✅ The device rate this rests
on is **no longer stale** — it is measured across 39 verified rows and is flat in statement, witness
size, response count and proof size, so it is a sound thing to design against.

⚠️ The Metal leaf time itself is unmeasured and remains the weak input to this comparison.

Remaining work, in order:

1. **A streaming orchestrator** — per arriving response, build a `LeafInput` and dispatch a
   `prove` into a pool; fire the root when the last leaf lands. No SP1, zkboo, or contract change.
2. **Per-response delivery from the device** — responses delimited and handed over as they
   complete rather than as one blob. Plumbing; the harness's framed pipe is the model.
3. **Pin the leaf verifying key as a compile-time constant in the root**, retiring the
   prover-chosen `leaf_vk` input.
4. **Validate `root(N)` at n≈64** before quoting the 23-minute figure anywhere.

Not needed, on the measurements above: the chained/tree root (the marginal cost is too small to
be worth overlapping), the guest-side streaming frame (a leaf holds one response by construction),
and any patch to SP1.

## The forged-leaf negative, and why its first form proved nothing

The root's binding to its leaves is not enforced by any code in this repository — it is
SP1's. The mechanism, read out of the 6.4.0 sources: each `verify_sp1_proof` folds
`(vk_digest, pv_digest)` into a running Poseidon2 chain in the guest, the chain is committed
into the program's public values at halt, and the recursion circuit asserts that the digest
it reconstructs from the child proofs *actually supplied* equals the committed one, starting
from zero. Order- and length-dependent, so the claims and the proofs cannot diverge.

The first attempt to test this corrupted the leaf verifying-key digest the root claims. The
root did fail — and the test, which asserted only that proving failed, called it a pass. The
error was **`artifact not found`**: SP1 looks up a recursion artifact by verifying-key digest
before it checks anything cryptographic, and a corrupted digest names no artifact. **The run
never reached the binding it was supposed to test.** A refusal that stops short of the
mechanism under test is not evidence about that mechanism, and the test as written could not
tell the difference.

Three negatives now replace it, and each asserts it did not stop at artifact lookup:

- `forge-vk` — corrupted leaf verifying key (kept, but expected to fail at artifact lookup,
  which the assertion now rejects rather than counts);
- `forge-count` — the root claims `n` leaves and supplies `n-1` proofs, with a genuine key;
- `forge-order` — the proofs are supplied in an order the claims do not match.

The last two keep a real verifying key precisely so that artifact lookup succeeds and the
deferred-proof digest check is what refuses them. **None has been run yet**; they belong to
the quiet-machine relaunch.

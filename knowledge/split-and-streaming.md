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

The leaf-key binding. It has its own section at the end of this document, because what happened when
it was finally tested is the most useful thing in here.

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

## Measured (2026-08-28, quiet machine, zkboo 2.0.0, compressed proofs)

The earlier numbers here were taken while other work ran on this machine and were discarded on that
ground; nothing from them is quoted below. These replace them. Every run was taken alone, at the
12-thread cap, under `/usr/bin/time -l`, on the released zkboo 2.0.0 and the current program
(fingerprint `d4825905…`).

Three arms, because the GPU has two settings and the difference between them matters — see
`gpu-corruption.md`. The CPU column is the same binary with every GPU switch forced off, which is
the only comparison in which nothing but the path under test differs; it measures within 0.8% of the
published SP1 prover, which was checked separately rather than assumed. "GPU, safe" is the Metal backend with its Merkle tree left on the CPU, which is
the only setting that survives a long run; "GPU, full" is every phase accelerated, which is 2.5×
faster again and corrupts a proof about once every twenty.

| | CPU | GPU, safe | GPU, full |
|---|---:|---:|---:|
| leaf (one response) | 109.72 s | **57.00 s** | 23.26 s |
| root over 4 leaves | 205.61 s | 112.18 s | 47.87 s |
| root over 64 leaves | 527.20 s | **264.95 s** | *(not run)* |

The leaf is extraordinarily stable: over 64 consecutive proofs, 56.27 s to 57.93 s, a spread of under
2%, mean 57.00 s.

### The root is near-fixed, and now it is measured where it matters

    CPU:       root(N) ≈ 184.2 s + 5.36 s × N
    GPU, safe: root(N) ≈ 102.0 s + 2.55 s × N

**The marginal cost of admitting one more leaf proof is 2.55 s against a leaf of 57.00 s — 4.5%.**

The previous, discarded fit was `204.8 + 5.43 × N`, taken at n = 4 and n = 8 on a loaded machine.
Its slope was **right to 1.3%**. That is worth recording precisely because the caution attached to it
was the correct one anyway: it was a 27× extrapolation and this repository had already been burned
once by a small-n measurement that did not survive full scale, so it could not have been believed in
advance. The present fit spans 4 to 64 and is a 3.4× extrapolation to the classical-128 count.

| at N = 219 | CPU | GPU, safe |
|---|---:|---:|
| root | 1 358 s (22.6 min) | **660 s (11.0 min)** |
| critical path = one leaf + root | 1 468 s (24.5 min) | **717 s (12.0 min)** |
| leaves summed | 24 029 s (6.67 h) | 12 483 s (3.47 h) |

Against a monolithic lift, on this machine's CPU baseline of 23 320 s: the leaves alone sum to
**1.03×** that, and the root brings the split's total work to **1.09×**. What it returns for that 9%
is the wall clock above, given enough machines — a **15.9× reduction**.

### Peak memory

| | CPU | GPU, safe |
|---|---:|---:|
| leaf | 82.2 GB | 58.9 GB |
| root(4) | 86.2 GB | 68.8 GB |
| root(64) | 96.6 GB | 80.1 GB |

Peak memory grows slowly with the number of leaves admitted and is comfortable at both ends.

## What this makes possible, and what it costs to build

Because each leaf is an independent `prove` call whose stdin is complete the moment its response
lands, **streaming needs no change to SP1 at all**. The "stdin must be fully materialised"
constraint is per call, and a call carrying one response satisfies it trivially. Option C above
(patching the prove path to expose hooks) is therefore not merely lower-value — it is unnecessary.

End-to-end latency changes character. Monolithic makes device streaming and proving a **sum**,
because the lift cannot start until the last response has landed; the split makes them a **max**.

The balance condition is one number against one number: **a response arrives every ~29–30 s** of
measured device work (51,297–51,691 B streamed at 3,234–3,500 B/s, plus a byte-silent challenge
phase), and a leaf must be proved in less than that for one machine to keep pace in real time.

| leaf on | leaf time | keeps pace with one machine? |
|---|---:|---|
| CPU | 109.72 s | no — needs four |
| GPU, safe | 57.00 s | no — needs two |
| GPU, full | 23.26 s | yes, with 21–27% headroom — but see `gpu-corruption.md` |

**The configuration that keeps pace on one card is the one that corrupts a proof about once every
twenty**, so as things stand the honest statement is *two* machines, not one. Both halves of that
were estimates when this document was last written and both are now measured; the device rate is the
one that stopped being the weak input, and the leaf time is the one that turned out to have a
correctness caveat attached rather than a clock caveat.

Remaining work, in order:

1. ~~A streaming orchestrator — per arriving response, build a `LeafInput` and dispatch a
   `prove` into a pool; fire the root when the last leaf lands.~~ **Done, 2026-08-28**: `host/src/orchestrator.rs`
   and the `stream-lift` driver. No SP1, zkboo, or contract change, as expected.
2. **Per-response delivery from the device** — responses delimited and handed over as they
   complete rather than as one blob. Plumbing; the harness's framed pipe is the model.
3. ~~Pin the leaf verifying key as a compile-time constant in the root, retiring the
   prover-chosen `leaf_vk` input.~~ **Done, 2026-08-28** (`8e9cc0b`): both roots take their leaf key
   from `pinned-keys`, and a claimed key that disagrees is refused before the loop. The leaf keys did
   not move; both root keys did, which is the expected consequence and is why the pinned constant
   lives in a crate the leaf programs do not link. Root cost unchanged: 112.29 s pinned against
   112.18 s unpinned.
4. ~~Validate `root(N)` at n≈64 before quoting the 23-minute figure anywhere.~~ **Done, 2026-08-28**: the fit spans 4 to 64 on a quiet machine and the discarded slope survived.
5. **Exercise the in-circuit leaf binding, which the stock SDK will not let these tests reach.** It
   needs a root proved against a deliberately inconsistent (claims, proofs) pair, which the SDK's own
   bookkeeping refuses to assemble — so it needs the recursion layer driven directly, below the SDK.
   Until then that binding rests on a source reading; see the last section.

Not needed, on the measurements above: the chained/tree root (the marginal cost is too small to
be worth overlapping), the guest-side streaming frame (a leaf holds one response by construction),
and any patch to SP1.

## The forged-leaf negatives: run at last, and they do not reach what they were built for

The root's binding to its leaves is not enforced by any code in this repository — it is SP1's, and
SP1 enforces it **twice, in two different places, and only one of them matters.**

- **In the recursion layer, in circuit.** Each `verify_sp1_proof` folds `(vk_digest, pv_digest)` into
  a running Poseidon2 chain in the guest; the chain is committed into the program's public values at
  halt; and the recursion circuit asserts that the digest it reconstructs from the child proofs
  *actually supplied* equals the committed one, starting from zero. Order- and length-dependent, so
  the claims and the proofs cannot diverge. **This is the binding that matters**, because it is the
  one a prover cannot decline to run.
- **On the host, at execution time.** The `SubproofVerifier` matches what the guest asked for against
  what the caller supplied, and the deferred-proof artifact store is keyed by digest. SP1's own
  comment on that trait (`sp1-core-executor/src/subproof.rs`) calls it a sanity check "that the user
  passed in the correct proof", and adds that "the actual constraints happen in the recursion layer".

**All three negatives were run on 2026-08-28, all three refuse, and all three refuse at the host
layer** — `artifact not found`, at about 104 s, before a recursion circuit exists.

That includes the two that were written specifically to get past it. `forge-count` and `forge-order`
keep a genuine leaf verifying key precisely so that lookup would succeed and the in-circuit check
would be what refuses them, and it is not what refuses them. **The reason is structural rather than a
defect in the tests**: SP1 resolves a deferred proof by looking it up by digest, so any tampering
that puts claims and proofs out of correspondence makes some lookup fail before anything is built.

So: **the in-circuit binding is not reachable through the stock SDK, and it is not demonstrated
here.** The evidence for it is a reading of the 6.4.0 sources and nothing more, and a deployment must
not read these three passes as evidence about it. An adversarial prover would not run the host layer
at all, which is exactly why the in-circuit one exists.

What the three cases *do* establish is that a caller using this SDK cannot assemble a root whose
claims and proofs disagree — which is worth having, and is not what the tests were labelled as
establishing until now.

Each is validated by removing its tamper: with the withheld proof restored, `forge-count` proves and
the test fails with "the root PROVED with the leaf claims and the leaf proofs out of correspondence".

### Re-run under the pinned leaf key, and the pin does not move them

Pinning the leaf verifying key (item 3 above) was expected to change what `forge-vk` does. It does
not, and the expectation was wrong for a reason worth writing down.

All three were re-run on 2026-08-28 against the pinned roots. **All three still refuse at
`artifact not found`**, at 18.7 s, 105.8 s and 108.8 s, with the nine-case gate matrix green and the
control accepting in each. Log: `fixtures/forge-negatives-pinned.summary.log`.

The reasoning that expected otherwise was that `forge-vk` now has a new in-guest assertion to hit —
the root rejects a claimed leaf key that disagrees with the constant. But **the guest never runs.**
The host resolves each deferred proof by `(vk_digest, pv_digest)` before execution begins, and a
forged key digest matches no artifact, so the run dies a layer above any guest code. Pinning moved
what the guest would assert; it did not move where the refusal happens, because nothing reaches the
guest.

That makes the pin's assertion the same kind of object as the in-circuit binding itself: correct as
far as a source reading goes, and unexercised. It is defence in depth against a caller who drives the
recursion layer directly — which is the only caller that could ever reach it, and is precisely the
caller the stock SDK will not let these tests impersonate.

**This is the second time an expectation about these negatives has been wrong in the same direction**,
and both times the error was to reason about which check *would* catch a tamper without asking
whether control flow reaches that check at all.

### Why the first form of this test proved even less

The first attempt corrupted the leaf verifying-key digest and asserted only that proving failed. It
did fail — with `artifact not found`, the same host-layer refusal — and the test called it a pass.
**A refusal that stops short of the mechanism under test is not evidence about that mechanism**, and
the test as written could not tell the difference. The assertion added afterwards to catch that is
what caught all three of these, and is the reason this section says what it says instead of claiming
three green negatives.

### Still open

**~~`leaf_vk` is prover-chosen.~~ ✅ Pinned, 2026-08-28.** The roots compile the leaf verifying key in
(`pinned-keys/`), so a root's own verifying key now says which leaf program it will admit — there is
no longer a field outside `check_journal` that a relying party can forget. The frame still carries a
claimed key and the journal still commits one, so a journal stays self-describing, and the root
refuses any claim that disagrees with what it used.

It cost nothing measurable: 112.29 s for a root over four leaves against 112.18 s unpinned.

The constant lives in a crate of its own because the leaf programs must **not** link it — a leaf that
did would change its own key whenever that key was recorded, which does not converge. `vkeys`
recomputes both keys from the leaf ELFs, refuses if either has drifted, and prints the correct value;
it was demonstrated red by pinning zeros first.

**No Groth16 wrap in the split.** All proofs in the probe are compressed. The wrap is ~171 s and
constant, so it shifts the totals by a fixed amount and the on-chain gas not at all.

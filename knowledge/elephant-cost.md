# The cost of lifting the Ethereum-address statement

Measured 2026-08-22 on the 16-core Apple Silicon machine (137 GB), SP1 sdk 6.4.0, zkboo 1.0.0.
Everything below is measured on this machine unless it is labelled an extrapolation.

## Why this statement

The proof-of-seed suite divides in two, and the division is not a matter of degree.

The **hash-only** statements — master key, hardened child, BIP-39 seed — are HMAC-SHA512 chains.
They cost tens of kilobytes per response, and the lifter proves a full classical-128 lift of one in
6 h 28 m.

The **secp256k1** statements — public key `d·G`, normal child, and the Ethereum address — put an
in-circuit scalar multiplication in front of that, and cost megabytes per response. Only these bind
a proof of seed to an actual Ethereum account:

```
address = Keccak256(x_be ‖ y_be)[12:]   where   Q = (x, y) = d·G
```

This matters for the deployment story rather than the mechanism. An ERC-4337 account that accepts a
lifted proof as authorisation must know *which* legacy account the prover is proving for, and only
the address statement establishes that. A lift of the hardened-child statement demonstrates the
machinery end to end; it does not, on its own, migrate anybody's account.

## What was measured

The probe lives in `elephant/` (the circuit and its wire format) and `program-elephant/` (its zkVM
guest), deliberately outside the production statement crate: that crate's shape determines the guest
ELF and hence the verifying key pinned by the on-chain suite, and a probe must not move it. The
suite was re-run after building and still passes 5/5 at the same key.

### At the circuit

| | hardened child | Ethereum address | ratio |
|---|---:|---:|---:|
| response size | ~58 KiB | **11.28 MB** | 190× |
| full proof at 219 responses | 12.7 MB | **2.47 GB** | — |
| circuit execution | — | 269 ms | — |
| native prove, one response | ~5 ms | 1.16 s | — |
| native verify, one response | ~2.1 ms | **511 ms** | 243× |

> ⚠️ `zkboo-bip32/README.md` still lists **42.3 MB** per response for this statement. That table
> predates the optimisation round and is stale by 3.75×. Anything quoting it — including this
> project's own earlier estimate of ~9.3 GB of proof data at full scale — is wrong by the same
> factor. The correct figures are 11.28 MB and 2.47 GB.

### In the zkVM

| responses | instructions | executor wall | peak RSS |
|---:|---:|---:|---:|
| 1 | 30 495 877 378 | 185.2 s | 15.1 GB |
| 2 | 40 917 254 932 | 248.8 s | 15.7 GB |
| 4 | 61 760 022 190 | 378.0 s | 16.5 GB |

**`instructions = 2.007e10 + 1.042e10 × n`**

The marginal cost from n=1→2 and from n=2→4 agree to **0.0001%**, so the law is solid across the
probed range.

**The fixed term is 66% of a single-response run** — the in-guest circuit-fingerprint pass over a
secp256k1 circuit, plus decoding a 13.4 MB frame. A cost estimate extrapolated from n=1 alone
therefore **overstates full scale by 2.90×**. At 219 responses the same term is 0.87% of the total
and can be ignored. (This is the second time on this project that a single cheap measurement has
answered a different question than the one asked; see the prover-configuration sweep.)

## What a full lift would cost

At 219 responses — classical-128 in the forgery-work-ratio sense, with the usual caveat that
stricter definitions rate it lower, and with BN254's ~100-bit pairing security the real ceiling of
this track either way:

- **2.302e12 instructions**
- at this machine's measured proving rate of **204 687 instructions/s**: **≈ 130 days**
- **482×** the 6 h 28 m the hardened-child lift actually takes

> ⚠️ That rate is the *hardened-child* statement's, assumed to carry over. It has not been measured
> on an elephant workload, whose instruction mix is entirely different, and **no elephant response
> has been proved — only executed**. Treat 130 days as an order of magnitude, not a figure.

Scaled onto other hardware, each factor an extrapolation stacked on the one before it:

| | estimate |
|---|---:|
| this machine, measured rate | ~130 days |
| valkyrie, 64 EPYC cores | ~35 days |
| + one of its GPUs (sp1-metal's projected 10×) | ~13 days |
| + four or five cards busy on one proof | ~3 days |

None of these is a route to lifting this statement as a single proof.

## Why the cost is shaped this way

The elephant costs **481× per response against a 190× response size** — about 2.5× more per byte
than the hash statements. The work is replaying scalar-multiplication gates, and no precompile
relieves it: SP1's Keccak accelerator, which does most of the heavy lifting for the hash statements,
does nothing for field arithmetic.

The zkVM also needs roughly **6–12× the instructions the native verifier uses** for this statement,
where on the hash statement it sits at rough parity. 256-bit field arithmetic compiles far less
favourably to RISC-V than hashing does.

## The structural obstacle

219 responses look like 219 independent verification tasks that a cluster could divide between
machines. **They are not, and this repository has already demonstrated that they are not.**

The Fiat-Shamir challenge is derived from the view commitments of exactly the responses present in
the proof, and the regenerated challenge sequence is compared position by position. Remove any
response and every challenge changes, so the remainder verifies as nothing. Truncated prefixes,
arbitrary sub-ranges, a sparse selection of 9 responses out of 30 iterations, reorderings and
duplications were each built and each rejected. (That same property is what makes zkboo's ranged
proving safe, and what makes the response count a load-bearing part of the statement.)

So distributing the work cannot mean distributing the proof. Any division must preserve a **global**
challenge check across all 219 responses while splitting the replay between provers — a design in
its own right, and the same global-Fiat-Shamir gate the STARK track identified as the correctness
condition for any lifter.

## Conclusion, and what would change it

**The Ethereum-address statement is not liftable by this route at classical-128 on any hardware
currently available to the project.** The gap is about two orders of magnitude; it is not something
a better prover configuration closes, and the sweep that established that is recorded separately.

What the probe has bought is that this is measured rather than assumed, and that exactly two levers
are visible:

1. **Instructions per response** (1.042e10). This is zkboo's circuit, where a 3.75× reduction has
   already been found once. It is the larger lever and the one with precedent.
2. **The proving rate** (204 687 instr/s). SP1's CPU prover does its field arithmetic on scalar
   values while the packed path sits unused; packing measures ~1.9× end to end, costs no soundness,
   and is vkey-neutral — but 1.9× against a 482× gap is not the answer on its own.

A third option is not a lever but a change of target: a statement that binds an account without an
in-circuit scalar multiplication would sidestep the whole cost. Whether one exists is a question for
the protocol, not for the lifter.

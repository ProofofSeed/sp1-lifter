# verifier

Solidity and Foundry suite for the on-chain side of the SP1 lifter.

This is a research prototype and has not been audited.
`src/LiftJournalGate.sol` is first-party code with no audit.
`src/vendor/` holds the SP1 Groth16 verifier contracts, taken verbatim from `sp1-contracts` v6.1.0 (MIT, Succinct Labs; the only edit flattens one import path), and those are audited upstream.
`lib/forge-std` is vendored with its own MIT and Apache-2.0 licence files.
Nothing here has been deployed, and there is no ERC-4337 account contract.

## What a relying party must enforce

`SP1VerifierGroth16.verifyProof` establishes only that the program with the given verifying key ran and committed the given public values.
It authorises nothing on its own.
A relying contract must:

- pin the verifying key and pass it to `verifyProof` itself, because the key is not in the journal;
- pass the public values through `LiftJournalGate.check`, which decodes the journal and compares it with an `Expectation`;
- reject a journal whose assertion word is not 1, which `check` does unconditionally;
- require a minimum number of responses, because ZKBoo accepts any count and the soundness is (2/3)^k in the count k.

The assertion word is the last `u8` word of the circuit output, not the last byte of the journal.
A valid proof with assertion word 0 is obtainable by anyone for any address without the seed, so a gate that ignores the word authorises everybody.
The journal is postcard-encoded with varint fields and must be decoded, never indexed at fixed offsets.

## Security caveats

- The wrapped proof is capped at roughly 100 bits of security by the BN254 pairing; at 219 responses the ZKBoo part is 128-bit classical only in the forgery-work-ratio sense.
- The track is not post-quantum.

## Tests

```sh
forge test
```

The suite reads `fixtures/lift-219.onchain.json`, a verifying key, public values and proof from a full-soundness run, and skips without it.
A fully skipped suite still prints `Suite result: ok`, so check for `18 tests passed`.
The fixture is guarded by pinned verifying key, circuit fingerprint, proof-format identifier and response count; regenerate it with the `lift` binary whenever any of them moves.
Measured gas for a relying call is 225,975 for the pairing check and 54,615 for the journal gate (`../knowledge/performance-table.md`).

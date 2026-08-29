// SPDX-License-Identifier: LGPL-3.0-or-later
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {console2} from "forge-std/console2.sol";
import {SP1Verifier} from "../src/vendor/SP1VerifierGroth16.sol";
import {Verifier} from "../src/vendor/Groth16Verifier.sol";

/// On-chain verification of the lifted proof of seed, against the audited SP1 Groth16
/// verifier (vendored from sp1-contracts v6.1.0), on a real fixture exported by the
/// host (`lift`, fully local native-gnark wrap): the verifying-key hash, the public
/// values (the postcard-encoded lift journal), and the EVM proof bytes.
///
/// The positive case measures gas for the proof-verification call a relying contract
/// would make: `verifyProof(vkey, publicValues, proofBytes)`. The reject matrix covers
/// a corrupted proof, a wrong verifying key, and wrong public values (each refused by
/// the Groth16 pairing check, `ProofInvalid`), plus a wrong selector
/// (`WrongVerifierSelector`).
///
/// ⚠️ `verifyProof` alone is NOT an authorisation. It establishes only "this program,
/// identified by `vkey`, ran successfully and committed these public values". A relying
/// contract must additionally decode the public values (the postcard-encoded lift
/// journal) and enforce the reference consumer gate — fingerprint, statement, binding,
/// the soundness-critical minimum response count, and the trailing assertion word —
/// exactly as `check_journal` in the statement crate specifies. Two of those re-open a
/// total break on their own:
///
/// - omitting the **count** check accepts proofs at prover-chosen soundness, since the
///   ZKBoo verifier accepts even the empty proof;
/// - omitting the **assertion word** accepts a proof that asserts nothing. ZKBoo 2.0.0
///   leaves prover advice unconstrained except by the circuit's in-circuit assertions,
///   which are accumulated into the last word of the output. Run the honest prover on a
///   witness that does not satisfy the circuit and you get a perfectly valid proof whose
///   assertion word is 0 — obtainable by anyone, for any address, with no seed. A gate
///   that reads the twenty address bytes and ignores that word authorises everybody.
contract LiftGroth16Test is Test {
    /// `LIFT_JOURNAL_VERSION` from the statement crate. The journal is postcard-encoded
    /// and `version` is its first field, so for any value below 128 the first byte of
    /// the public values is the version varint.
    ///
    /// This is a FRESHNESS guard, not the journal gate: fixtures are generated
    /// artefacts that outlive the program they came from, and a fixture built by an
    /// older program stays internally consistent, so `verifyProof` accepts it and the
    /// suite would go green against a program that no longer exists. Bump this with
    /// `LIFT_JOURNAL_VERSION` and a stale fixture fails loudly instead.
    uint8 internal constant EXPECTED_JOURNAL_VERSION = 2;

    /// The structural fingerprint of the circuit this fixture must attest, sitting at
    /// `publicValues[2..34]` (the journal's third field, a bare 32-byte digest).
    ///
    /// The version byte above catches a change of journal *layout*; this catches a
    /// change of *circuit* that leaves the layout intact — which is the drift that
    /// actually happened once already, when adopting the ZKB++ view commitment moved the
    /// fingerprint from `33e1f70b…` to `e23ddb6a…` without touching the journal shape,
    /// and again when zkboo 2.0.0 gave every circuit a trailing assertion word and moved it
    /// to `d4825905…`.
    /// Update it deliberately when the circuit changes, which is the point.
    bytes32 internal constant EXPECTED_FINGERPRINT =
        0xd48259056d4b45ae38cb57178a6487fa7ffafbf0360e09248ab9c317efeb0a2b;

    /// The verifying key of the program the fixture must come from.
    ///
    /// Without this the suite attests nothing about which program ran: `vkey` is read
    /// from the fixture and handed straight to `verifyProof`, so a fixture is
    /// self-consistent by construction and verifies whichever program produced it. The
    /// version byte catches a journal-layout change and the fingerprint catches a
    /// circuit change; only this catches a program change that alters neither.
    ///
    /// It is deliberately a pin: when the program legitimately changes, this fails, and
    /// updating it is the act of accepting the new program.
    /// `REQUIRED_PROOF_FORMAT_VERSION` from the statement crate. It is the journal's
    /// last field, so it is the last byte of the public values.
    uint8 internal constant EXPECTED_PROOF_FORMAT_VERSION = 3;

    /// `num_responses` for the classical-128 target, as postcard writes it: 219 is above
    /// 127, so its LEB128 varint is two bytes, `0xDB 0x01`. It is the second-to-last
    /// field, hence at `[length-3]` and `[length-2]`.
    ///
    /// Only enforced for the default fixture, which is the acceptance artefact. An
    /// explicitly named fixture is a smoke at some smaller count and may attest any
    /// number — but the suite's own header calls the count soundness-critical, and
    /// without this a smoke fixture copied to the acceptance path would pass at the
    /// same gas while attesting a single response.
    uint8 internal constant N_EPS_VARINT_LOW = 0xDB;
    uint8 internal constant N_EPS_VARINT_HIGH = 0x01;

    bytes32 internal constant EXPECTED_VKEY =
        0x004290a12d1ecd14ebd8d11a441cc25a80c311e84246ff5df1c023d5885c1b88;

    SP1Verifier internal verifier;
    bytes32 internal vkey;
    bytes internal publicValues;
    bytes internal proofBytes;

    function setUp() public {
        // LIFT_FIXTURE overrides the fixture path (e.g. a small-n smoke fixture); the
        // default is the full classical-128 acceptance fixture. A missing fixture only
        // SKIPS when the default path is used implicitly (pre-acceptance compile runs);
        // an explicitly requested fixture that is absent is a hard failure, so an
        // acceptance invocation can never pass vacuously.
        string memory fixture =
            vm.envOr("LIFT_FIXTURE", string("fixtures/lift-219.onchain.json"));
        bool explicitFixture = bytes(vm.envOr("LIFT_FIXTURE", string(""))).length != 0;
        verifier = new SP1Verifier();
        if (!vm.isFile(fixture)) {
            require(!explicitFixture, "LIFT_FIXTURE set but the file does not exist");
            // forge reports a fully skipped suite as `Suite result: ok`, which reads as
            // a pass, so the reason has to reach the default output. It must be the
            // string form of `skip`: forge discards `console2` logs from a `setUp` that
            // reverts with the skip sentinel, so logging here prints nothing at any
            // verbosity.
            vm.skip(true, "no fixture: this suite verified NOTHING; generate with lift 219 groth16");
        }
        string memory json = vm.readFile(fixture);
        vkey = vm.parseJsonBytes32(json, ".vkey");
        publicValues = vm.parseJsonBytes(json, ".publicValues");
        proofBytes = vm.parseJsonBytes(json, ".proof");
        require(publicValues.length >= 34, "fixture public values are truncated");
        // Postcard encodes a u32 as a LEB128 varint, so the version occupies one byte
        // only while it stays below 128. At 128 exactly the byte would be 0x80 and the
        // next byte a continuation, and every offset below would silently shift.
        require(uint8(publicValues[0]) < 0x80, "journal version is multi-byte: offsets below are invalid");
        require(
            uint8(publicValues[0]) == EXPECTED_JOURNAL_VERSION,
            "fixture journal version does not match the statement crate: regenerate it"
        );
        // `valid` is the journal's second field; the program only ever commits `true`,
        // so anything else means the fixture was not produced by an accepting run.
        require(uint8(publicValues[1]) == 1, "fixture journal does not attest a valid proof");
        bytes32 fingerprint;
        bytes memory pv = publicValues;
        assembly { fingerprint := mload(add(pv, 34)) }
        require(fingerprint == EXPECTED_FINGERPRINT, "fixture attests a different circuit: regenerate it");
        require(vkey == EXPECTED_VKEY, "fixture comes from a different program: regenerate it");
        uint256 pvLen = publicValues.length;
        require(
            uint8(publicValues[pvLen - 1]) == EXPECTED_PROOF_FORMAT_VERSION,
            "fixture attests a different ZKBoo proof format"
        );
        if (!explicitFixture) {
            require(
                uint8(publicValues[pvLen - 3]) == N_EPS_VARINT_LOW
                    && uint8(publicValues[pvLen - 2]) == N_EPS_VARINT_HIGH,
                "acceptance fixture does not attest 219 responses"
            );
        }
        // Without this a valid proof that asserts nothing passes every other clause.
        require(_assertionWord(publicValues) == 1, "journal's assertion word is not set: the proof attests nothing");
    }


    /// Reads a postcard LEB128 varint at `off`, returning its value and the next offset.
    ///
    /// The journal's fields are varint-encoded, so a relying contract cannot reach the
    /// assertion word by a fixed index: the index field alone is one byte for an
    /// unhardened child and five for a hardened one. Walking the prefix is the only
    /// correct way in, which is why this exists rather than a constant offset.
    function _varint(bytes memory b, uint256 off) internal pure returns (uint256 value, uint256 next) {
        uint256 shift = 0;
        while (true) {
            require(off < b.length, "journal truncated inside a varint");
            uint8 x = uint8(b[off]);
            off += 1;
            value |= uint256(x & 0x7f) << shift;
            if (x & 0x80 == 0) break;
            shift += 7;
            require(shift <= 63, "journal varint too long");
        }
        return (value, off);
    }

    /// The trailing assertion word of the journal's `expected_output`.
    ///
    /// Walks the postcard prefix: version, valid, the 32-byte fingerprint, the chain code,
    /// the index, then the `u8` word vector whose last element is the accumulated
    /// conjunction of every in-circuit assertion.
    ///
    /// 🪤 It is the last **u8** word, not the last byte of the journal, and not the last
    /// byte of `expected_output` either: `Words` stores wider word types after the u8
    /// vector, and this statement's output carries u64 words after it.
    function _assertionWord(bytes memory pv) internal pure returns (uint8) {
        uint256 off;
        (, off) = _varint(pv, 0);          // version
        off += 1;                          // valid
        off += 32;                         // fingerprint
        uint256 len;
        (len, off) = _varint(pv, off);     // chain_code length
        off += len;
        (, off) = _varint(pv, off);        // index
        (len, off) = _varint(pv, off);     // u8 word count
        require(len > 0, "expected output has no u8 words, so no assertion word");
        require(off + len <= pv.length, "journal truncated inside expected_output");
        return uint8(pv[off + len - 1]);
    }

    /// The offset `_assertionWord` reads, exposed so a test can flip exactly that byte and
    /// show the guard depends on it rather than on something incidental.
    function _assertionWordOffset(bytes memory pv) internal pure returns (uint256) {
        uint256 off;
        (, off) = _varint(pv, 0);
        off += 33;
        uint256 len;
        (len, off) = _varint(pv, off);
        off += len;
        (, off) = _varint(pv, off);
        (len, off) = _varint(pv, off);
        return off + len - 1;
    }

    /// The parser points at the byte we think it does: flipping exactly that byte in a copy
    /// of the real fixture is what changes the answer, and nothing else in the journal does.
    function test_assertion_word_is_located_in_the_real_fixture() public view {
        assertEq(_assertionWord(publicValues), 1, "the acceptance fixture must assert");
        bytes memory vacuous = publicValues;
        uint256 off = _assertionWordOffset(vacuous);
        vacuous[off] = bytes1(uint8(0));
        assertEq(_assertionWord(vacuous), 0, "flipping the located byte must clear the assertion");
    }

    /// The guard itself, on a journal that is valid in every other respect. This is the
    /// forgery it exists for: the prover ran the honest protocol on a witness that does not
    /// satisfy the circuit, so the proof verifies, the fingerprint matches, the response
    /// count is 219, and the statement asserts nothing.
    function test_gate_rejects_a_vacuous_journal() public {
        bytes memory vacuous = publicValues;
        vacuous[_assertionWordOffset(vacuous)] = bytes1(uint8(0));
        vm.expectRevert(bytes("journal's assertion word is not set: the proof attests nothing"));
        this.requireAssertionWord(vacuous);
    }

    /// External so `vm.expectRevert` can catch the require above.
    function requireAssertionWord(bytes memory pv) public pure {
        require(_assertionWord(pv) == 1, "journal's assertion word is not set: the proof attests nothing");
    }

    function test_lift_verifies_and_measures_gas() public view {
        uint256 before = gasleft();
        verifier.verifyProof(vkey, publicValues, proofBytes);
        uint256 used = before - gasleft();
        console2.log("lift verify gas:", used);
    }

    function test_rejects_corrupted_proof() public {
        bytes memory bad = proofBytes;
        bad[bad.length - 1] ^= 0x01;
        vm.expectRevert(Verifier.ProofInvalid.selector);
        verifier.verifyProof(vkey, publicValues, bad);
    }

    function test_rejects_wrong_vkey() public {
        bytes32 badKey = vkey ^ bytes32(uint256(1));
        vm.expectRevert(Verifier.ProofInvalid.selector);
        verifier.verifyProof(badKey, publicValues, proofBytes);
    }

    function test_rejects_wrong_public_values() public {
        bytes memory bad = publicValues;
        bad[0] ^= 0x01;
        vm.expectRevert(Verifier.ProofInvalid.selector);
        verifier.verifyProof(vkey, bad, proofBytes);
    }

    function test_rejects_wrong_selector() public {
        bytes memory bad = proofBytes;
        bad[0] ^= 0x01;
        vm.expectRevert(
            abi.encodeWithSelector(
                SP1Verifier.WrongVerifierSelector.selector,
                bytes4(bad),
                bytes4(verifier.VERIFIER_HASH())
            )
        );
        verifier.verifyProof(vkey, publicValues, bad);
    }
}

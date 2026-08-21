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
/// and the soundness-critical minimum response count — exactly as `check_journal` in
/// the statement crate specifies. Omitting the count check alone re-opens acceptance of
/// proofs at prover-chosen soundness (the ZKBoo verifier accepts even the empty proof).
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
    /// fingerprint from `33e1f70b…` to `e23ddb6a…` without touching the journal shape.
    /// Update it deliberately when the circuit changes, which is the point.
    bytes32 internal constant EXPECTED_FINGERPRINT =
        0xe23ddb6a1be341ba770275fd73b33730a051dfc998cfa339fb19fabc57e2df1b;

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
    bytes32 internal constant EXPECTED_VKEY =
        0x004dc0b7561f87fc31370bec92a601824bf6619c47a232212d4fb3976298ca69;

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

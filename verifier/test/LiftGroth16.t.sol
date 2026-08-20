// SPDX-License-Identifier: LGPL-3.0-or-later
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {console2} from "forge-std/console2.sol";
import {SP1Verifier} from "../src/vendor/SP1VerifierGroth16.sol";
import {Verifier} from "../src/vendor/Groth16Verifier.sol";

/// On-chain verification of the lifted proof of seed, against the audited SP1 Groth16
/// verifier (vendored from sp1-contracts v6.1.0), on a real fixture exported by the
/// host (`s3-wrap`, fully local native-gnark wrap): the verifying-key hash, the public
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
            vm.skip(true);
        }
        string memory json = vm.readFile(fixture);
        vkey = vm.parseJsonBytes32(json, ".vkey");
        publicValues = vm.parseJsonBytes(json, ".publicValues");
        proofBytes = vm.parseJsonBytes(json, ".proof");
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

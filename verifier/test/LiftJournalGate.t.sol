// SPDX-License-Identifier: LGPL-3.0-or-later
pragma solidity ^0.8.20;

import {Test} from "forge-std/Test.sol";
import {console2} from "forge-std/console2.sol";
import {LiftJournalGate as G} from "../src/LiftJournalGate.sol";

/// The on-chain relying-party gate, against the real acceptance fixture.
///
/// Every negative below is produced by mutating that fixture, so each one is a journal
/// that differs from an accepted one in exactly the field under test. The proof is not
/// re-verified here — a mutated journal would fail the pairing check, which would prove
/// nothing about the gate. What is under test is what the gate does with public values
/// once `verifyProof` has accepted them, which is the only thing standing between a
/// relying contract and a valid proof of the wrong statement.
contract LiftJournalGateTest is Test {
    /// `REQUIRED_PROOF_FORMAT_ID` from the statement crate: the digest of ZKBoo's canonical
    /// reference proof.
    ///
    /// A third copy of that digest, after the statement crate's and zkboo's own, and
    /// deliberately so: a relying contract is a separate trust domain and must pin what it
    /// requires rather than read it out of the thing it is checking. Update it here whenever the
    /// Rust constant moves, and expect the fixture to need regenerating in the same breath.
    bytes32 internal constant EXPECTED_PROOF_FORMAT_ID =
        0x4be5d4e64c11ba4d40e78977f68d45ad8bf01fe31aa27507e8ddca20223a7370;

    bytes internal pv;
    G.Expectation internal expectation;

    function setUp() public {
        string memory fixture = vm.envOr("LIFT_FIXTURE", string("fixtures/lift-219.onchain.json"));
        if (!vm.exists(fixture)) {
            vm.skip(true, "no fixture: this suite gated NOTHING; generate with lift 219 groth16");
        }
        pv = vm.parseJsonBytes(vm.readFile(fixture), ".publicValues");
        G.Journal memory j = G.decode(pv);
        // The expectation is built from the fixture's own statement fields, which is what a
        // relying contract would have pinned at deployment. It is NOT built from the
        // assertion word or the response count — those are requirements, not observations,
        // and taking them from the journal is exactly the mistake this gate exists to stop.
        expectation = G.Expectation({
            version: j.version,
            fingerprint: j.fingerprint,
            chainCodeHash: keccak256(j.chainCode),
            index: j.index,
            outputU8Hash: keccak256(j.outputU8),
            bindingHash: keccak256(j.binding),
            minResponses: 219,
            proofFormatId: EXPECTED_PROOF_FORMAT_ID
        });
    }

    function test_decodes_the_real_journal() public view {
        G.Journal memory j = G.decode(pv);
        assertEq(j.version, 3, "journal version");
        assertTrue(j.valid, "valid");
        assertEq(j.chainCode.length, 32, "chain code length");
        assertEq(j.index, 0x80000000, "hardened index");
        assertEq(j.numResponses, 219, "response count");
        assertEq(j.proofFormatId, EXPECTED_PROOF_FORMAT_ID, "proof format");
        assertEq(j.assertionWord, 1, "assertion word must be set");
        // 32 key bytes plus the assertion word.
        assertEq(j.outputU8.length, 33, "u8 output words");
    }

    function test_accepts_the_real_journal() public view {
        G.Journal memory j = G.check(pv, expectation);
        assertEq(j.numResponses, 219);
    }

    /// The forgery. Everything else about this journal is impeccable: right fingerprint,
    /// right statement, 219 responses, right format. Only the assertion word is clear, and
    /// the expectation is regenerated to agree with the mutation in every other field — so
    /// no clause but this one can reject it.
    function test_rejects_a_vacuous_journal() public {
        bytes memory bad = _withAssertionWord(0);
        G.Journal memory j = G.decode(bad);
        G.Expectation memory e = expectation;
        e.outputU8Hash = keccak256(j.outputU8);
        vm.expectRevert(abi.encodeWithSelector(G.AssertionWordNotSet.selector, uint8(0)));
        this.gate(bad, e);
    }

    /// The format clause, which is the one this journal shape exists to make workable. Its
    /// predecessor was a counter that sat at 3 across an incompatible change of the proof
    /// format, so the clause could not fire on the transition it was written for; the identity
    /// is the digest of the format's own bytes, so a moved format moves it.
    function test_rejects_a_different_proof_format() public {
        G.Expectation memory e = expectation;
        e.proofFormatId = bytes32(uint256(e.proofFormatId) ^ 1);
        vm.expectRevert(
            abi.encodeWithSelector(G.ProofFormatMismatch.selector, EXPECTED_PROOF_FORMAT_ID, e.proofFormatId)
        );
        this.gate(pv, e);
    }

    function test_rejects_too_few_responses() public {
        G.Expectation memory e = expectation;
        e.minResponses = 438;
        vm.expectRevert(abi.encodeWithSelector(G.TooFewResponses.selector, uint32(219), uint32(438)));
        this.gate(pv, e);
    }

    function test_rejects_a_different_circuit() public {
        G.Expectation memory e = expectation;
        e.fingerprint = bytes32(uint256(e.fingerprint) ^ 1);
        vm.expectRevert();
        this.gate(pv, e);
    }

    function test_rejects_a_different_statement() public {
        G.Expectation memory e = expectation;
        e.index = 0;
        vm.expectRevert(G.StatementMismatch.selector);
        this.gate(pv, e);
    }

    function test_rejects_a_different_binding() public {
        G.Expectation memory e = expectation;
        e.bindingHash = keccak256("some other transaction");
        vm.expectRevert(G.BindingMismatch.selector);
        this.gate(pv, e);
    }

    function test_rejects_trailing_bytes() public {
        bytes memory bad = bytes.concat(pv, hex"00");
        vm.expectRevert(G.JournalTrailingBytes.selector);
        this.gate(bad, expectation);
    }

    function test_rejects_truncation() public {
        bytes memory bad = new bytes(pv.length - 1);
        for (uint256 i = 0; i < bad.length; i++) bad[i] = pv[i];
        vm.expectRevert();
        this.gate(bad, expectation);
    }

    /// Gas for the gate itself.
    ///
    /// 🪤 Same trap as `LiftGroth16.t.sol`: `pv` and `expectation` are storage fields, and reading
    /// them inside the measured region charges a cold SLOAD per word — about 34,700 gas here,
    /// nearly two fifths of the figure, all of it the test harness. A relying contract takes the
    /// journal from calldata and has its expectation pinned. Both are copied to memory before the
    /// clock starts. The storage-resident figure is NOT re-measured here: a second call in the
    /// same test is warm and reproduces nothing, so it is recorded below instead.
    ///
    /// Measured 2026-09-07: memory-resident 54,610, storage-resident 89,322.
    function test_gas() public view {
        bytes memory values = pv;
        G.Expectation memory e = expectation;
        uint256 before = gasleft();
        G.check(values, e);
        console2.log("journal gate gas (memory-resident, what a caller pays):", before - gasleft());
    }

    /// External so `vm.expectRevert` catches reverts from the library call.
    function gate(bytes memory values, G.Expectation memory e) public pure {
        G.check(values, e);
    }

    /// A copy of the fixture with the trailing u8 output word replaced.
    function _withAssertionWord(uint8 w) internal view returns (bytes memory out) {
        out = pv;
        uint256 off = _assertionOffset(out);
        out[off] = bytes1(w);
        return out;
    }

    function _assertionOffset(bytes memory b) internal pure returns (uint256) {
        uint256 off = 1 + 1 + 32;            // version varint (one byte here), valid, fingerprint
        uint256 len = uint8(b[off]);         // chain code length (32, single-byte varint)
        off += 1 + len;
        while (uint8(b[off]) & 0x80 != 0) off++;   // index varint
        off++;
        len = uint8(b[off]);                 // u8 word count (33, single-byte varint)
        off += 1;
        return off + len - 1;
    }
}

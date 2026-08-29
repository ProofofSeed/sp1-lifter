// SPDX-License-Identifier: LGPL-3.0-or-later
pragma solidity ^0.8.20;

/// The relying-party gate over a lifted proof of seed, on chain.
///
/// `SP1VerifierGroth16.verifyProof` establishes only *"the program identified by this
/// verifying key ran successfully and committed these public values"*. It is not an
/// authorisation. This library is the second half: it decodes the public values — the
/// postcard-encoded lift journal — and enforces the checks `check_journal` in the
/// statement crate specifies. A contract that verifies a proof and reads an address out of
/// the public values without coming through here is not gated.
///
/// ⚠️ The verifying key is NOT checked here, because it is not in the journal. The caller
/// must pin it and pass it to `verifyProof` itself; a journal is self-consistent under
/// whichever program produced it, so without a pinned key this library attests nothing
/// about which program ran.
///
/// Two clauses re-open a total break on their own, and both are easy to omit because the
/// journal looks well-formed without them:
///
/// - **`minResponses`.** ZKBoo's soundness is (2/3)^k in the number of responses, and its
///   verifier accepts any count, the empty proof included. Skip this and the prover picks
///   its own soundness.
/// - **The assertion word.** ZKBoo 2.0.0 leaves prover advice unconstrained except by the
///   circuit's in-circuit assertions, accumulated by conjunction into the last word of the
///   output. Run the honest prover on a witness that does *not* satisfy the circuit and you
///   get a perfectly valid proof whose assertion word is 0 — a genuine proof of "I ran this
///   circuit and its assertions did not hold", obtainable by anyone, for any address, with
///   no seed. A gate that reads the twenty address bytes and ignores that word authorises
///   everybody.
library LiftJournalGate {
    error JournalTruncated();
    error JournalTrailingBytes();
    error VarintTooLong();
    error VersionMismatch(uint32 found, uint32 expected);
    error NotValid();
    error FingerprintMismatch(bytes32 found, bytes32 expected);
    error StatementMismatch();
    error BindingMismatch();
    error TooFewResponses(uint32 found, uint32 required);
    error ProofFormatMismatch(uint32 found, uint32 expected);
    error AssertionWordMissing();
    error AssertionWordNotSet(uint8 found);

    /// A decoded lift journal. `outputU8` is the `u8` word vector of the circuit's expected
    /// output, whose final element is `assertionWord`.
    struct Journal {
        uint32 version;
        bool valid;
        bytes32 fingerprint;
        bytes chainCode;
        uint32 index;
        bytes outputU8;
        uint8 assertionWord;
        bytes binding;
        uint32 numResponses;
        uint32 proofFormatVersion;
    }

    /// What a consumer requires. Hashes rather than bytes, so a statement pins into one
    /// storage word each.
    struct Expectation {
        uint32 version;
        bytes32 fingerprint;
        bytes32 chainCodeHash;
        uint32 index;
        bytes32 outputU8Hash;
        bytes32 bindingHash;
        uint32 minResponses;
        uint32 proofFormatVersion;
    }

    /// Decodes and gates in one call. Reverts unless every clause holds.
    function check(bytes memory pv, Expectation memory e) internal pure returns (Journal memory j) {
        j = decode(pv);
        if (j.version != e.version) revert VersionMismatch(j.version, e.version);
        if (!j.valid) revert NotValid();
        if (j.fingerprint != e.fingerprint) revert FingerprintMismatch(j.fingerprint, e.fingerprint);
        if (keccak256(j.chainCode) != e.chainCodeHash || j.index != e.index) revert StatementMismatch();
        if (keccak256(j.outputU8) != e.outputU8Hash) revert StatementMismatch();
        if (keccak256(j.binding) != e.bindingHash) revert BindingMismatch();
        if (j.numResponses < e.minResponses) revert TooFewResponses(j.numResponses, e.minResponses);
        if (j.proofFormatVersion != e.proofFormatVersion) {
            revert ProofFormatMismatch(j.proofFormatVersion, e.proofFormatVersion);
        }
        // Unconditional, and deliberately not derived from the expectation: an expectation
        // agreeing with the journal in every other field would otherwise sail through.
        if (j.assertionWord != 1) revert AssertionWordNotSet(j.assertionWord);
        return j;
    }

    /// Decodes the postcard-encoded journal, enforcing exact consumption.
    ///
    /// Every integer field is a LEB128 varint, so nothing here can be read at a fixed
    /// offset: the index alone is one byte for an unhardened child and five for a hardened
    /// one. The assertion word is the last **u8** word — not the last byte of the journal,
    /// and not the last byte of the expected output either, since `Words` stores wider word
    /// types after the u8 vector and these statements carry u64 words there.
    function decode(bytes memory pv) internal pure returns (Journal memory j) {
        uint256 off;
        uint256 len;
        (j.version, off) = _u32(pv, 0);
        if (off >= pv.length) revert JournalTruncated();
        j.valid = uint8(pv[off]) == 1;
        off += 1;
        if (off + 32 > pv.length) revert JournalTruncated();
        bytes32 fp;
        assembly { fp := mload(add(add(pv, 32), off)) }
        j.fingerprint = fp;
        off += 32;
        (len, off) = _len(pv, off);
        j.chainCode = _slice(pv, off, len);
        off += len;
        (j.index, off) = _u32(pv, off);
        (len, off) = _len(pv, off);
        if (len == 0) revert AssertionWordMissing();
        j.outputU8 = _slice(pv, off, len);
        j.assertionWord = uint8(j.outputU8[len - 1]);
        off += len;
        // The u32 and u64 word vectors: skipped, but skipped by *parsing*, since their
        // elements are varints of unpredictable width.
        off = _skipVarints(pv, off);
        off = _skipVarints(pv, off);
        (len, off) = _len(pv, off);
        j.binding = _slice(pv, off, len);
        off += len;
        (j.numResponses, off) = _u32(pv, off);
        (j.proofFormatVersion, off) = _u32(pv, off);
        // Trailing bytes would let public values smuggle data past the decoder.
        if (off != pv.length) revert JournalTrailingBytes();
        return j;
    }

    function _skipVarints(bytes memory b, uint256 off) private pure returns (uint256) {
        uint256 count;
        (count, off) = _varint(b, off);
        for (uint256 k = 0; k < count; k++) {
            (, off) = _varint(b, off);
        }
        return off;
    }

    function _varint(bytes memory b, uint256 off) private pure returns (uint256 value, uint256 next) {
        uint256 shift;
        while (true) {
            if (off >= b.length) revert JournalTruncated();
            uint8 x = uint8(b[off]);
            off += 1;
            value |= uint256(x & 0x7f) << shift;
            if (x & 0x80 == 0) break;
            shift += 7;
            if (shift > 63) revert VarintTooLong();
        }
        return (value, off);
    }

    function _u32(bytes memory b, uint256 off) private pure returns (uint32 value, uint256 next) {
        uint256 v;
        (v, next) = _varint(b, off);
        if (v > type(uint32).max) revert VarintTooLong();
        return (uint32(v), next);
    }

    function _len(bytes memory b, uint256 off) private pure returns (uint256 value, uint256 next) {
        (value, next) = _varint(b, off);
        if (next + value > b.length) revert JournalTruncated();
        return (value, next);
    }

    function _slice(bytes memory b, uint256 off, uint256 len) private pure returns (bytes memory out) {
        out = new bytes(len);
        for (uint256 k = 0; k < len; k++) {
            out[k] = b[off + k];
        }
        return out;
    }
}

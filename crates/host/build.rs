// SPDX-License-Identifier: LGPL-3.0-or-later

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The zkboo release each crate this workspace links is built against.
///
/// Declared here rather than read from whatever the lockfile happens to say, deliberately:
/// a check that takes its expectation from the thing it is checking cannot fail. Moving to a
/// new zkboo release is a decision someone makes by editing this table, and until they do, a
/// tree that resolves anything else does not build. The crates are released independently, so
/// the version is per crate; a zkboo crate missing from the table is refused.
const REQUIRED_ZKBOO_VERSIONS: &[(&str, &str)] = &[
    ("zkboo", "1.3.1"),
    ("zkboo-bip32", "1.3.0"),
    ("zkboo-circuit-hash", "1.3.0"),
    ("zkboo-ecc", "1.3.0"),
    ("zkboo-hmac", "1.3.0"),
    ("zkboo-keccak", "1.2.0"),
    ("zkboo-modular", "1.3.0"),
    ("zkboo-profiling", "1.3.0"),
    ("zkboo-ripemd160", "1.2.0"),
    ("zkboo-sha2", "1.3.0"),
];

/// The workspace root, two levels up from this crate under `crates/`.
fn workspace_root() -> PathBuf {
    return Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
}

/// The lockfile's source for crates.io.
const CRATES_IO: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// Refuses to build unless every zkboo crate resolves at its required version, from crates.io.
///
/// This exists because of a failure that has already happened on this project. Every manifest
/// here once required `zkboo = "1.0.0"` and redirected it through `[patch.crates-io]` to a local
/// working tree; when that tree's version moved on, the local version stopped satisfying the
/// requirement, cargo **silently ignored the patch** and resolved the published crate instead.
/// Nothing in the resulting artefacts revealed it — the proofs were well-formed and simply failed
/// to verify — and the provenance that was being recorded came from `git describe` on the working
/// tree, so it named a version the binary did not contain.
///
/// The guard therefore reads the lockfile, which records what was actually resolved, and never
/// the working tree. A registry source pins the bytes by checksum, which this records into the
/// binary; a path source, from a `[patch]` in a manifest or a cargo configuration, pins nothing
/// and is refused. Between 2026-09 and 2026-11-24, while these releases were unpublished, the
/// requirement ran the other way and a path source was the only correct one.
fn check_zkboo_provenance() -> String {
    let lock_path = workspace_root().join("Cargo.lock");
    println!("cargo::rerun-if-changed={}", lock_path.display());
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", lock_path.display()));
    let mut found = 0usize;
    let mut summary = String::new();
    for block in lock.split("[[package]]") {
        let field = |key: &str| -> Option<&str> {
            block
                .lines()
                .map(str::trim)
                .find_map(|l| l.strip_prefix(key)?.strip_prefix(" = \"")?.strip_suffix('"'))
        };
        let Some(name) = field("name") else { continue };
        if !name.starts_with("zkboo") {
            continue;
        }
        let version = field("version").unwrap_or("<none>");
        let source = field("source").unwrap_or("a path");
        let Some(&(_, required)) = REQUIRED_ZKBOO_VERSIONS.iter().find(|(n, _)| *n == name) else {
            panic!(
                "provenance guard: {name} is in the lockfile but has no required version. Add it \
                 to REQUIRED_ZKBOO_VERSIONS once the release it should be built against is decided.",
            );
        };
        assert!(
            version == required,
            "provenance guard: {name} resolves to {version}, not the required {required}. Fix the \
             requirement rather than this check.",
        );
        assert!(
            source == CRATES_IO,
            "provenance guard: {name} {version} resolves from {source} rather than from crates.io, \
             so a [patch] is in force and the bytes being compiled are not the published ones.",
        );
        let checksum = field("checksum").unwrap_or("<none>");
        let _ = write!(
            summary,
            "{name}={version}@{} ",
            &checksum[..checksum.len().min(12)]
        );
        found += 1;
    }
    assert!(
        found >= 8,
        "provenance guard: found only {found} zkboo crates in the lockfile. This workspace links \
         at least eight, so either the lockfile is not the one this build resolves against or the \
         parser above has stopped matching it.",
    );
    return format!(
        "{found} crates at their required versions from crates.io: {}",
        summary.trim_end(),
    );
}

fn main() {
    let provenance = check_zkboo_provenance();
    println!("cargo::rustc-env=ZKBOO_PROVENANCE={provenance}");
    sp1_build::build_program_with_args("../../programs/program", Default::default());
    sp1_build::build_program_with_args("../../programs/program-elephant", Default::default());
    sp1_build::build_program_with_args("../../programs/program-leaf", Default::default());
    sp1_build::build_program_with_args("../../programs/program-root", Default::default());
    sp1_build::build_program_with_args("../../programs/program-leaf-elephant", Default::default());
    sp1_build::build_program_with_args("../../programs/program-root-elephant", Default::default());
}

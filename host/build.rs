// SPDX-License-Identifier: LGPL-3.0-or-later

use std::fmt::Write as _;

/// The zkboo release this workspace is built against.
///
/// Declared here rather than read from whatever the lockfile happens to say, deliberately:
/// a check that takes its expectation from the thing it is checking cannot fail. Moving to a
/// new zkboo release is a decision someone makes by editing this line, and until they do, a
/// tree that resolves anything else does not build.
const REQUIRED_ZKBOO_VERSION: &str = "2.0.0";

/// Refuses to build unless every zkboo crate resolves from crates.io at the required version.
///
/// This exists because of a failure that has already happened on this project. Every manifest
/// here once required `zkboo = "1.0.0"` and redirected it through `[patch.crates-io]` to a local
/// working tree; when that tree was released as 2.0.0, the local version stopped satisfying the
/// requirement, cargo **silently ignored the patch** and resolved the published 1.0.0 instead.
/// Nothing in the resulting artefacts revealed it — the proofs were well-formed and simply failed
/// to verify — and the provenance that was being recorded came from `git describe` on the working
/// tree, so it named a version the binary did not contain.
///
/// The lockfile is therefore the only acceptable source of provenance: it records the version
/// actually resolved, its origin, and a checksum over the bytes that were compiled.
fn check_zkboo_provenance() -> String {
    let lock_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.lock");
    println!("cargo::rerun-if-changed={}", lock_path.display());
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", lock_path.display()));
    let mut summary = String::new();
    let mut found = 0usize;
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
        let source = field("source").unwrap_or("<none: a local path or patch override>");
        let checksum = field("checksum").unwrap_or("<none>");
        assert!(
            version == REQUIRED_ZKBOO_VERSION,
            "provenance guard: {name} resolves to {version}, not the required \
             {REQUIRED_ZKBOO_VERSION}. Fix the requirement rather than this check.",
        );
        assert!(
            source.starts_with("registry+"),
            "provenance guard: {name} {version} does not resolve from a registry \
             (source: {source}). A path or git override means the bytes that get compiled are \
             whatever a working tree holds at build time, which no artefact can record. Remove \
             the override, or change this guard deliberately if a local build is what is wanted.",
        );
        assert!(
            checksum.len() == 64,
            "provenance guard: {name} {version} carries no lockfile checksum, so the lockfile \
             pins its origin but not its contents.",
        );
        found += 1;
        let _ = write!(summary, "{name}={version}/{} ", &checksum[..8]);
    }
    assert!(
        found >= 8,
        "provenance guard: found only {found} zkboo crates in the lockfile. This workspace links \
         at least eight, so either the lockfile is not the one this build resolves against or the \
         parser above has stopped matching it.",
    );
    return format!("{found} crates from crates.io: {}", summary.trim_end());
}

fn main() {
    let provenance = check_zkboo_provenance();
    println!("cargo::rustc-env=ZKBOO_PROVENANCE={provenance}");
    sp1_build::build_program_with_args("../program", Default::default());
    sp1_build::build_program_with_args("../program-elephant", Default::default());
    sp1_build::build_program_with_args("../program-leaf", Default::default());
    sp1_build::build_program_with_args("../program-root", Default::default());
    sp1_build::build_program_with_args("../program-leaf-elephant", Default::default());
    sp1_build::build_program_with_args("../program-root-elephant", Default::default());
}

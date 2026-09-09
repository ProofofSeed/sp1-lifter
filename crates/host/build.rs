// SPDX-License-Identifier: LGPL-3.0-or-later

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The zkboo release this workspace is built against.
///
/// Declared here rather than read from whatever the lockfile happens to say, deliberately:
/// a check that takes its expectation from the thing it is checking cannot fail. Moving to a
/// new zkboo release is a decision someone makes by editing this line, and until they do, a
/// tree that resolves anything else does not build.
const REQUIRED_ZKBOO_VERSION: &str = "1.2.0";

/// The workspace root, two levels up from this crate under `crates/`.
fn workspace_root() -> PathBuf {
    return Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
}

/// The paths `[patch.crates-io]` redirects the zkboo crates to, read from the root manifest.
///
/// Read rather than assumed: the guard below reports which working tree each crate was compiled
/// from, and a hardcoded path would report a location the build need not have used.
fn patched_paths() -> Vec<(String, PathBuf)> {
    let manifest_path = workspace_root().join("Cargo.toml");
    println!("cargo::rerun-if-changed={}", manifest_path.display());
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", manifest_path.display()));
    let mut paths = Vec::new();
    for line in manifest.lines().map(str::trim) {
        let Some((name, rest)) = line.split_once('=') else { continue };
        let name = name.trim();
        if !name.starts_with("zkboo") {
            continue;
        }
        let Some(start) = rest.find("path = \"") else { continue };
        let rest = &rest[start + "path = \"".len()..];
        let Some(end) = rest.find('"') else { continue };
        paths.push((name.to_string(), workspace_root().join(&rest[..end])));
    }
    return paths;
}

/// A one-line description of a checkout: its short revision, and whether it has local edits.
///
/// ⚠️ A dirty tree is reported, not refused. Refusing would make the crates unusable for the
/// development they exist for. A measurement taken against uncommitted work says so on its face.
fn checkout_provenance(name: &str, path: &Path) -> String {
    let head = path.join(".git/HEAD");
    if head.exists() {
        println!("cargo::rerun-if-changed={}", head.display());
    }
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git").arg("-C").arg(path).args(args).output().ok()?;
        if !out.status.success() {
            return None;
        }
        return Some(String::from_utf8_lossy(&out.stdout).trim().to_string());
    };
    let Some(rev) = git(&["rev-parse", "--short=12", "HEAD"]) else {
        return format!("{name}=path:{}(no git)", path.display());
    };
    let dirty = match git(&["status", "--porcelain"]) {
        Some(s) if !s.is_empty() => "-dirty",
        Some(_) => "",
        None => "-unknown",
    };
    return format!("{name}={rev}{dirty}");
}

/// Refuses to build unless every zkboo crate resolves at the required version, from a path.
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
/// the working tree. What it requires has been inverted since that failure, because zkboo 1.2.0
/// is unpublished and every published version is yanked: a registry source is now the symptom of
/// a disengaged patch rather than the desired state, and a path source is what a correct build
/// looks like. The version requirement does the rest — `=1.2.0` matches nothing on crates.io, so
/// a disengaged patch fails to resolve rather than resolving elsewhere.
///
/// Restore the registry form of this check when 1.2.0 is published.
fn check_zkboo_provenance() -> String {
    let lock_path = workspace_root().join("Cargo.lock");
    println!("cargo::rerun-if-changed={}", lock_path.display());
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", lock_path.display()));
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
        let source = field("source").unwrap_or("");
        assert!(
            version == REQUIRED_ZKBOO_VERSION,
            "provenance guard: {name} resolves to {version}, not the required \
             {REQUIRED_ZKBOO_VERSION}. Fix the requirement rather than this check.",
        );
        assert!(
            source.is_empty(),
            "provenance guard: {name} {version} resolves from {source} rather than from a path \
             override. While zkboo is unpublished that means the patch table has disengaged and \
             the bytes being compiled are not the ones intended.",
        );
        found += 1;
    }
    assert!(
        found >= 8,
        "provenance guard: found only {found} zkboo crates in the lockfile. This workspace links \
         at least eight, so either the lockfile is not the one this build resolves against or the \
         parser above has stopped matching it.",
    );
    let patched = patched_paths();
    assert!(
        patched.len() >= found,
        "provenance guard: the lockfile carries {found} zkboo crates but the root manifest \
         patches only {}. An unpatched crate would be resolved from the registry.",
        patched.len(),
    );
    let mut summary = String::new();
    for (name, path) in &patched {
        let _ = write!(summary, "{} ", checkout_provenance(name, path));
    }
    return format!(
        "{found} crates at {REQUIRED_ZKBOO_VERSION} from local paths: {}",
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

//! Machine-checked enforcement of the §1 dependency constraints.
//!
//! > Dependency direction is strictly downward; `mt-doc`, `mt-inline`,
//! > `mt-md`, and `mt-layout` never depend on `mt-ui` or `mt-app`. Those four
//! > compile and test **without a window**, which is what makes §11 runnable
//! > in CI.
//!
//! That sentence is the load-bearing architectural invariant of the whole
//! plan: the conformance ratchet, the differential harness, the round-trip
//! property tests, and the fuzzing gate all rest on those four crates being
//! runnable headlessly on a CI runner with no display and no GPU. It is also
//! exactly the kind of constraint that erodes one convenient `use` at a time.
//!
//! Cargo will catch a *cycle* but not a one-way edge, so these checks exist.
//! They are deliberately crude — a line scan over manifests we own, not a TOML
//! parser — because the alternative is a dependency, and because the failure
//! mode of crudeness here is a false positive that someone reads and fixes.

use std::path::Path;

/// The four crates that must stay headless.
pub const HEADLESS: [&str; 4] = ["mt-doc", "mt-inline", "mt-md", "mt-layout"];

/// Crates they must never reach.
pub const FORBIDDEN_INTERNAL: [&str; 2] = ["mt-ui", "mt-app"];

/// Third-party crates that would break "no windowing, no GPU, no I/O" if they
/// appeared in a headless crate's manifest. Not exhaustive — a denylist never
/// is — but it names the ones the plan actually schedules, so an accidental
/// early adoption is caught at the moment it happens.
pub const FORBIDDEN_THIRD_PARTY: [&str; 12] = [
    "winit",
    "wgpu",
    "vello",
    "tiny-skia",
    "accesskit",
    "muda",
    "rfd",
    "keyring",
    "notify",
    "tree-sitter",
    "ignore",
    "grep-searcher",
];

/// All 14 crates from the §1 workspace layout, in the order the plan lists
/// them.
pub const WORKSPACE_CRATES: [&str; 14] = [
    "mt-doc",
    "mt-inline",
    "mt-md",
    "mt-layout",
    "mt-render",
    "mt-highlight",
    "mt-math",
    "mt-diagram",
    "mt-export",
    "mt-fs",
    "mt-search",
    "mt-ui",
    "mt-app",
    "mt-cli",
];

/// Read the dependency names declared by a crate's manifest.
///
/// Scans every `[*dependencies*]` table, including target-specific ones, and
/// takes the key of each entry.
pub fn declared_dependencies(manifest: &str) -> Vec<String> {
    let mut deps = Vec::new();
    let mut in_deps = false;

    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`,
            // `[target.'cfg(...)'.dependencies]`.
            in_deps = line.ends_with("dependencies]");
            continue;
        }
        if !in_deps || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        // `mt-doc.workspace = true` declares `mt-doc`; take the part before
        // the first dot.
        let name = key
            .trim()
            .trim_matches('"')
            .split('.')
            .next()
            .unwrap_or("")
            .trim();
        if !name.is_empty() {
            deps.push(name.to_string());
        }
    }
    deps
}

/// Source files under a crate, recursively.
fn source_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            source_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Every violation found, as human-readable lines.
pub fn check(repo_root: &Path) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    let crates_dir = repo_root.join("crates");

    // Every crate the plan lists exists, and no extras have appeared.
    let mut found: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&crates_dir)
        .map_err(|e| format!("cannot read {}: {e}", crates_dir.display()))?
    {
        let path = entry.map_err(|e| format!("{e}"))?.path();
        if path.is_dir() {
            found.push(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    found.sort();
    let mut expected: Vec<String> = WORKSPACE_CRATES.iter().map(|s| s.to_string()).collect();
    expected.sort();
    if found != expected {
        violations.push(format!(
            "crates/ does not match the §1 workspace layout.\n  found:    {found:?}\n  expected: {expected:?}"
        ));
    }

    for name in WORKSPACE_CRATES {
        let manifest_path = crates_dir.join(name).join("Cargo.toml");
        let manifest = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("cannot read {}: {e}", manifest_path.display()))?;
        let deps = declared_dependencies(&manifest);

        if deps.iter().any(|d| d == name) {
            violations.push(format!("{name} depends on itself"));
        }

        if HEADLESS.contains(&name) {
            for forbidden in FORBIDDEN_INTERNAL {
                if deps.iter().any(|d| d == forbidden) {
                    violations.push(format!(
                        "{name} depends on {forbidden}. §1: the four headless crates must compile \
                         and test without a window."
                    ));
                }
            }
            for forbidden in FORBIDDEN_THIRD_PARTY {
                if deps.iter().any(|d| d == forbidden) {
                    violations.push(format!(
                        "{name} depends on {forbidden}, which breaks \
                         \"no windowing, no GPU, no I/O\" (§1)."
                    ));
                }
            }

            // Source-level: the headless four must not reach the filesystem.
            // `mt-cli` and `mt-fs` are where I/O belongs.
            let mut files = Vec::new();
            source_files(&crates_dir.join(name).join("src"), &mut files)?;
            for file in files {
                let text = std::fs::read_to_string(&file)
                    .map_err(|e| format!("cannot read {}: {e}", file.display()))?;
                for needle in ["std::fs::", "std::net::", "std::process::"] {
                    if text.contains(needle) {
                        violations.push(format!(
                            "{} uses {needle} — §1 says {name} does no I/O. If a test genuinely \
                             needs a fixture, move it to an integration test and narrow this \
                             check.",
                            file.strip_prefix(repo_root).unwrap_or(&file).display()
                        ));
                    }
                }
            }
        }
    }

    Ok(violations)
}

/// `cargo xtask deps`.
pub fn main(repo_root: &Path) -> Result<i32, String> {
    println!("dependency-direction guard — RUST-REWRITE-PLAN.md §1");
    let violations = check(repo_root)?;
    if violations.is_empty() {
        println!(
            "  ok  {} crates; {} headless crates reach neither mt-ui nor mt-app",
            WORKSPACE_CRATES.len(),
            HEADLESS.len()
        );
        return Ok(0);
    }
    for v in &violations {
        println!("  FAIL  {v}");
    }
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_workspace_shorthand_and_inline_tables() {
        let manifest = "\
[package]
name = \"mt-md\"
version.workspace = true

[dependencies]
mt-doc.workspace = true
serde_json = { version = \"1\", features = [\"std\"] }
# a comment
regex = \"1\"

[dev-dependencies]
mt-inline.workspace = true

[target.'cfg(windows)'.dependencies]
windows-sys = \"0.5\"
";
        let deps = declared_dependencies(manifest);
        assert_eq!(
            deps,
            ["mt-doc", "serde_json", "regex", "mt-inline", "windows-sys"]
        );
        // `version.workspace = true` is in [package], not a dependency table.
        assert!(!deps.contains(&"version".to_string()));
    }

    #[test]
    fn a_forbidden_edge_is_detected() {
        let manifest = "[dependencies]\nmt-ui.workspace = true\n";
        assert!(declared_dependencies(manifest).contains(&"mt-ui".to_string()));
    }

    /// The invariant itself, checked against the real workspace.
    #[test]
    fn the_workspace_satisfies_the_section_1_constraints() {
        let violations = check(&crate::repo_root()).expect("check");
        assert!(violations.is_empty(), "\n{}", violations.join("\n"));
    }
}

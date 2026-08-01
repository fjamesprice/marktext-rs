//! # `mt-cli` — command line entry point
//!
//! ## Contract
//!
//! argv handling and headless conversion. Two audiences: users doing
//! `mt-cli --to-html notes.md`, and CI running the differential harness.
//!
//! ## `--dump-state` and the differential harness (§11.2)
//!
//! `--dump-state` is the Rust half of the highest-value test asset the project
//! has:
//!
//! ```text
//! corpus file ──┬──► node harness → @muyajs/core → state JSON ──┐
//!               │                                                ├──► assert equal
//!               └──► mt-cli --dump-state ────────────────────────┘
//! ```
//!
//! It exists only because §0 keeps the state shape identical between the two
//! engines. It is built in M0 and runs in CI **for the entire project**,
//! because it converts "did I port the lexer correctly?" from a judgement call
//! into a boolean, across the whole corpus, on every commit. Extend it to
//! serialized markdown and exported HTML as those land.
//!
//! ## Dependency constraints
//!
//! `mt-cli` may perform I/O — it is a program, not a library component. It
//! must **not** depend on `mt-ui` or `mt-app`: `--dump-state` has to run on a
//! CI runner with no display, and if the CLI ever needs a window the harness
//! stops working on exactly the platforms it is most needed on.
//!
//! ## M0 status
//!
//! `--dump-state` parses arguments, reads the file, and reports
//! [`ExitCode::UNIMPLEMENTED`] because `mt_md::dump_state` is a stub. The
//! comparison runner reports that as *skipped*, not *failed*. When `mt-md`
//! lands in M1/M2 the exit code becomes 0 and the harness starts enforcing
//! with no change to the harness itself.

use std::path::PathBuf;

/// Process exit codes.
///
/// [`ExitCode::UNIMPLEMENTED`] is distinct from [`ExitCode::ERROR`] on
/// purpose: the differential comparison runner must be able to tell "the Rust
/// engine does not do this yet" (skip, CI stays green) from "the Rust engine
/// tried and failed" (fail the build).
pub struct ExitCode;

impl ExitCode {
    pub const SUCCESS: i32 = 0;
    /// Bad arguments, unreadable file, or a genuine failure.
    pub const ERROR: i32 = 1;
    /// The requested operation is not implemented in this milestone.
    pub const UNIMPLEMENTED: i32 = 3;
}

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print muya-compatible state JSON for a markdown file.
    DumpState {
        path: PathBuf,
        /// Use [`mt_md::Options::SPEC`] instead of
        /// [`mt_md::Options::MUYA_DEFAULT`].
        spec_options: bool,
    },
    Help,
    Version,
}

/// Argument parsing failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgError(pub String);

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ArgError {}

pub const USAGE: &str = "\
mt-cli — MarkText (Rust) command line

USAGE:
    mt-cli --dump-state <FILE> [--spec-options]
    mt-cli --help
    mt-cli --version

OPTIONS:
    --dump-state <FILE>   Print muya-compatible state JSON for FILE to stdout.
                          The Rust half of the differential-test harness
                          (RUST-REWRITE-PLAN.md §11.2).
    --spec-options        Parse with every muya extension disabled, matching
                          the CommonMark/GFM spec runners. Default is muya's
                          own defaults (math and front matter on).
    --help, -h            Print this message.
    --version, -V         Print the version.

EXIT CODES:
    0   success
    1   error
    3   not implemented in this milestone
";

/// Parse argv (excluding the program name).
///
/// Hand-rolled rather than `clap`: the M0 surface is three flags, and the
/// dependency tables in §8/§12 are the plan of record — third-party deps land
/// with the milestone that needs them. Swap in `clap` when the real command
/// surface (§10: convert, export, open) arrives.
pub fn parse_args<I, S>(args: I) -> Result<Command, ArgError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|s| s.as_ref().to_owned()).collect();

    if args.is_empty() {
        return Ok(Command::Help);
    }

    let mut path: Option<PathBuf> = None;
    let mut spec_options = false;
    let mut dump_state = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            "--spec-options" => spec_options = true,
            "--dump-state" => {
                dump_state = true;
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err(ArgError("--dump-state requires a FILE argument".into()));
                };
                path = Some(PathBuf::from(value));
            }
            other => return Err(ArgError(format!("unrecognised argument: {other}"))),
        }
        i += 1;
    }

    if !dump_state {
        return Err(ArgError("no command given; try --help".into()));
    }

    match path {
        Some(path) => Ok(Command::DumpState { path, spec_options }),
        None => Err(ArgError("--dump-state requires a FILE argument".into())),
    }
}

/// Run a parsed command, writing to `out`. Returns the process exit code.
pub fn run(command: Command, out: &mut dyn std::io::Write) -> i32 {
    match command {
        Command::Help => {
            let _ = write!(out, "{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            let _ = writeln!(out, "mt-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Command::DumpState { path, spec_options } => {
            let markdown = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("mt-cli: cannot read {}: {e}", path.display());
                    return ExitCode::ERROR;
                }
            };
            let options = if spec_options {
                mt_md::Options::SPEC
            } else {
                mt_md::Options::MUYA_DEFAULT
            };
            match mt_md::dump_state(&markdown, options) {
                Ok(json) => {
                    let _ = writeln!(out, "{json}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    // Machine-readable on stderr so the comparison runner can
                    // classify this as SKIPPED rather than FAILED without
                    // parsing prose. See tools/diff/compare.mjs.
                    eprintln!("mt-cli: unimplemented: mt_md::dump_state: {e}");
                    ExitCode::UNIMPLEMENTED
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_is_help() {
        assert_eq!(parse_args(Vec::<String>::new()), Ok(Command::Help));
    }

    #[test]
    fn parses_dump_state() {
        assert_eq!(
            parse_args(["--dump-state", "a.md"]),
            Ok(Command::DumpState {
                path: PathBuf::from("a.md"),
                spec_options: false,
            })
        );
    }

    #[test]
    fn parses_dump_state_with_spec_options_either_order() {
        let expected = Command::DumpState {
            path: PathBuf::from("a.md"),
            spec_options: true,
        };
        assert_eq!(
            parse_args(["--dump-state", "a.md", "--spec-options"]),
            Ok(expected.clone())
        );
        assert_eq!(
            parse_args(["--spec-options", "--dump-state", "a.md"]),
            Ok(expected)
        );
    }

    #[test]
    fn dump_state_without_file_is_an_error() {
        assert!(parse_args(["--dump-state"]).is_err());
    }

    #[test]
    fn unknown_flag_is_an_error() {
        assert!(parse_args(["--nope"]).is_err());
    }

    /// A file name is not mistaken for a flag, and a flag is not mistaken for
    /// a file name — `--dump-state --help` takes `--help` as the path, which
    /// is wrong but unambiguous; assert the current behaviour so a future
    /// change to it is deliberate.
    #[test]
    fn flag_after_dump_state_is_taken_as_the_path() {
        assert_eq!(
            parse_args(["--dump-state", "--help"]),
            Ok(Command::DumpState {
                path: PathBuf::from("--help"),
                spec_options: false,
            })
        );
    }

    #[test]
    fn help_writes_usage_and_succeeds() {
        let mut out = Vec::new();
        assert_eq!(run(Command::Help, &mut out), ExitCode::SUCCESS);
        assert!(String::from_utf8_lossy(&out).contains("--dump-state"));
    }

    /// The M0 contract the differential harness depends on: `--dump-state` on
    /// a real file exits 3 (UNIMPLEMENTED), not 1 (ERROR). The comparison
    /// runner keys off that distinction to report SKIPPED instead of failing
    /// CI. When `mt-md` lands this becomes 0 and the harness starts enforcing.
    #[test]
    fn dump_state_exits_unimplemented_at_m0() {
        let dir = std::env::temp_dir().join("mt-cli-test-dump-state");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.md");
        std::fs::write(&path, "# hello\n").unwrap();

        let mut out = Vec::new();
        let code = run(
            Command::DumpState {
                path: path.clone(),
                spec_options: false,
            },
            &mut out,
        );
        assert_eq!(code, ExitCode::UNIMPLEMENTED);
        assert!(
            out.is_empty(),
            "nothing should be written to stdout when unimplemented"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dump_state_on_a_missing_file_is_an_error_not_unimplemented() {
        let mut out = Vec::new();
        let code = run(
            Command::DumpState {
                path: PathBuf::from("definitely-not-a-real-file-9f3a.md"),
                spec_options: false,
            },
            &mut out,
        );
        assert_eq!(code, ExitCode::ERROR);
    }
}

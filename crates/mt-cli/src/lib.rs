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
//! ## `--to-html`, added at M2 S5
//!
//! The command line onto `mt_md::render_to_static_html`, which is also what
//! §11.1's conformance ratchet calls. It is the second user of
//! [`ExitCode::UNIMPLEMENTED`] that S3's comment predicted — and the
//! prediction was half right, because S5 opened that entry point too, so the
//! code is inherited as a *contract* rather than as a behaviour. See
//! [`Command::ToHtml`].
//!
//! ## M0 status, and what became of it
//!
//! `--dump-state` parsed arguments, read the file, and reported
//! [`ExitCode::UNIMPLEMENTED`] because `mt_md::dump_state` was a stub; the
//! comparison runner reported that as *skipped*, not *failed*. **M2 S3 turned
//! that into exit 0** and `cargo xtask diff` started enforcing with no change
//! to the harness — which is the M0 design working as intended, and is why the
//! exit code is still defined here rather than deleted with its last user.

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
    /// Render a markdown file to static HTML — `mt_md::render_to_static_html`.
    ///
    /// **Added at M2 S5**, and it is the second user of
    /// [`ExitCode::UNIMPLEMENTED`] the code comment above predicted. The
    /// prediction was half right: the exit code is still defined, still
    /// documented and still handled by `cargo xtask diff`, and `--to-html`
    /// does **not** produce it either, because S5 opened the last of D6's
    /// three entry points. What it inherits instead is the *contract* — the
    /// signature `mt_md::render_to_static_html` keeps its `Result` for is
    /// exactly this caller, and a milestone that reopens it (a renderer that
    /// declines a document `mt-export` hands it) has an exit code to say so
    /// with rather than one to invent.
    ToHtml {
        path: PathBuf,
        /// Use [`mt_md::Options::SPEC`] instead of
        /// [`mt_md::Options::MUYA_DEFAULT`].
        spec_options: bool,
        /// Run the output through `ammonia`. **Off by default**, matching the
        /// spec runners rather than matching muya: `renderToStaticHTML`'s own
        /// default is to sanitize, and the inversion here is deliberate
        /// because a *command line* rendering a file the user owns is the
        /// conformance-runner case rather than the untrusted-paste case, and
        /// because a flag that is on by default cannot be discovered from
        /// `--help` by someone comparing output. `mt_md`'s own default is
        /// unchanged, which is where the safety argument belongs.
        sanitize: bool,
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
    mt-cli --to-html <FILE> [--spec-options] [--sanitize]
    mt-cli --help
    mt-cli --version

OPTIONS:
    --dump-state <FILE>   Print muya-compatible state JSON for FILE to stdout.
                          The Rust half of the differential-test harness
                          (RUST-REWRITE-PLAN.md §11.2).
    --to-html <FILE>      Print static HTML for FILE to stdout. The command
                          line onto mt_md::render_to_static_html (§11.1's
                          conformance ratchet calls the same function).
    --sanitize            Run --to-html's output through ammonia. Off by
                          default, matching the spec runners; mt_md's own
                          default is the other way round.
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
    let mut sanitize = false;
    let mut verb: Option<&'static str> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            "--spec-options" => spec_options = true,
            "--sanitize" => sanitize = true,
            verb_arg @ ("--dump-state" | "--to-html") => {
                // Two verbs rather than one, from S5. They are mutually
                // exclusive because the alternative is deciding which wins,
                // and a command line whose behaviour depends on flag order is
                // one the differential harness could drive wrongly in silence.
                if let Some(first) = verb {
                    return Err(ArgError(format!(
                        "{first} and {verb_arg} are mutually exclusive"
                    )));
                }
                verb = Some(if verb_arg == "--dump-state" {
                    "--dump-state"
                } else {
                    "--to-html"
                });
                i += 1;
                let Some(value) = args.get(i) else {
                    return Err(ArgError(format!("{verb_arg} requires a FILE argument")));
                };
                path = Some(PathBuf::from(value));
            }
            other => return Err(ArgError(format!("unrecognised argument: {other}"))),
        }
        i += 1;
    }

    let (Some(verb), Some(path)) = (verb, path) else {
        return Err(ArgError("no command given; try --help".into()));
    };
    Ok(match verb {
        "--to-html" => Command::ToHtml {
            path,
            spec_options,
            sanitize,
        },
        _ => Command::DumpState { path, spec_options },
    })
}

/// A file's bytes as **muya's file layer** would hand them to the parser.
///
/// Two normalisations, and the reason they live here rather than in `mt_md` is
/// the whole of what makes them right:
///
/// - **Strip a UTF-8 BOM.** muya's file layer strips it; `marked` does not, so
///   a BOM handed straight to `MarkdownToState.generate()` becomes the first
///   character of the first paragraph. Measured, not assumed.
/// - **Normalise `\r\n` and a lone `\r` to `\n`.** `Lexer.lex` does exactly
///   this (`other.carriageReturn` = `/\r\n|\r/g`) — but **`lexBlock` does not
///   call `lex`**. It calls `new m.Lexer(m.defaults).blockTokens(src)`
///   directly, skipping the preprocessing step, so muya's parser sees carriage
///   returns raw: `"a\r\nb\r\n"` generates one paragraph whose text is
///   `"a\r\nb\r"`. The normalisation is therefore the *file layer's*, and
///   putting it in `mt_md::parse` would make the port disagree with the
///   reference on any string a caller hands it directly.
///
/// # Found by D6's first ratchet, on its first run
///
/// This is a straight transcription of `tools/diff/dump-ts-state.mjs`'s
/// `readMarkdown`, whose M0 doc comment already said *"The Rust side must do
/// the same, and `mt-fs` owns that in the real application"* — and nothing had
/// ever checked that it did. `cargo xtask blocks` could not see it because
/// `blocks.rs` normalises on its own side before handing one string to both
/// engines; `cargo xtask diff` hands each side a **path**, so the first `Ok`
/// from `mt_md::dump_state` turned 11 of the 22 corpus files red on a `\r`.
/// M2.md §0's rule again, in a new place: a check that has never executed is
/// not a check.
///
/// `mt-fs` inherits this when it exists; the doc comment moves with it.
fn read_markdown(text: &str) -> String {
    // **One rule, one implementation, from M2 S4.** S3 wrote this here because
    // `mt-cli` was the only caller; S4 needs it a second time, in
    // `mt-md`'s round-trip gate, which reads the same files and would
    // otherwise be a fourth spelling of a rule that already has three
    // (`dump-ts-state.mjs`'s `readMarkdown`, this, and `blocks.rs`'s
    // pre-normalisation). A second spelling is how S3's first `cargo xtask
    // diff` run came out 11 of 22 red on a `\r`.
    //
    // It lives in `mt-md` rather than here because that is the crate both
    // callers can reach, and it is a **pure string function** — the I/O is
    // still `std::fs::read_to_string` above, which is still `mt-cli`'s. Its
    // own docs say why `mt_md::parse` must not call it.
    mt_md::normalize_source(text)
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
                Ok(s) => read_markdown(&s),
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
            // Infallible from M2 S3: `mt_md::dump_state` is `parse` plus the
            // state serializer, and every string is a document. The
            // `UNIMPLEMENTED` exit code stays defined and stays handled by
            // `cargo xtask diff` — S5's `--to-html` is its next user.
            let _ = writeln!(out, "{}", mt_md::dump_state(&markdown, options));
            ExitCode::SUCCESS
        }
        Command::ToHtml {
            path,
            spec_options,
            sanitize,
        } => {
            let markdown = match std::fs::read_to_string(&path) {
                Ok(s) => read_markdown(&s),
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
            // The `Err` arm no input reaches at M2, handled rather than
            // unwrapped: `mt_md::render_to_static_html` keeps its `Result` for
            // this caller, and `ExitCode::UNIMPLEMENTED` is what the contract
            // means on a process boundary. Unwrapping here would turn a future
            // "this renderer declines" into a panic and delete the exit code's
            // only user.
            match mt_md::render_to_static_html(&markdown, options, sanitize) {
                Ok(html) => {
                    let _ = writeln!(out, "{html}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("mt-cli: {e}");
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
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("--dump-state"));
        assert!(text.contains("--to-html"));
    }

    /// **Replaces `dump_state_exits_unimplemented_at_m0`, deleted at M2 S3.**
    ///
    /// That test asserted the M0 contract the differential harness was built
    /// on: `--dump-state` on a real file exits 3 (UNIMPLEMENTED) rather than 1
    /// (ERROR), so the runner reports SKIPPED instead of failing CI. S3 is the
    /// stage its own doc comment named — "when `mt-md` lands this becomes 0 and
    /// the harness starts enforcing" — so the assertion is replaced by the one
    /// it predicted rather than left inverted.
    #[test]
    fn dump_state_prints_state_json_and_succeeds() {
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
        assert_eq!(code, ExitCode::SUCCESS);

        let state: serde_json::Value = serde_json::from_slice(&out).expect("valid state JSON");
        assert_eq!(state[0]["name"], "atx-heading");
        assert_eq!(state[0]["text"], "# hello");

        let _ = std::fs::remove_file(&path);
    }

    /// [`read_markdown`] is `dump-ts-state.mjs`'s `readMarkdown`, and the two
    /// halves are separately checkable.
    #[test]
    fn a_file_is_read_the_way_muyas_file_layer_reads_one() {
        assert_eq!(read_markdown("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(read_markdown("a\rb\r"), "a\nb\n");
        assert_eq!(read_markdown("\u{feff}# h\n"), "# h\n");
        assert_eq!(read_markdown("\u{feff}a\r\nb"), "a\nb");
        // A BOM only counts at the start, and a document with neither is
        // untouched — the fast path.
        assert_eq!(read_markdown("a\u{feff}b\n"), "a\u{feff}b\n");
        assert_eq!(read_markdown("plain\n"), "plain\n");
    }

    /// End to end, because the reason this exists is that the two engines were
    /// reading the same file differently and nothing said so. **The port's
    /// parser deliberately does not normalise** — `lexBlock` skips `Lexer.lex`,
    /// so muya's does not either — which is what makes this the file layer's
    /// job and this test the one that would catch it moving.
    #[test]
    fn dump_state_normalises_line_endings_before_the_parser_sees_them() {
        let dir = std::env::temp_dir().join("mt-cli-test-crlf");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("crlf.md");
        std::fs::write(&path, "\u{feff}a\r\nb\r\n\r\nc\r\n").unwrap();

        let mut out = Vec::new();
        assert_eq!(
            run(
                Command::DumpState {
                    path: path.clone(),
                    spec_options: true,
                },
                &mut out,
            ),
            ExitCode::SUCCESS
        );
        let state: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(state[0]["text"], "a\nb");
        assert_eq!(state[1]["text"], "c");

        // And the parser itself is untouched by that: handed the raw bytes it
        // reproduces muya's own answer, carriage returns and all —
        // `"a\r\nb\r\n"` is **one** paragraph whose text keeps them.
        let raw = mt_md::parse("a\r\nb\r\n", mt_md::Options::SPEC).document;
        let tops = raw.children(raw.root());
        assert_eq!(tops.len(), 1);
        assert_eq!(
            raw.block(tops[0])
                .and_then(mt_doc::Block::text)
                .map(|t| t.to_str().into_owned()),
            Some("a\r\nb\r".to_string())
        );

        let _ = std::fs::remove_file(&path);
    }

    /// `--to-html`'s argument surface, and the one rule it adds: the two verbs
    /// are mutually exclusive, so a command line's behaviour never depends on
    /// flag order.
    #[test]
    fn parses_to_html_and_refuses_two_verbs() {
        assert_eq!(
            parse_args(["--to-html", "a.md"]),
            Ok(Command::ToHtml {
                path: PathBuf::from("a.md"),
                spec_options: false,
                sanitize: false,
            })
        );
        assert_eq!(
            parse_args(["--to-html", "a.md", "--spec-options", "--sanitize"]),
            Ok(Command::ToHtml {
                path: PathBuf::from("a.md"),
                spec_options: true,
                sanitize: true,
            })
        );
        assert!(parse_args(["--to-html"]).is_err());
        assert!(parse_args(["--dump-state", "a.md", "--to-html", "b.md"]).is_err());
        assert!(parse_args(["--to-html", "a.md", "--dump-state", "b.md"]).is_err());
    }

    /// D6's third ratchet at the process boundary: `--to-html` exits 0 and
    /// prints HTML, where before S5 the same command did not exist and the
    /// function behind it exited 3.
    #[test]
    fn to_html_prints_html_and_succeeds() {
        let dir = std::env::temp_dir().join(format!("mt-cli-to-html-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("a.md");
        std::fs::write(
            &path,
            "# hi

<script>x</script>
",
        )
        .expect("write");

        let mut out = Vec::new();
        let code = run(
            Command::ToHtml {
                path: path.clone(),
                spec_options: true,
                sanitize: false,
            },
            &mut out,
        );
        assert_eq!(code, ExitCode::SUCCESS);
        let html = String::from_utf8_lossy(&out).into_owned();
        assert!(html.contains("<h1>hi</h1>"), "{html}");
        assert!(html.contains("<script>"), "unsanitized by default: {html}");

        let mut out = Vec::new();
        let code = run(
            Command::ToHtml {
                path,
                spec_options: true,
                sanitize: true,
            },
            &mut out,
        );
        assert_eq!(code, ExitCode::SUCCESS);
        let html = String::from_utf8_lossy(&out).into_owned();
        assert!(html.contains("<h1>hi</h1>"), "{html}");
        assert!(!html.contains("<script"), "{html}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn to_html_on_a_missing_file_is_an_error_not_unimplemented() {
        let mut out = Vec::new();
        let code = run(
            Command::ToHtml {
                path: PathBuf::from("no/such/file.md"),
                spec_options: false,
                sanitize: false,
            },
            &mut out,
        );
        assert_eq!(
            code,
            ExitCode::ERROR,
            "an unreadable file is not `not implemented`"
        );
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

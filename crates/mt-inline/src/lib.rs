//! # `mt-inline` — the inline tokenizer
//!
//! ## Contract
//!
//! Given a leaf block's raw markdown text, produce a token tree in which
//! **every token preserves its source bytes and its byte range**. That is the
//! whole contract, and it is stricter than "parse the inlines correctly".
//!
//! This is a faithful port of `packages/muya/src/inlineRenderer/lexer.ts`
//! (898 lines) and `rules.ts` (112 lines). RUST-REWRITE-PLAN.md §3 names it
//! **the single highest-fidelity-risk component in the project**, and it is
//! small enough to port line by line rather than reinterpret. Do not
//! reinterpret it.
//!
//! ## Dependency constraints
//!
//! Per §1, `mt-inline` is **pure**: it sits at the bottom of the dependency
//! graph beside `mt-doc` and depends on neither it nor anything else in the
//! workspace.
//!
//! - **No windowing.** **No GPU.** **No I/O.**
//! - **No dependency on `mt-ui` or `mt-app`,** ever.
//! - Input is `&str`, output is a `Vec<Token>`. Nothing else crosses the
//!   boundary.
//!
//! Being pure is what makes the fuzzing gate in §11.3 and the 24-hour
//! no-panic requirement at the M1 exit gate (§9) achievable.
//!
//! ## The three non-negotiable porting rules (§3)
//!
//! 1. **Ranges are byte offsets, and they must tile the input exactly.**
//!    Assert in debug builds that concatenating every token's `raw` slice
//!    reproduces the source byte-for-byte. This single invariant catches most
//!    porting errors immediately, and it is checked over the whole corpus at
//!    the M1 gate.
//! 2. **`Marker` and backslash spans are retained, not normalised away.**
//!    They are what the renderer reveals when the caret is inside a token,
//!    and what the serializer re-emits. A tokenizer that "cleans up" markers
//!    is a tokenizer that loses user data.
//! 3. **Behaviour is defined by the TypeScript tests, not by the spec.**
//!    `packages/muya/src/inlineRenderer/__tests__/` covers autolink trailing
//!    punctuation, autolink encoding, emoji word boundaries, inline-math
//!    escaping, CJK flanking for `strong`, reference-link/image anchors, and
//!    link-followed-by-autolink. §14 step 5 is explicit: **port every one of
//!    those specs to Rust as failing tests first, then make them pass.**
//!
//! `rules.ts` is regex-driven; use `regex` with pre-compiled `LazyLock<Regex>`.
//! Where a regex is a hot path, replace it with a hand-written scanner **only
//! after** the regex version passes the full suite, and only with the suite as
//! the guard.
//!
//! ## Marker reveal (§3.1)
//!
//! The rule that makes MarkText feel like MarkText: a token's markers reveal
//! when the caret is inside `token.range` (inclusive of edges) or the
//! selection intersects it; ancestors reveal when a descendant reveals;
//! everything else renders decorated. It is a pure function of
//! `(tokens, caret, selection)`, so it is cheap to test headlessly — and it is
//! load-bearing for the product's identity. Get it exactly right early.
//!
//! ## M0 status
//!
//! Stub. No types, no implementation. `mt-inline` is M1 (§9) and is the first
//! thing built after this milestone — deliberately, because the §11.2
//! differential harness scaffolded in M0 is what makes its correctness a
//! boolean rather than a judgement call.

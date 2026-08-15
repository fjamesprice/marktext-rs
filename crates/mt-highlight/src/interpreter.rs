//! A port of Prism's `tokenize` / `matchGrammar` — prism-core.js:893-1058.
//!
//! Written from the reference clone's
//! `node_modules/prismjs/components/prism-core.js` rather than from a
//! description of it, and deliberately transcribed line for line rather than
//! rewritten idiomatically. The structure below is odd Rust — a hand-rolled
//! doubly-linked list in a `Vec`, a loop whose update clause is duplicated at
//! four `continue` sites, an abort guard that compares a node count against a
//! **UTF-16** length — and every one of those oddities is load-bearing. S3's
//! gate is exact span equality against this exact algorithm, so the places where
//! it is surprising are the places a tidier port would diverge.
//!
//! # The four things that are not obvious
//!
//! **1. The list, and why it cannot be a `Vec<Value>`.** Prism tokenizes by
//! repeatedly splitting a linked list of *strings and tokens* in place. Each
//! rule sweeps the list, and a match replaces one node with up to three
//! (`before`, the token, `after`). A `Vec` with index arithmetic would be
//! equivalent right up to the greedy re-match at the bottom of the function,
//! which re-enters `matchGrammar` from a node that the current sweep is holding
//! — and Prism's `removeRange` deliberately leaves the removed nodes' own
//! `next` pointers intact (prism-core.js:1118-1126), so the outer sweep walks
//! off a detached chain and that is *correct* behaviour it depends on. The arena
//! reproduces it exactly by never touching a removed node's links.
//!
//! **2. `lookbehind` is not a regex feature.** `matchPattern`
//! (prism-core.js:890-899) runs the pattern normally and then, if
//! `lookbehind: true` and **group 1** participated and is non-empty, moves the
//! match start forward past it. Prism uses this wherever a rule needs to see a
//! preceding character without consuming it — which is nearly everywhere,
//! because JS regexes had no lookbehind when Prism was written. Note what it is
//! *not*: it is not `(?<=…)`, it does not re-run the match, and a group 1 that
//! matched the empty string trims nothing.
//!
//! **3. `greedy` changes both the haystack and the blast radius.** A greedy
//! pattern is matched against the **whole text** from the current position
//! (prism-core.js:969-970) rather than against one string node, so `^`, `\b` and
//! any lookaround see their real neighbours; and it is allowed to swallow
//! *tokens* other rules already produced (`removeCount`), which is what lets a
//! string literal reclaim a `//` that the comment rule got to first. Swallowing
//! a token is exactly the condition that triggers the re-match at the bottom.
//!
//! **4. Everything here counts in bytes where Prism counts in UTF-16.** That is
//! sound because every position in the algorithm is derived from the same
//! measure and only ever compared against another one — a linked list of
//! substrings is the same list whichever monotone measure of length you use, and
//! byte offsets are what D15 wants out. The **one** place the measure leaks is
//! the abort guard at prism-core.js:955, which compares a node *count* against
//! `text.length`; that one is computed in UTF-16 (see [`utf16_len`]) rather than
//! quietly changed.
//!
//! # What is not here
//!
//! **Hooks.** D14 names them and excludes them: 21 grammars register
//! `before-tokenize` / `after-tokenize` / `wrap` callbacks, they run at tokenize
//! time and so cannot be snapshotted, and *"the corpus does not need any of
//! them — of its 16 languages only `markup` registers a hook, and it is a
//! `wrap`"*, which is HTML presentation with no tokenization content. A language
//! whose grammar depends on a hook must not be added to `PORTED` without
//! implementing one, and the differential is what would catch it.
//!
//! **`rest`.** `Prism.tokenize` merges `grammar.rest` into the grammar and
//! deletes it (prism-core.js:894-901) — a destructive mutation of a shared
//! object, so it happens once no matter how many times tokenize runs. The
//! generator does that merge while snapshotting, so by the time the data reaches
//! this file there is no `rest` left to handle.

use crate::generated::{GRAMMARS, PATTERNS, RULES};
use crate::grammar::compiled;

/// The tree `tokenize` produces: Prism's token stream with **absolute** byte
/// offsets into the original code attached.
///
/// Prism's own `Token` carries no offsets at all — only a `length`
/// (prism-core.js:812) — and `tools/diff/dump-prism-tokens.mjs` has to rebuild
/// positions by accumulating those lengths as it walks. Here they fall out of
/// the algorithm, so they are kept.
pub(crate) enum Item {
    /// Text no rule claimed.
    Text { start: usize, end: usize },
    /// A matched token. `content` is `None` when the pattern had no `inside`,
    /// which is Prism's *"content is a string"* case, and `Some` when it did —
    /// including when the nested tokenization matched nothing and produced a
    /// single string, because Prism's `content` is an array either way and the
    /// flattening rule distinguishes the two.
    Token {
        class: &'static str,
        start: usize,
        end: usize,
        content: Option<Vec<Item>>,
    },
}

// ---------------------------------------------------------------------------
// The linked list — prism-core.js:1076-1140
// ---------------------------------------------------------------------------

/// A node's value: a slice of the text being tokenized, or a token.
///
/// Both carry `start`/`end` **local** to the current `tokenize` call, because
/// that is the coordinate system the algorithm's arithmetic runs in. `base` is
/// added once, at [`List::into_items`].
enum Value {
    Str {
        start: usize,
        end: usize,
    },
    Token {
        class: &'static str,
        start: usize,
        end: usize,
        content: Option<Vec<Item>>,
    },
}

impl Value {
    /// `currentNode.value.length` — for a string its own length, for a token
    /// `Token.length`, i.e. the length of the text it was created from.
    fn len(&self) -> usize {
        match self {
            Value::Str { start, end } | Value::Token { start, end, .. } => end - start,
        }
    }
}

struct Node {
    value: Value,
    prev: usize,
    next: usize,
}

/// The sentinel indices. Prism's `head` and `tail` carry `value: null` and their
/// values are never read; the placeholders here are never read either.
const HEAD: usize = 0;
const TAIL: usize = 1;

struct List {
    nodes: Vec<Node>,
    /// Prism's `list.length`, which counts **live** nodes and is read only by
    /// the abort guard.
    len: usize,
}

impl List {
    fn new(text_len: usize) -> Self {
        let mut nodes = Vec::with_capacity(16);
        nodes.push(Node {
            value: Value::Str { start: 0, end: 0 },
            prev: usize::MAX,
            next: TAIL,
        });
        nodes.push(Node {
            value: Value::Str { start: 0, end: 0 },
            prev: HEAD,
            next: usize::MAX,
        });
        let mut list = List { nodes, len: 0 };
        list.add_after(
            HEAD,
            Value::Str {
                start: 0,
                end: text_len,
            },
        );
        list
    }

    /// prism-core.js:1099-1108. Note what it does **not** do: it never touches
    /// any node other than `node` and `node.next`, so calling it on a node that
    /// `remove_range` has unlinked splices into the detached chain — which is
    /// behaviour the greedy re-match depends on.
    fn add_after(&mut self, node: usize, value: Value) -> usize {
        let next = self.nodes[node].next;
        let new = self.nodes.len();
        self.nodes.push(Node {
            value,
            prev: node,
            next,
        });
        self.nodes[node].next = new;
        self.nodes[next].prev = new;
        self.len += 1;
        new
    }

    /// prism-core.js:1118-1126. Removes `count` nodes **after** `node`; the
    /// removed nodes keep their own `prev`/`next` untouched, deliberately.
    fn remove_range(&mut self, node: usize, count: usize) {
        let mut next = self.nodes[node].next;
        let mut removed = 0;
        while removed < count && next != TAIL {
            next = self.nodes[next].next;
            removed += 1;
        }
        self.nodes[node].next = next;
        self.nodes[next].prev = node;
        self.len -= removed;
    }

    fn next(&self, node: usize) -> usize {
        self.nodes[node].next
    }

    fn prev(&self, node: usize) -> usize {
        self.nodes[node].prev
    }

    fn value_len(&self, node: usize) -> usize {
        debug_assert!(node != TAIL && node != HEAD, "sentinels carry no value");
        self.nodes[node].value.len()
    }

    fn is_token(&self, node: usize) -> bool {
        matches!(self.nodes[node].value, Value::Token { .. })
    }

    /// `toArray` (prism-core.js:1133-1140), with `base` folded into every
    /// offset so that what comes out is absolute.
    fn into_items(mut self, base: usize) -> Vec<Item> {
        let mut items = Vec::new();
        let mut node = self.nodes[HEAD].next;
        while node != TAIL {
            let next = self.nodes[node].next;
            // Replaced rather than cloned: `content` is a `Vec` and this walk
            // visits every live node exactly once.
            let value =
                std::mem::replace(&mut self.nodes[node].value, Value::Str { start: 0, end: 0 });
            items.push(match value {
                Value::Str { start, end } => Item::Text {
                    start: base + start,
                    end: base + end,
                },
                Value::Token {
                    class,
                    start,
                    end,
                    content,
                } => Item::Token {
                    class,
                    start: base + start,
                    end: base + end,
                    content,
                },
            });
            node = next;
        }
        items
    }
}

// ---------------------------------------------------------------------------
// tokenize — prism-core.js:893-909
// ---------------------------------------------------------------------------

/// Tokenize `text` with `grammar`, reporting offsets shifted by `base`.
///
/// `base` is what makes an `inside` grammar's spans absolute: the recursive call
/// at prism-core.js:1034 tokenizes only the matched substring, and its results
/// have to land in the original code's coordinate system.
pub(crate) fn tokenize(text: &str, base: usize, grammar: usize) -> Vec<Item> {
    let mut list = List::new(text.len());
    let context = Context {
        text,
        base,
        // Only ever compared against a node count, and only in the abort guard.
        // ASCII is the overwhelmingly common case for code and skips the walk.
        utf16_len: utf16_len(text),
    };
    match_grammar(&context, &mut list, grammar, HEAD, 0, None);
    list.into_items(base)
}

/// The three values every level of `match_grammar` needs and none of them
/// changes: threading them as one struct keeps the recursive call at the bottom
/// legible.
struct Context<'a> {
    text: &'a str,
    base: usize,
    utf16_len: usize,
}

/// `text.length` as JavaScript would report it.
///
/// Used **only** by the abort guard, which is the one comparison in the
/// algorithm between a node count and a text length. `is_ascii` is a SIMD scan
/// and answers for every fence in the corpus, so the `chars()` walk is a
/// correctness backstop rather than a cost.
fn utf16_len(text: &str) -> usize {
    if text.is_ascii() {
        text.len()
    } else {
        text.chars().map(char::len_utf16).sum()
    }
}

/// prism-core.js:908-914's `RematchOptions`.
///
/// `cause` is `token + ',' + j` in the reference — a rule *name* and a pattern
/// index. Rule names are unique within a grammar (they are object keys) and the
/// recursion is always against the same grammar, so the rule's index is the same
/// key with less allocation.
struct Rematch {
    cause: (usize, usize),
    reach: usize,
}

/// prism-core.js:916-1060, transcribed.
fn match_grammar(
    context: &Context<'_>,
    list: &mut List,
    grammar: usize,
    start_node: usize,
    start_pos: usize,
    mut rematch: Option<&mut Rematch>,
) {
    let text = context.text;
    let g = &GRAMMARS[grammar];

    for rule_index in 0..g.count {
        let rule = &RULES[g.first + rule_index];
        for j in 0..rule.count {
            // prism-core.js:926 — the guard that stops the re-match from
            // re-running the very pattern that caused it, and everything after
            // it. `return`, not `break`: the whole remaining grammar is skipped.
            if let Some(r) = rematch.as_deref() {
                if r.cause == (rule_index, j) {
                    return;
                }
            }

            let pattern = &PATTERNS[rule.first + j];
            let regex = compiled(rule.first + j);

            // prism-core.js:944-948's `for` header. The update clause —
            // `pos += currentNode.value.length, currentNode = currentNode.next`
            // — is spelled out at each `continue` below rather than being
            // factored out, because two of the four sites reach it having
            // already moved `currentNode`.
            let mut current = list.next(start_node);
            let mut pos = start_pos;
            while current != TAIL {
                // :951
                if let Some(r) = rematch.as_deref() {
                    if pos >= r.reach {
                        break;
                    }
                }
                // :957 — *"Something went terribly wrong, ABORT, ABORT!"*. Kept
                // because a port that dropped it would hang where Prism gives
                // up, and because it is the one comparison that is not
                // measure-agnostic.
                if list.len > context.utf16_len {
                    return;
                }
                // :961 — a token is never re-matched by a non-greedy pattern.
                if list.is_token(current) {
                    pos += list.value_len(current);
                    current = list.next(current);
                    continue;
                }

                let mut remove_count = 1usize;
                let str_start;
                let str_end;
                // Both relative to `[str_start, str_end)`.
                let from;
                let match_len;

                if pattern.greedy {
                    // :969-970 — matched against the **whole text** from `pos`.
                    // `captures_from_pos` is `lastIndex = pos; exec(text)`: the
                    // match must start at or after `pos`, `^` still anchors to
                    // 0, and lookaround and `\b` see the real neighbours.
                    let Some((match_start, match_end)) =
                        match_from(regex, text, pos, pattern.lookbehind)
                    else {
                        break;
                    };
                    // :971 — a match starting at the very end cannot be used.
                    if match_start >= text.len() {
                        break;
                    }

                    let to = match_end;
                    // :978-985 — walk to the node the match starts in.
                    let mut p = pos + list.value_len(current);
                    while match_start >= p {
                        current = list.next(current);
                        debug_assert!(current != TAIL, "the list always covers the text");
                        p += list.value_len(current);
                    }
                    p -= list.value_len(current);
                    pos = p;

                    // :988 — the match starts inside a token, which is invalid.
                    if list.is_token(current) {
                        pos += list.value_len(current);
                        current = list.next(current);
                        continue;
                    }

                    // :992-1000 — every node this match covers, **plus every
                    // trailing string node** even past the match's end. That
                    // second clause is not a typo in the reference: it merges
                    // adjacent strings back into one node, and `after` below
                    // puts the surplus back.
                    let mut k = current;
                    while k != TAIL && (p < to || !list.is_token(k)) {
                        remove_count += 1;
                        p += list.value_len(k);
                        k = list.next(k);
                    }
                    remove_count -= 1;

                    // :1003-1004
                    str_start = pos;
                    str_end = p;
                    from = match_start - pos;
                    match_len = match_end - match_start;
                } else {
                    // :1009 — matched against **this node's string only**, from
                    // its start, so `^` anchors to the node and not to the text.
                    str_start = pos;
                    str_end = pos + list.value_len(current);
                    let Some((ms, me)) =
                        match_at_start(regex, &text[str_start..str_end], pattern.lookbehind)
                    else {
                        pos += list.value_len(current);
                        current = list.next(current);
                        continue;
                    };
                    from = ms;
                    match_len = me - ms;
                }

                // :1015-1041, common to both branches.
                let str_len = str_end - str_start;
                let after_start = from + match_len;
                let reach = pos + str_len;
                if let Some(r) = rematch.as_deref_mut() {
                    if reach > r.reach {
                        r.reach = reach;
                    }
                }

                let mut remove_from = list.prev(current);
                if from > 0 {
                    remove_from = list.add_after(
                        remove_from,
                        Value::Str {
                            start: str_start,
                            end: str_start + from,
                        },
                    );
                    pos += from;
                }
                list.remove_range(remove_from, remove_count);

                let token_start = str_start + from;
                let token_end = token_start + match_len;
                let content = pattern.inside.map(|inside| {
                    tokenize(
                        &text[token_start..token_end],
                        context.base + token_start,
                        inside as usize,
                    )
                });
                current = list.add_after(
                    remove_from,
                    Value::Token {
                        class: pattern.class,
                        start: token_start,
                        end: token_end,
                        content,
                    },
                );
                if after_start < str_len {
                    list.add_after(
                        current,
                        Value::Str {
                            start: str_start + after_start,
                            end: str_end,
                        },
                    );
                }

                // :1042-1057 — at least one token was swallowed, so the rules
                // that produced it have to be given another go over the region
                // this match created. Only a greedy pattern can get here.
                if remove_count > 1 {
                    let mut nested = Rematch {
                        cause: (rule_index, j),
                        reach,
                    };
                    let from_node = list.prev(current);
                    match_grammar(context, list, grammar, from_node, pos, Some(&mut nested));
                    if let Some(r) = rematch.as_deref_mut() {
                        if nested.reach > r.reach {
                            r.reach = nested.reach;
                        }
                    }
                }

                pos += list.value_len(current);
                current = list.next(current);
            }
        }
    }
}

/// `matchPattern(pattern, pos, text, lookbehind)` — prism-core.js:890-899, the
/// greedy call site's version.
///
/// Returns the match's byte range with Prism's lookbehind trim already applied.
fn match_from(
    regex: &fancy_regex::Regex,
    text: &str,
    pos: usize,
    lookbehind: bool,
) -> Option<(usize, usize)> {
    // A backtracking blow-up is a defect in a *generated* pattern, not a
    // condition a caller can do anything about, so an error is treated as "no
    // match" here rather than propagated: one pathological rule degrades that
    // rule's highlighting instead of failing the fence.
    let captures = regex.captures_from_pos(text, pos).ok().flatten()?;
    Some(trim_lookbehind(&captures, lookbehind))
}

/// The non-greedy call site's version: always from offset 0 of `text`.
fn match_at_start(
    regex: &fancy_regex::Regex,
    text: &str,
    lookbehind: bool,
) -> Option<(usize, usize)> {
    let captures = regex.captures(text).ok().flatten()?;
    Some(trim_lookbehind(&captures, lookbehind))
}

/// prism-core.js:893-898.
///
/// *"if `match && lookbehind && match[1]`"* — three conditions, and the third is
/// JavaScript truthiness on a capture: a group that did not participate is
/// `undefined` and a group that matched the empty string is `''`. Both are
/// falsy, and both mean *trim nothing*.
fn trim_lookbehind(captures: &fancy_regex::Captures<'_, str>, lookbehind: bool) -> (usize, usize) {
    let whole = captures.get(0).expect("group 0 always participates");
    if !lookbehind {
        return (whole.start(), whole.end());
    }
    match captures.get(1) {
        Some(one) if one.end() > one.start() => {
            (whole.start() + (one.end() - one.start()), whole.end())
        }
        _ => (whole.start(), whole.end()),
    }
}

// ---------------------------------------------------------------------------
// Flattening — the rule `tools/diff/dump-prism-tokens.mjs` chose
// ---------------------------------------------------------------------------

/// Flatten the token tree into the ascending, non-overlapping run list D15
/// requires.
///
/// **The rule is the dumper's, and it has to be, because the two are compared
/// for exact equality.** From its header: *innermost wins, and an outer token
/// fills its own gaps*.
///
/// - a token whose content is a string emits one span with its own class;
/// - a token whose content is nested recurses, and any *text* inside it that no
///   deeper token covers emits a span with the **enclosing** token's class;
/// - text at the top level, enclosed by nothing, emits **no span at all** —
///   that is the block default, which D15 says must never be pushed as a run.
///
/// That is what a browser draws: `Prism.highlightElement` produces nested
/// `<span>`s, and an outer class shows through wherever an inner one does not
/// cover it. It is deliberately *not* `utils/prism/walkToken.ts`'s rule, which
/// visits only leaves and drops the enclosing class — right for deleting a
/// character, wrong for colour.
///
/// **Adjacent spans that resolve to the same class are not merged.** Merging is
/// a normalisation, and a port that split one keyword into two tokens would have
/// a real grammar bug that a merged wire form would hide behind an identical
/// rendering.
pub(crate) fn flatten(items: &[Item], enclosing: Option<&'static str>, out: &mut Vec<crate::Span>) {
    for item in items {
        match item {
            Item::Text { start, end } => {
                if let Some(class) = enclosing {
                    if end > start {
                        out.push(crate::Span {
                            start: *start,
                            end: *end,
                            class,
                        });
                    }
                }
            }
            Item::Token {
                class,
                start,
                end,
                content: None,
            } => {
                if end > start {
                    out.push(crate::Span {
                        start: *start,
                        end: *end,
                        class,
                    });
                }
            }
            Item::Token {
                class,
                content: Some(nested),
                ..
            } => flatten(nested, Some(class), out),
        }
    }
}

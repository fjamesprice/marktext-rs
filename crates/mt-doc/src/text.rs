//! Per-block text storage (RUST-REWRITE-PLAN.md §2.1).

/// The byte length at which [`Text::Inline`] is promoted to [`Text::Rope`].
///
/// §2.1 writes 8 KiB and M2.md §5 D8 keeps it. **The demotion direction is
/// deliberately not implemented**: a block that was once large stays a rope,
/// because oscillating across the threshold on every keystroke is worse than
/// carrying a few ropes that no longer need to be ropes. Recorded on the type
/// rather than left implicit; M2.md §9 says where to revisit it.
pub const ROPE_THRESHOLD: usize = 8 * 1024;

/// A leaf block's text.
///
/// Per-block storage keeps edits O(block) rather than O(document): a 5 MB file
/// is thousands of small independent buffers, not one enormous one. The rope
/// variant exists solely so that a single enormous code fence — the one case
/// where a "block" can be megabytes — does not degrade to O(n) inserts.
///
/// The threshold is 8 KiB; the overwhelming majority of blocks never leave
/// [`Text::Inline`].
///
/// # Offsets are UTF-8 bytes
///
/// Every offset in this type is a byte offset, matching M1's D1 for
/// `mt-inline`. `ropey` counts in `char`s, so [`Text::splice`] converts at the
/// boundary and nowhere else — the conversion is an implementation detail of
/// the rope variant, not part of the contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    /// < 8 KiB. The common case.
    Inline(String),
    /// Promoted on threshold.
    Rope(Rope),
}

impl Text {
    /// An empty text, in the [`Text::Inline`] representation.
    pub fn new() -> Self {
        Text::Inline(String::new())
    }

    /// Byte length of the text.
    pub fn len(&self) -> usize {
        match self {
            Text::Inline(s) => s.len(),
            Text::Rope(r) => r.len(),
        }
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Splice `insert` in at byte offset `at`, removing `remove` bytes first.
    ///
    /// Promotes [`Text::Inline`] to [`Text::Rope`] if the result crosses the
    /// 8 KiB threshold. Offsets are byte offsets and must land on `char`
    /// boundaries — the caller (`mt-layout`, via parley's cluster API) is
    /// responsible for never producing an offset that splits a grapheme.
    ///
    /// # Panics
    ///
    /// If `at` or `at + remove` is out of bounds or does not lie on a `char`
    /// boundary. That is a caller bug in every case: `Edit::SpliceText`'s
    /// offsets come from the layout engine's cluster API, which cannot produce
    /// one, and from an inverse this crate itself generated.
    pub fn splice(&mut self, at: usize, remove: usize, insert: &str) {
        let end = at
            .checked_add(remove)
            .expect("splice range overflows usize");
        assert!(
            end <= self.len(),
            "splice range {at}..{end} is out of bounds for a text of {} bytes",
            self.len()
        );

        match self {
            Text::Inline(s) => {
                assert!(
                    s.is_char_boundary(at) && s.is_char_boundary(end),
                    "splice range {at}..{end} does not lie on char boundaries"
                );
                s.replace_range(at..end, insert);
                // Promotion is checked after the edit, on the resulting
                // length, so a block that shrinks back below the threshold in
                // the same splice is never promoted.
                if s.len() >= ROPE_THRESHOLD {
                    *self = Text::Rope(Rope::from_str(s));
                }
            }
            Text::Rope(r) => r.splice(at, end, insert),
        }
    }

    /// The text as a contiguous `&str`, materialising the rope if needed.
    pub fn to_str(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Text::Inline(s) => std::borrow::Cow::Borrowed(s.as_str()),
            Text::Rope(r) => std::borrow::Cow::Owned(r.to_string()),
        }
    }
}

impl Default for Text {
    fn default() -> Self {
        Text::new()
    }
}

impl From<&str> for Text {
    /// Builds the representation the length calls for, so that constructing a
    /// large block directly — which is what `mt-md`'s parser does, rather than
    /// splicing a block into existence a byte at a time — lands in the rope
    /// variant without a second pass.
    fn from(s: &str) -> Self {
        if s.len() >= ROPE_THRESHOLD {
            Text::Rope(Rope::from_str(s))
        } else {
            Text::Inline(s.to_string())
        }
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        if s.len() >= ROPE_THRESHOLD {
            Text::Rope(Rope::from_str(&s))
        } else {
            Text::Inline(s)
        }
    }
}

/// `ropey::Rope`, wrapped.
///
/// **M2 S0, per M2.md §5 D8.** M0 wrote this as a newtype over `String` so
/// that adopting `ropey` at M2 would be a body swap with [`Text`]'s enum
/// untouched. That is what happened: the enum above is unchanged and the
/// backing store below is now a real rope.
///
/// It stays a newtype rather than becoming `pub type Rope = ropey::Rope`
/// because a re-export would put `ropey` in `mt-doc`'s public API, and every
/// downstream crate would then be pinned to this crate's `ropey` version for a
/// type they only ever reach through [`Text`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rope {
    inner: ropey::Rope,
}

impl Rope {
    /// A rope holding `s`.
    ///
    /// Not `FromStr`: that trait is fallible and this cannot fail, so the
    /// signature would be `Result<Self, Infallible>` for no reason. Named
    /// `from_str` anyway because it is what `ropey`'s own constructor is
    /// called, and this type is a thin wrapper over it.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Rope {
            inner: ropey::Rope::from_str(s),
        }
    }

    /// Byte length.
    pub fn len(&self) -> usize {
        self.inner.len_bytes()
    }

    /// Whether the rope is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Replace the bytes in `start..end` with `insert`.
    ///
    /// The one place in this crate where byte offsets meet `ropey`'s `char`
    /// indices. `byte_to_char` panics on a non-boundary offset, which is the
    /// same contract [`Text::splice`] asserts for the inline variant.
    fn splice(&mut self, start: usize, end: usize, insert: &str) {
        let (start_char, end_char) = (self.inner.byte_to_char(start), self.inner.byte_to_char(end));
        if start_char != end_char {
            self.inner.remove(start_char..end_char);
        }
        if !insert.is_empty() {
            self.inner.insert(start_char, insert);
        }
    }
}

impl std::fmt::Display for Rope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for chunk in self.inner.chunks() {
            f.write_str(chunk)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_text_is_inline_and_empty() {
        let t = Text::new();
        assert!(matches!(t, Text::Inline(_)));
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn splice_replaces_a_byte_range() {
        let mut t = Text::from("hello world");
        t.splice(6, 5, "there");
        assert_eq!(t.to_str(), "hello there");
    }

    #[test]
    fn splice_with_an_empty_insert_is_a_delete() {
        let mut t = Text::from("abcdef");
        t.splice(1, 3, "");
        assert_eq!(t.to_str(), "aef");
    }

    #[test]
    fn splice_at_the_end_appends() {
        let mut t = Text::from("ab");
        let len = t.len();
        t.splice(len, 0, "cd");
        assert_eq!(t.to_str(), "abcd");
    }

    /// Offsets are UTF-8 bytes, not chars and not UTF-16 units — M1's D1,
    /// which this crate inherits. `é` is two bytes, so removing it is
    /// `splice(0, 2, "")` and nothing else.
    #[test]
    fn offsets_are_utf8_bytes() {
        let mut t = Text::from("éx");
        assert_eq!(t.len(), 3);
        t.splice(0, 2, "");
        assert_eq!(t.to_str(), "x");
    }

    #[test]
    #[should_panic(expected = "char boundaries")]
    fn splicing_inside_a_char_panics() {
        let mut t = Text::from("é");
        t.splice(1, 0, "x");
    }

    #[test]
    #[should_panic(expected = "out of bounds")]
    fn splicing_past_the_end_panics() {
        let mut t = Text::from("ab");
        t.splice(1, 5, "");
    }

    /// §2.1's threshold, and the reason [`Text::Rope`] exists: one enormous
    /// code fence must not make every insert O(n).
    #[test]
    fn crossing_the_threshold_promotes_to_a_rope() {
        let mut t = Text::from("x".repeat(ROPE_THRESHOLD - 1));
        assert!(matches!(t, Text::Inline(_)), "still below the threshold");
        t.splice(0, 0, "y");
        assert!(matches!(t, Text::Rope(_)), "promoted at the threshold");
        assert_eq!(t.len(), ROPE_THRESHOLD);
        assert!(t.to_str().starts_with("yx"));
    }

    /// D8: the demotion direction is deliberately absent. A block that was
    /// once large stays a rope. This test is the decision in executable form —
    /// if someone implements demotion, they have to delete it and say why.
    #[test]
    fn a_rope_is_never_demoted_back_to_inline() {
        let mut t = Text::from("x".repeat(ROPE_THRESHOLD));
        assert!(matches!(t, Text::Rope(_)));
        t.splice(0, ROPE_THRESHOLD, "");
        assert!(t.is_empty());
        assert!(
            matches!(t, Text::Rope(_)),
            "M2.md §5 D8 decides against demotion — oscillating at the \
             threshold is worse than carrying a rope that no longer needs to be one"
        );
    }

    /// The rope variant must behave identically to the inline one, or the
    /// 8 KiB threshold becomes a behaviour change rather than a representation
    /// change.
    #[test]
    fn a_rope_splices_the_same_way_an_inline_string_does() {
        let base = "α".repeat(ROPE_THRESHOLD); // 2 bytes each, well over 8 KiB
        let mut rope = Text::from(base.as_str());
        let mut inline = base.clone();
        assert!(matches!(rope, Text::Rope(_)));

        rope.splice(4, 2, "hello");
        inline.replace_range(4..6, "hello");
        assert_eq!(rope.to_str(), inline);

        let end = rope.len();
        rope.splice(end, 0, "!");
        inline.push('!');
        assert_eq!(rope.to_str(), inline);
    }

    /// Constructing directly from a large `&str` lands in the rope variant
    /// without a splice — which is the path `mt-md` will take at S1.
    #[test]
    fn a_large_str_is_born_a_rope() {
        assert!(matches!(
            Text::from("x".repeat(ROPE_THRESHOLD).as_str()),
            Text::Rope(_)
        ));
        assert!(matches!(Text::from("x"), Text::Inline(_)));
    }
}

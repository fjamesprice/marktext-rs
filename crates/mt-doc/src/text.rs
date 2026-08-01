//! Per-block text storage (RUST-REWRITE-PLAN.md §2.1).

/// A leaf block's text.
///
/// Per-block storage keeps edits O(block) rather than O(document): a 5 MB file
/// is thousands of small independent buffers, not one enormous one. The rope
/// variant exists solely so that a single enormous code fence — the one case
/// where a "block" can be megabytes — does not degrade to O(n) inserts.
///
/// The threshold is 8 KiB; the overwhelming majority of blocks never leave
/// [`Text::Inline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Text {
    /// < 8 KiB. The common case.
    Inline(String),
    /// Promoted on threshold.
    Rope(Rope),
}

impl Text {
    /// Byte length of the text.
    pub fn len(&self) -> usize {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Whether the text is empty.
    pub fn is_empty(&self) -> bool {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// Splice `insert` in at byte offset `at`, removing `remove` bytes first.
    ///
    /// Promotes [`Text::Inline`] to [`Text::Rope`] if the result crosses the
    /// 8 KiB threshold. Offsets are byte offsets and must land on `char`
    /// boundaries — the caller (`mt-layout`, via parley's cluster API) is
    /// responsible for never producing an offset that splits a grapheme.
    pub fn splice(&mut self, _at: usize, _remove: usize, _insert: &str) {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }

    /// The text as a contiguous `&str`, materialising the rope if needed.
    pub fn to_str(&self) -> std::borrow::Cow<'_, str> {
        todo!("M2: mt-doc — see RUST-REWRITE-PLAN.md §9")
    }
}

/// Placeholder for `ropey::Rope`.
///
/// **M0 note.** `ropey` is not a dependency yet — the dependency tables in §8
/// and §12 are the plan of record, and deps land with the milestone that needs
/// them. This newtype exists so [`Text`] has its final *shape* now; at M2 the
/// body is replaced with `ropey::Rope` and the enum is untouched.
///
/// It is a newtype rather than a type alias for `String` deliberately: an
/// alias would make `Text::Inline` and `Text::Rope` structurally
/// interchangeable and let code accidentally rely on that.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Rope {
    /// Backing storage until `ropey` lands.
    pub text: String,
}

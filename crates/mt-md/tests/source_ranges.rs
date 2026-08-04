//! The invariants that make [`mt_md::SourceMap`] a check rather than an
//! ornament.
//!
//! M2.md §10's owed item — *"keep the per-node source ranges"* — falls due at
//! S3, because S3 is where `parse`'s signature is written. The risk that comes
//! with taking it is the one this milestone has recorded repeatedly: **an API
//! with no caller until M4 is not a check**, and shipping one is how a
//! mechanism ends up wired, filtered and never fired. So the map is asserted
//! here, over the same 20-odd whole documents `cargo xtask diff` compares, at
//! both option sets.
//!
//! # Why an integration test rather than `src/block/tests.rs`
//!
//! Because it reads files, and §1 says `mt-md` does no I/O — `cargo xtask deps`
//! enforces that over `src/`, and it caught this exact test being written in
//! the wrong place. The two range tests that need `mt_md::block`'s private
//! `Prefix` and `strip_lines` stay in `src/block/tests.rs`; this one needs only
//! the public surface, which is what made moving it free.

use std::ops::Range;
use std::path::{Path, PathBuf};

use mt_doc::{Document, NodeId};
use mt_md::{Options, SourceMap, block::parse_blocks_with_ranges};

/// `cargo xtask diff`'s corpus, minus the two generated megabyte files.
///
/// The same two `cargo xtask blocks` excludes with `CORPUS_SIZE_LIMIT`, and for
/// the same reason: 6 MB adds minutes to a unit test and no structure the
/// smaller documents do not already have. Saying so rather than filtering
/// silently is M2.md's "no silent caps" rule.
fn corpus() -> Vec<(PathBuf, String)> {
    const SIZE_LIMIT: usize = 300 * 1024;

    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
                continue;
            }
            if path.extension().is_none_or(|e| e != "md")
                || path
                    .file_stem()
                    .is_some_and(|s| s.eq_ignore_ascii_case("README"))
            {
                continue;
            }
            if let Ok(source) = std::fs::read_to_string(&path)
                && source.len() <= SIZE_LIMIT
            {
                out.push((path, source));
            }
        }
    }

    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/mt-md is two levels below the repo root")
        .to_path_buf();

    let mut out = Vec::new();
    walk(&root.join("bench").join("corpus"), &mut out);
    walk(
        &root
            .join("spec")
            .join("fixtures")
            .join("marktext-round-trip"),
        &mut out,
    );
    out.sort();
    assert!(
        out.len() >= 18,
        "the corpus has shrunk to {} documents — `cargo xtask diff` compares 22, \
         two of which are over the size limit",
        out.len()
    );
    out
}

/// Three claims, over every corpus document at both option sets.
///
/// 1. **Every live node except the root carries a range**, so a consumer never
///    has to decide what a missing entry means.
/// 2. **A child's range is inside its parent's, and siblings are in document
///    order.** That is what makes the ancestor chain walkable, which is what
///    the container-prefix re-derivation needs
///    (`block::tests::the_stripper_is_re_runnable_from_src_tree_and_ranges`).
/// 3. **Every range is inside the source and on a character boundary**, so
///    `&src[range]` cannot panic on any document in the corpus.
#[test]
fn every_node_has_a_range_and_the_ranges_nest() {
    let mut nodes = 0usize;
    for (path, source) in corpus() {
        for (label, options) in [
            ("SPEC", Options::SPEC),
            ("MUYA_DEFAULT", Options::MUYA_DEFAULT),
        ] {
            let (doc, map) = parse_blocks_with_ranges(&source, options);
            let what = format!("{} at {label}", path.display());
            assert!(
                map.get(doc.root()).is_none(),
                "{what}: the root has no block and must have no range"
            );
            nodes += check(&doc, &map, &source, doc.root(), None, &what);
        }
    }
    assert!(
        nodes > 1_000,
        "only {nodes} nodes were checked — the corpus or the walk has stopped covering anything"
    );
}

fn check(
    doc: &Document,
    map: &SourceMap,
    src: &str,
    id: NodeId,
    parent: Option<&Range<usize>>,
    what: &str,
) -> usize {
    let mut seen = 0;
    let mut previous: Option<Range<usize>> = None;
    for child in doc.children(id) {
        let range = map
            .get(*child)
            .unwrap_or_else(|| panic!("{what}: a live node carries no range"));
        assert!(
            range.start <= range.end && range.end <= src.len(),
            "{what}: {range:?} is not inside 0..{}",
            src.len()
        );
        assert!(
            src.is_char_boundary(range.start) && src.is_char_boundary(range.end),
            "{what}: {range:?} is not on a character boundary"
        );
        if let Some(parent) = parent {
            assert!(
                range.start >= parent.start && range.end <= parent.end,
                "{what}: child {range:?} escapes parent {parent:?}"
            );
        }
        if let Some(previous) = &previous {
            assert!(
                range.start >= previous.start,
                "{what}: sibling {range:?} starts before {previous:?}"
            );
        }
        previous = Some(range.clone());
        seen += 1 + check(doc, map, src, *child, Some(&range), what);
    }
    seen
}

/// And the map is complete for the tree it was handed back with — one entry per
/// node, no more, over the same corpus.
#[test]
fn the_map_has_exactly_one_entry_per_live_node() {
    fn count(doc: &Document, id: NodeId) -> usize {
        doc.children(id)
            .iter()
            .map(|c| 1 + count(doc, *c))
            .sum::<usize>()
    }

    for (path, source) in corpus() {
        let (doc, map) = parse_blocks_with_ranges(&source, Options::MUYA_DEFAULT);
        assert_eq!(
            map.len(),
            count(&doc, doc.root()),
            "{}: the map and the tree disagree about how many nodes there are",
            path.display()
        );
    }
}

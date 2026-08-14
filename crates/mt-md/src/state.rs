//! [`mt_doc::Document`] → muya's `TState[]` JSON.
//!
//! This is one half of the §11.2 differential harness: the shape
//! `MarkdownToState.generate()` returns, emitted from the Rust model so that
//! the two can be compared as JSON. It exists because `mt_doc::Block` maps 1:1
//! onto the TypeScript `TState` union — `block_names_match_typescript_union_one_to_one`
//! is the machine-checked form of that promise, and this module is what cashes
//! it in.
//!
//! # Why it lives in `mt-md` rather than in the harness
//!
//! [`crate::dump_state`] returns muya-compatible state JSON by M0's design, so
//! producing that JSON is this crate's job. Writing it in `xtask` instead would
//! mean the thing S1 and S2 measure and the thing S3 ships are two different
//! serializers that happen to agree today.
//!
//! # The three shape rules that are easy to get wrong
//!
//! 1. **A block with no `meta` object has no `meta` key**, not an empty one.
//!    Seven of the nineteen variants are in that class ([`mt_doc::Block::meta`]
//!    returns `None` for them), and `first_difference` reports a key only one
//!    side has as `present`/`missing` — so getting this wrong is loud, but it
//!    is loud on every block in every document.
//! 2. **`meta.fenceLength` is omitted, not null**, unless the block is fenced
//!    with a fence longer than three. `markdownToState.ts`'s `_buildCodeState`
//!    spreads `...(isFenced && fenceLength && fenceLength > 3 ? { fenceLength } : {})`.
//!    Carried on [`mt_doc::Block::CodeBlock::fence_len`] as `None` for every
//!    other case, so the omission is decided by the parser rather than here.
//! 3. **`MathStyle::Default` serializes to `""`, not to `"default"`** — the
//!    TypeScript type is `'' | 'gitlab'`.
//!
//! Key order does not matter: `first_difference` compares objects as maps, and
//! `serde_json`'s default `Map` is a `BTreeMap`, so this side is sorted for
//! free and the JavaScript side canonicalises explicitly.

use mt_doc::{
    Align, Block, BulletMarker, CodeKind, DiagramLang, Document, FrontmatterLang, FrontmatterStyle,
    MathStyle, NodeId, OrderDelim,
};
use serde_json::{Map, Value, json};

/// The document's top-level blocks as muya's `TState[]`.
///
/// The root itself is not serialized: it has no `TState` counterpart (muya's
/// tree is rooted at `ScrollPage`, whose `blockName` is not a `TState` member
/// either), and what `MarkdownToState.generate()` returns is an *array* of the
/// top-level blocks.
/// It takes a [`Document`] rather than a parse, so the tree need not have come
/// from [`crate::parse`] and need not be inside
/// [`MAX_NESTING_DEPTH`](crate::MAX_NESTING_DEPTH) — `mt_doc::Edit::InsertNode`
/// reaches any depth without going near this crate. §11.3 therefore applies
/// here on its own account, and the walk stops descending at the limit rather
/// than trusting its caller. **Two recursions are bounded by that, not one**:
/// this function's, and `serde_json`'s over the nested [`Value`] it returns —
/// `to_string_pretty` and `Value`'s own drop glue both descend once per level.
pub fn to_state(doc: &Document) -> Value {
    Value::Array(
        doc.children(doc.root())
            .iter()
            .map(|id| node_to_state(doc, *id, 1))
            .collect(),
    )
}

/// [`to_state`], rendered as the pretty JSON `mt-cli --dump-state` prints.
pub fn to_state_json(doc: &Document) -> String {
    serde_json::to_string_pretty(&to_state(doc)).expect("a state tree is always serializable")
}

/// `depth` is the node's own depth, counting a top-level block as 1 — the same
/// unit [`crate::MAX_NESTING_DEPTH`] is stated in.
fn node_to_state(doc: &Document, id: NodeId, depth: usize) -> Value {
    let block = doc
        .block(id)
        .expect("only the root has no block, and the root is not serialized");

    let mut object = Map::new();
    object.insert("name".into(), json!(block.name()));

    if let Some(meta) = meta_of(block) {
        object.insert("meta".into(), Value::Object(meta));
    }

    match block.text() {
        Some(text) => {
            object.insert("text".into(), json!(text.to_str().as_ref()));
        }
        None => {
            // The clamp, for a document this crate did not parse. A tree from
            // `parse` is inside the limit by construction, so this branch is
            // unreachable from any markdown; a hand-built one has the blocks
            // below the limit flattened onto it, which is the same answer
            // `serialize` and `html` give the same document — see
            // `crate::leaves_below`.
            let children: Vec<Value> = if depth + 1 >= crate::MAX_NESTING_DEPTH {
                crate::leaves_below(doc, doc.children(id))
                    .into_iter()
                    .map(|leaf| node_to_state(doc, leaf, depth + 1))
                    .collect()
            } else {
                doc.children(id)
                    .iter()
                    .map(|child| node_to_state(doc, *child, depth + 1))
                    .collect()
            };
            object.insert("children".into(), Value::Array(children));
        }
    }

    Value::Object(object)
}

/// The block's `meta` object, or `None` for the seven variants that have none.
///
/// Deliberately written over [`Block`] rather than over [`mt_doc::BlockMeta`]:
/// `BlockMeta` is `SetMeta`'s payload and drops the block's identity, so a
/// serializer written against it would have to reconstruct which variant it
/// came from. The two are kept in step by
/// [`tests::every_variant_with_a_meta_object_serializes_one`].
fn meta_of(block: &Block) -> Option<Map<String, Value>> {
    let mut meta = Map::new();
    match block {
        Block::AtxHeading { level, .. } => {
            meta.insert("level".into(), json!(level));
        }
        Block::SetextHeading {
            level, underline, ..
        } => {
            meta.insert("level".into(), json!(level));
            // The literal run, not `"==="` — see `mt_doc::Underline`'s
            // correction note.
            meta.insert("underline".into(), json!(underline.to_source()));
        }
        Block::CodeBlock {
            kind,
            info,
            fence_len,
            ..
        } => {
            meta.insert(
                "type".into(),
                json!(match kind {
                    CodeKind::Indented => "indented",
                    CodeKind::Fenced => "fenced",
                }),
            );
            // The whole info string, never the first word — constraint 1 on
            // `Block::CodeBlock::info`, and #4770's data loss.
            meta.insert("lang".into(), json!(info));
            // Rule 2: omitted rather than null. `fence_len` is `Some` only
            // where muya writes the key.
            if let Some(len) = fence_len {
                meta.insert("fenceLength".into(), json!(len));
            }
        }
        Block::MathBlock { style, .. } => {
            meta.insert(
                "mathStyle".into(),
                json!(match style {
                    // Rule 3: the empty string, not "default".
                    MathStyle::Default => "",
                    MathStyle::Gitlab => "gitlab",
                }),
            );
        }
        Block::Frontmatter { lang, style, .. } => {
            meta.insert(
                "lang".into(),
                json!(match lang {
                    FrontmatterLang::Yaml => "yaml",
                    FrontmatterLang::Toml => "toml",
                    FrontmatterLang::Json => "json",
                }),
            );
            meta.insert(
                "style".into(),
                json!(match style {
                    FrontmatterStyle::Dash => "-",
                    FrontmatterStyle::Plus => "+",
                    FrontmatterStyle::Semicolon => ";",
                    FrontmatterStyle::Brace => "{",
                }),
            );
        }
        Block::Diagram { lang, kind, .. } => {
            meta.insert(
                "lang".into(),
                json!(match lang {
                    DiagramLang::Yaml => "yaml",
                    DiagramLang::Json => "json",
                }),
            );
            // The TypeScript key is `type`; the Rust field is `kind` because
            // `type` is a keyword. The JSON keeps muya's spelling.
            meta.insert(
                "type".into(),
                // `DiagramKind::info_lang` — one table in `mt-doc`, four
                // consumers; see its doc comment (M3.md §5 D11).
                json!(kind.info_lang()),
            );
        }
        Block::TableCell { align, .. } => {
            meta.insert(
                "align".into(),
                json!(match align {
                    Align::None => "none",
                    Align::Left => "left",
                    Align::Center => "center",
                    Align::Right => "right",
                }),
            );
        }
        Block::BulletList { marker, loose, .. } | Block::TaskList { marker, loose, .. } => {
            meta.insert("loose".into(), json!(loose));
            meta.insert("marker".into(), json!(marker_str(*marker)));
        }
        Block::OrderList {
            start,
            delimiter,
            loose,
            ..
        } => {
            meta.insert("loose".into(), json!(loose));
            meta.insert("start".into(), json!(start));
            meta.insert(
                "delimiter".into(),
                json!(match delimiter {
                    OrderDelim::Period => ".",
                    OrderDelim::Paren => ")",
                }),
            );
        }
        Block::TaskListItem { checked, .. } => {
            meta.insert("checked".into(), json!(checked));
        }
        Block::Footnote { identifier, .. } => {
            meta.insert("identifier".into(), json!(identifier));
        }
        // Rule 1: no `meta` key at all, not an empty object.
        Block::Paragraph { .. }
        | Block::ThematicBreak { .. }
        | Block::HtmlBlock { .. }
        | Block::BlockQuote { .. }
        | Block::ListItem { .. }
        | Block::Table { .. }
        | Block::TableRow { .. } => return None,
    }
    Some(meta)
}

fn marker_str(marker: BulletMarker) -> &'static str {
    match marker {
        BulletMarker::Dash => "-",
        BulletMarker::Plus => "+",
        BulletMarker::Star => "*",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `DiagramKind` is only named by the tests now that the kind -> string
    // table moved to `mt-doc` (M3.md §5 D11).
    use mt_doc::{DiagramKind, Text, Underline};

    /// Build a document from a flat list of top-level blocks.
    ///
    /// Through `Document::apply`, like everything else that writes to a
    /// document — see `block::to_document` for why that is not a detour.
    fn document_of(blocks: Vec<Block>) -> Document {
        let mut doc = Document::new();
        let root = doc.root();
        let placeholder = doc.children(root)[0];
        for (index, block) in blocks.into_iter().enumerate() {
            doc.apply(&[mt_doc::Edit::InsertNode {
                parent: root,
                index,
                block,
            }]);
        }
        doc.apply(&[mt_doc::Edit::RemoveNode { node: placeholder }]);
        doc.prune_detached();
        doc
    }

    #[test]
    fn a_paragraph_has_a_name_and_a_text_and_no_meta_key() {
        let doc = document_of(vec![Block::Paragraph {
            text: Text::from("hello"),
        }]);
        assert_eq!(
            to_state(&doc),
            json!([{ "name": "paragraph", "text": "hello" }])
        );
    }

    /// Rule 1, stated as a failure: an empty `meta` object is not the same as
    /// no `meta` key, and `first_difference` reports the difference on every
    /// block in the document if this regresses.
    #[test]
    fn the_seven_variants_without_a_meta_object_emit_no_meta_key() {
        for block in [
            Block::Paragraph { text: Text::new() },
            Block::ThematicBreak { text: Text::new() },
            Block::HtmlBlock { text: Text::new() },
            Block::BlockQuote { children: vec![] },
            Block::ListItem { children: vec![] },
            Block::Table { children: vec![] },
            Block::TableRow { children: vec![] },
        ] {
            let name = block.name();
            let doc = document_of(vec![block]);
            let state = to_state(&doc);
            assert!(state[0].get("meta").is_none(), "{name} emitted a meta key");
        }
    }

    /// Rule 2. A three-backtick fence carries no `fenceLength`; a four-backtick
    /// one carries `4`. This is the round-trip hazard `Block::CodeBlock`'s
    /// constraint 3 names.
    #[test]
    fn fence_length_is_omitted_for_the_default_fence_and_present_otherwise() {
        let code = |fence_len| Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "js".into(),
            fence_len,
            text: Text::from("x"),
        };
        let three = to_state(&document_of(vec![code(None)]));
        assert!(three[0]["meta"].get("fenceLength").is_none());
        assert_eq!(three[0]["meta"]["lang"], json!("js"));

        let four = to_state(&document_of(vec![code(Some(4))]));
        assert_eq!(four[0]["meta"]["fenceLength"], json!(4));
    }

    /// Rule 3.
    #[test]
    fn the_default_math_style_is_the_empty_string() {
        let doc = document_of(vec![Block::MathBlock {
            style: MathStyle::Default,
            text: Text::from("x"),
        }]);
        assert_eq!(to_state(&doc)[0]["meta"]["mathStyle"], json!(""));
    }

    /// Constraint 1: `meta.lang` is the whole info string, and the shape of
    /// the bug `diff.rs::catches_a_truncated_code_fence_info_string` exists to
    /// catch is this serializer emitting the first word instead.
    #[test]
    fn the_code_block_lang_is_the_whole_info_string() {
        let doc = document_of(vec![Block::CodeBlock {
            kind: CodeKind::Fenced,
            info: "js title=\"app.js\"".into(),
            fence_len: None,
            text: Text::new(),
        }]);
        assert_eq!(
            to_state(&doc)[0]["meta"]["lang"],
            json!("js title=\"app.js\"")
        );
    }

    #[test]
    fn a_container_emits_children_and_never_a_text_key() {
        let mut doc = Document::new();
        let root = doc.root();
        let placeholder = doc.children(root)[0];
        let quote = match doc
            .apply(&[mt_doc::Edit::InsertNode {
                parent: root,
                index: 0,
                block: Block::BlockQuote { children: vec![] },
            }])
            .as_slice()
        {
            [mt_doc::Edit::RemoveNode { node }] => *node,
            other => panic!("unexpected inverse: {other:?}"),
        };
        doc.apply(&[mt_doc::Edit::InsertNode {
            parent: quote,
            index: 0,
            block: Block::Paragraph {
                text: Text::from("q"),
            },
        }]);
        doc.apply(&[mt_doc::Edit::RemoveNode { node: placeholder }]);
        doc.prune_detached();

        assert_eq!(
            to_state(&doc),
            json!([{
                "name": "block-quote",
                "children": [{ "name": "paragraph", "text": "q" }],
            }])
        );
    }

    /// Twelve variants carry a `meta` object — the same twelve
    /// `mt_doc::Block::meta` reports, so the serializer and the model cannot
    /// drift about which those are.
    #[test]
    fn every_variant_with_a_meta_object_serializes_one() {
        let t = Text::new;
        let variants = [
            Block::Paragraph { text: t() },
            Block::AtxHeading {
                level: 1,
                text: t(),
            },
            Block::SetextHeading {
                level: 2,
                underline: Underline::Dashes(3),
                text: t(),
            },
            Block::ThematicBreak { text: t() },
            Block::CodeBlock {
                kind: CodeKind::Indented,
                info: String::new(),
                fence_len: None,
                text: t(),
            },
            Block::HtmlBlock { text: t() },
            Block::MathBlock {
                style: MathStyle::Gitlab,
                text: t(),
            },
            Block::Frontmatter {
                lang: FrontmatterLang::Toml,
                style: FrontmatterStyle::Plus,
                text: t(),
            },
            Block::Diagram {
                lang: DiagramLang::Json,
                kind: DiagramKind::VegaLite,
                text: t(),
            },
            Block::TableCell {
                align: Align::Center,
                text: t(),
            },
            Block::BlockQuote { children: vec![] },
            Block::BulletList {
                marker: BulletMarker::Star,
                loose: true,
                children: vec![],
            },
            Block::OrderList {
                start: 3,
                delimiter: OrderDelim::Paren,
                loose: false,
                children: vec![],
            },
            Block::ListItem { children: vec![] },
            Block::TaskList {
                marker: BulletMarker::Plus,
                loose: false,
                children: vec![],
            },
            Block::TaskListItem {
                checked: true,
                children: vec![],
            },
            Block::Table { children: vec![] },
            Block::TableRow { children: vec![] },
            Block::Footnote {
                identifier: "a".into(),
                children: vec![],
            },
        ];
        let mut with_meta = 0;
        for block in variants {
            let name = block.name();
            let model_says = block.meta().is_some();
            let doc = document_of(vec![block]);
            let json_says = to_state(&doc)[0].get("meta").is_some();
            assert_eq!(
                model_says, json_says,
                "{name}: Block::meta says {model_says}, the serializer says {json_says}"
            );
            if json_says {
                with_meta += 1;
            }
        }
        assert_eq!(with_meta, 12);
    }
}

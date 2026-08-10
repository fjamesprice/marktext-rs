//! E2 build 4a — build 1 (parley + `vello_cpu`) plus `tree-sitter` core plus
//! ONE grammar (`tree-sitter-rust`). Compare against `e2-treesitter-20` (same
//! core, 20 grammars) to separate tree-sitter's fixed cost from its
//! per-grammar marginal cost.
//!
//! No `std::env::args()` dispatch is needed here: unlike the renderer builds,
//! nothing here is mutually exclusive with anything else, so both the
//! `vello_cpu` baseline and the parse both run unconditionally, every
//! invocation — trivially "genuinely reachable", and each result is printed.
use tree_sitter::Parser;

const RUST_SNIPPET: &str = r#"
fn double(x: i32) -> i32 {
    x * 2
}
"#;

fn main() {
    e2_common::run_and_print_baseline("treesitter-1");

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("load tree-sitter-rust grammar");
    let tree = parser.parse(RUST_SNIPPET, None).expect("parse rust snippet");
    let root = tree.root_node();
    let sexp = root.to_sexp();
    let hash = e2_common::fnv1a(sexp.as_bytes());
    println!(
        "[tree-sitter] grammars=1 (tree-sitter-rust 0.24.2) root_kind={:?} \
         has_error={} node_count={} sexp_len={} sexp_hash={hash:016x}",
        root.kind(),
        root.has_error(),
        count_nodes(root),
        sexp.len()
    );
}

fn count_nodes(node: tree_sitter::Node) -> usize {
    let mut count = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_nodes(child);
    }
    count
}

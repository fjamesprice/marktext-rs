//! E2 build 4b — build 1 (parley + `vello_cpu`) plus `tree-sitter` core plus
//! 20 grammars, matching §12.2's "tree-sitter core + ~20 bundled grammars"
//! row. Compare against `e2-treesitter-1` (same core, 1 grammar) to separate
//! the fixed cost of `tree-sitter` core from the marginal cost per grammar.
//!
//! Every grammar is parsed against the SAME generic snippet rather than 20
//! language-correct samples — tree-sitter is error-tolerant (invalid syntax
//! still produces a tree with `ERROR` nodes), and the point here is genuine,
//! non-elidable linkage and execution of each grammar's parse tables, not a
//! correctness check. Each call's result (root kind, node count, an s-expr
//! hash) is printed, so none of the 20 can be proven dead or unobserved.
use tree_sitter::{Node, Parser};
use tree_sitter_language::LanguageFn;

const SNIPPET: &str = r#"
// generic snippet: braces, a string, a number, a comment
function or fn thing(x, y) {
    let z = x + y; // add
    return "result: " + z + 42;
}
"#;

fn main() {
    e2_common::run_and_print_baseline("treesitter-20");

    let grammars: &[(&str, &str, LanguageFn)] = &[
        ("tree-sitter-rust", "0.24.2", tree_sitter_rust::LANGUAGE),
        (
            "tree-sitter-javascript",
            "0.25.0",
            tree_sitter_javascript::LANGUAGE,
        ),
        (
            "tree-sitter-typescript",
            "0.23.2",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
        ),
        ("tree-sitter-python", "0.25.0", tree_sitter_python::LANGUAGE),
        ("tree-sitter-c", "0.24.2", tree_sitter_c::LANGUAGE),
        ("tree-sitter-cpp", "0.23.4", tree_sitter_cpp::LANGUAGE),
        ("tree-sitter-java", "0.23.5", tree_sitter_java::LANGUAGE),
        ("tree-sitter-go", "0.25.0", tree_sitter_go::LANGUAGE),
        ("tree-sitter-ruby", "0.23.1", tree_sitter_ruby::LANGUAGE),
        ("tree-sitter-php", "0.24.2", tree_sitter_php::LANGUAGE_PHP),
        ("tree-sitter-html", "0.23.2", tree_sitter_html::LANGUAGE),
        ("tree-sitter-css", "0.25.0", tree_sitter_css::LANGUAGE),
        ("tree-sitter-json", "0.24.8", tree_sitter_json::LANGUAGE),
        ("tree-sitter-yaml", "0.7.2", tree_sitter_yaml::LANGUAGE),
        ("tree-sitter-bash", "0.25.1", tree_sitter_bash::LANGUAGE),
        ("tree-sitter-md", "0.5.3", tree_sitter_md::LANGUAGE),
        (
            "tree-sitter-toml-ng",
            "0.7.0",
            tree_sitter_toml_ng::LANGUAGE,
        ),
        (
            "tree-sitter-c-sharp",
            "0.23.5",
            tree_sitter_c_sharp::LANGUAGE,
        ),
        (
            "tree-sitter-kotlin-ng",
            "1.1.0",
            tree_sitter_kotlin_ng::LANGUAGE,
        ),
        ("tree-sitter-scala", "0.26.2", tree_sitter_scala::LANGUAGE),
    ];

    println!(
        "[tree-sitter] grammars={} against the same generic snippet",
        grammars.len()
    );
    let mut total_nodes = 0usize;
    let mut combined_hash: u64 = 0xcbf29ce484222325;
    for (name, version, lang_fn) in grammars {
        let mut parser = Parser::new();
        parser
            .set_language(&(*lang_fn).into())
            .unwrap_or_else(|e| panic!("load {name} {version}: {e}"));
        let tree = parser
            .parse(SNIPPET, None)
            .unwrap_or_else(|| panic!("{name} {version} failed to parse"));
        let root = tree.root_node();
        let sexp = root.to_sexp();
        let nodes = count_nodes(root);
        total_nodes += nodes;
        combined_hash ^= e2_common::fnv1a(sexp.as_bytes());
        println!(
            "    {name:<24} {version:<8} root_kind={:<16?} has_error={:<5} node_count={nodes}",
            root.kind(),
            root.has_error()
        );
    }
    println!(
        "[tree-sitter] total_nodes_across_all_grammars={total_nodes} combined_hash={combined_hash:016x}"
    );
}

fn count_nodes(node: Node) -> usize {
    let mut count = 1;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_nodes(child);
    }
    count
}

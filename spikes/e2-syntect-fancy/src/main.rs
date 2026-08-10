//! E2 optional — `syntect` with the pure-Rust `regex-fancy` backend. See
//! `e2-syntect-onig`'s module docs; this is the same program, the other
//! regex backend.
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

fn main() {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("rs").expect("rust syntax");
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);
    let source = "pub struct Wow { hi: u64 }\nfn blah() -> u64 { 42 }\n";

    let mut total_spans = 0usize;
    for line in LinesWithEndings::from(source) {
        let ranges = h.highlight_line(line, &ps).expect("highlight line");
        total_spans += ranges.len();
    }
    println!(
        "[syntect regex-fancy] syntaxes={} themes={} spans_highlighted={total_spans}",
        ps.syntaxes().len(),
        ts.themes.len()
    );
}

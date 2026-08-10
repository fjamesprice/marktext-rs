//! E2 optional — `ignore` + `grep-searcher` (+ `grep-regex` as the `Matcher`
//! impl `grep-searcher` needs to do anything). §12.2 bills these at ~1.5 MB;
//! they belong to a later milestone (in-document / in-project search) but
//! link cheaply today.
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::Searcher;
use grep_searcher::sinks::UTF8;
use ignore::WalkBuilder;

const HAYSTACK: &[u8] = b"For the Doctor Watsons of this world, as opposed to the Sherlock\n\
Holmeses, success in the province of detective work must always\n\
be, to a very large extent, the result of luck. Sherlock Holmes\n\
can extract a clew from a wisp of straw or a flake of cigar ash;\n\
but Doctor Watson has to have it taken out for him and dusted,\n\
and exhibited clearly, with a label attached.\n";

fn main() {
    // `ignore`: walk this crate's own directory, honouring .gitignore rules.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut file_count = 0usize;
    for entry in WalkBuilder::new(dir).build() {
        if let Ok(entry) = entry {
            if entry.file_type().is_some_and(|t| t.is_file()) {
                file_count += 1;
            }
        }
    }

    // `grep-searcher` + `grep-regex`: the exact pattern from grep-searcher's
    // own doc example, over an in-memory haystack rather than a file.
    let matcher = RegexMatcher::new(r"Doctor \w+").expect("compile regex");
    let mut matches: Vec<(u64, String)> = Vec::new();
    Searcher::new()
        .search_slice(
            &matcher,
            HAYSTACK,
            UTF8(|lnum, line| {
                let m = matcher.find(line.as_bytes())?.unwrap();
                matches.push((lnum, line[m].to_string()));
                Ok(true)
            }),
        )
        .expect("search");

    println!(
        "[ignore] walked {} file(s) under {}",
        file_count,
        dir.display()
    );
    println!("[grep-searcher+grep-regex] matches={matches:?}");
}

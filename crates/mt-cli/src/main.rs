//! `mt-cli` binary. See the crate docs in `lib.rs` for the contract.

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let command = match mt_cli::parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("mt-cli: {e}");
            eprint!("{}", mt_cli::USAGE);
            std::process::exit(mt_cli::ExitCode::ERROR);
        }
    };

    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let code = mt_cli::run(command, &mut out);
    let _ = out.flush();
    std::process::exit(code);
}

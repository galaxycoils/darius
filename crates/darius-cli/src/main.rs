use clap::Parser;
use std::io::IsTerminal;

fn main() {
    let cli = darius_cli::args::Cli::parse();
    let io = darius_cli::IoCaps::new(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    );
    if let Err(error) = darius_cli::run_with(cli, io) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#![forbid(unsafe_code)]
#![doc = "Pulp command-line binary entry point."]

use clap::Parser;

fn main() {
    let cli = pulp_cli::Cli::parse();
    if let Err(error) = pulp_cli::run(cli) {
        eprintln!("pulp: {error}");
        std::process::exit(1);
    }
}

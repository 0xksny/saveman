use std::process::ExitCode;

use clap::Parser;

use crate::cli::Cli;

mod cli;
mod ps2;

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {:#}", error);
            ExitCode::FAILURE
        }
    }
}

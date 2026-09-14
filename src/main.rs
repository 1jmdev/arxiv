use std::io::{self, Write};
use std::process::ExitCode;

use arxiv::arguments::Arguments;
use clap::Parser;

fn main() -> ExitCode {
    match arxiv::commands::execute(Arguments::parse()) {
        Ok(output) => match io::stdout().lock().write_all(output.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("arxiv: {error}");
                ExitCode::FAILURE
            }
        },
        Err(error) => {
            eprintln!("arxiv: {error:#}");
            ExitCode::FAILURE
        }
    }
}

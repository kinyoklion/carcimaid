//! `carcimaid` — command-line front-end for the [`carcimaid`] library.
//!
//! Reads a mermaid diagram from a file (or stdin) and writes SVG to a file
//! (or stdout).

use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

/// Render a mermaid diagram to SVG.
#[derive(Parser, Debug)]
#[command(name = "carcimaid", version, about)]
struct Cli {
    /// Input `.mmd` file. Reads from stdin when omitted or `-`.
    input: Option<PathBuf>,

    /// Output SVG file. Writes to stdout when omitted or `-`.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("carcimaid: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let source = read_input(cli.input.as_deref())?;
    let svg = carcimaid::render_to_svg(&source)?;
    write_output(cli.output.as_deref(), &svg)?;
    Ok(())
}

fn read_input(path: Option<&std::path::Path>) -> io::Result<String> {
    match path {
        Some(p) if p.as_os_str() != "-" => fs::read_to_string(p),
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok(buf)
        }
    }
}

fn write_output(path: Option<&std::path::Path>, svg: &str) -> io::Result<()> {
    match path {
        Some(p) if p.as_os_str() != "-" => fs::write(p, svg),
        _ => {
            io::stdout().write_all(svg.as_bytes())?;
            io::stdout().write_all(b"\n")
        }
    }
}

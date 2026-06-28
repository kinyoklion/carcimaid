//! Compliance suite runner.
//!
//! For each corpus diagram, render with carcimaid and with the mermaid CLI
//! oracle, structurally diff the two SVGs, write artifacts, and print a summary.
//!
//! Usage:
//!   compliance [--corpus DIR] [--artifacts DIR] [--filter SUBSTR]
//!              [--tolerance N] [--no-oracle] [-v]

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use compliance::{corpus, oracle::Oracle, svgdiff};

struct Args {
    corpus: PathBuf,
    artifacts: PathBuf,
    filter: Option<String>,
    tolerance: f64,
    use_oracle: bool,
    verbose: bool,
}

impl Args {
    fn parse() -> Args {
        let mut a = Args {
            corpus: PathBuf::from("corpus"),
            artifacts: PathBuf::from("artifacts"),
            filter: None,
            tolerance: 1.0,
            use_oracle: true,
            verbose: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--corpus" => a.corpus = it.next().expect("--corpus needs a path").into(),
                "--artifacts" => a.artifacts = it.next().expect("--artifacts needs a path").into(),
                "--filter" => a.filter = it.next(),
                "--tolerance" => {
                    a.tolerance = it.next().and_then(|s| s.parse().ok()).expect("--tolerance N")
                }
                "--no-oracle" => a.use_oracle = false,
                "-v" | "--verbose" => a.verbose = true,
                other => {
                    eprintln!("unknown argument: {other}");
                }
            }
        }
        a
    }
}

fn main() -> ExitCode {
    let args = Args::parse();
    let cases = match corpus::discover(&args.corpus) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("failed to read corpus at {}: {e}", args.corpus.display());
            return ExitCode::FAILURE;
        }
    };
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|c| args.filter.as_ref().map_or(true, |f| c.id.contains(f)))
        .collect();

    if cases.is_empty() {
        eprintln!("no corpus cases found under {}", args.corpus.display());
        return ExitCode::FAILURE;
    }

    let oracle = Oracle::default();
    let oracle_ready = args.use_oracle && oracle.is_available();
    if args.use_oracle && !oracle_ready {
        eprintln!(
            "warning: oracle image `{}` not available; rendering carcimaid only",
            oracle.image
        );
    }

    let opts = svgdiff::Options {
        numeric_tolerance: args.tolerance,
        ..Default::default()
    };

    let mut passed = 0usize;
    let mut compared = 0usize;
    let mut errored = 0usize;

    println!("{:<40} {:>10} {:>8} {:>8}", "case", "tag-sim", "diffs", "result");
    println!("{}", "-".repeat(70));

    for case in &cases {
        let out_dir = args.artifacts.join(&case.id);
        let _ = std::fs::create_dir_all(&out_dir);

        let ours = match carcimaid::render_to_svg(&case.source) {
            Ok(svg) => svg,
            Err(e) => {
                println!("{:<40} {:>10} {:>8} {:>8}", trunc(&case.id), "-", "-", "ERR");
                if args.verbose {
                    println!("    carcimaid error: {e}");
                }
                errored += 1;
                continue;
            }
        };
        let _ = std::fs::write(out_dir.join("carcimaid.svg"), &ours);

        if !oracle_ready {
            println!("{:<40} {:>10} {:>8} {:>8}", trunc(&case.id), "-", "-", "ours");
            continue;
        }

        let reference = match oracle.render(&case.source, &out_dir.join("oracle")) {
            Ok(svg) => svg,
            Err(e) => {
                println!("{:<40} {:>10} {:>8} {:>8}", trunc(&case.id), "-", "-", "ORACLE-ERR");
                if args.verbose {
                    println!("    {e}");
                }
                errored += 1;
                continue;
            }
        };
        let _ = std::fs::write(out_dir.join("oracle.svg"), &reference);

        let report = match (svgdiff::parse(&reference), svgdiff::parse(&ours)) {
            (Ok(r), Ok(c)) => svgdiff::compare(&r, &c, &opts),
            _ => {
                println!("{:<40} {:>10} {:>8} {:>8}", trunc(&case.id), "-", "-", "PARSE-ERR");
                errored += 1;
                continue;
            }
        };
        compared += 1;
        let result = if report.is_match() {
            passed += 1;
            "PASS"
        } else {
            "diff"
        };
        println!(
            "{:<40} {:>10.3} {:>8} {:>8}",
            trunc(&case.id),
            report.tag_similarity,
            report.differences.len(),
            result
        );
        if args.verbose {
            for d in report.differences.iter().take(10) {
                println!("    {} : {:?}", d.path, d.kind);
            }
        }
        write_report(&out_dir, &report);
    }

    println!("{}", "-".repeat(70));
    println!(
        "{} cases | {} compared | {} pass | {} errors",
        cases.len(),
        compared,
        passed,
        errored
    );

    if errored > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn write_report(dir: &Path, report: &svgdiff::Report) {
    let mut s = String::new();
    s.push_str(&format!("tag_similarity: {:.4}\n", report.tag_similarity));
    s.push_str(&format!("reference_size: {}\n", report.reference_size));
    s.push_str(&format!("candidate_size: {}\n", report.candidate_size));
    s.push_str(&format!("differences: {}\n\n", report.differences.len()));
    for d in &report.differences {
        s.push_str(&format!("{} : {:?}\n", d.path, d.kind));
    }
    let _ = std::fs::write(dir.join("report.txt"), s);
}

fn trunc(s: &str) -> String {
    if s.len() <= 40 {
        s.to_string()
    } else {
        format!("…{}", &s[s.len() - 39..])
    }
}

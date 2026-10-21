//! `release-manifest section --version <v> [--changelog <file>] [--dry-run]`
//! `release-manifest assemble --version <v> --pub-date <date> --dir <dir> [--changelog <file>] [--dry-run]`
//! `release-manifest local --version <v> --pub-date <date> --dir <dir> --platform <key> --package <file>`
//!
//! `section` prints the version's section of the changelog (default
//! `CHANGELOG.md`). `assemble` reads the packages and `.sig` files in `<dir>`,
//! writes `<dir>/latest.json` and prints the release notes. Without a section
//! for the version both fail; with `--dry-run` they take the Unreleased
//! section instead, or the newest one when Unreleased is empty, and say so.
//! Exits 1 on an error and 2 on a usage error.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: release-manifest section --version <v> [--changelog <file>] [--dry-run]\n       \
release-manifest assemble --version <v> --pub-date <date> --dir <dir> [--changelog <file>] [--dry-run]\n       \
release-manifest local --version <v> --pub-date <date> --dir <dir> --platform <key> --package <file>";

enum Command {
    Section,
    Assemble {
        pub_date: String,
        dir: PathBuf,
    },
    Local {
        pub_date: String,
        dir: PathBuf,
        platform: String,
        package: String,
    },
}

struct Args {
    command: Command,
    version: String,
    changelog: PathBuf,
    dry_run: bool,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Option<Args> {
    let command = args.next()?;
    let mut version = None;
    let mut pub_date = None;
    let mut dir = None;
    let mut changelog = PathBuf::from("CHANGELOG.md");
    let mut dry_run = false;
    let mut platform = None;
    let mut package = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--version" => version = Some(args.next()?),
            "--pub-date" => pub_date = Some(args.next()?),
            "--dir" => dir = Some(PathBuf::from(args.next()?)),
            "--changelog" => changelog = PathBuf::from(args.next()?),
            "--dry-run" => dry_run = true,
            "--platform" => platform = Some(args.next()?),
            "--package" => package = Some(args.next()?),
            _ => return None,
        }
    }
    let command = match command.as_str() {
        "section" if pub_date.is_none() && dir.is_none() => Command::Section,
        "assemble" => Command::Assemble {
            pub_date: pub_date?,
            dir: dir?,
        },
        "local" => Command::Local {
            pub_date: pub_date?,
            dir: dir?,
            platform: platform?,
            package: package?,
        },
        _ => return None,
    };
    Some(Args {
        command,
        version: version?,
        changelog,
        dry_run,
    })
}

fn run(args: &Args) -> Result<(), release_manifest::Error> {
    let changelog =
        std::fs::read_to_string(&args.changelog).map_err(|source| release_manifest::Error::Io {
            path: args.changelog.clone(),
            source,
        })?;
    let (text, fallback) = match &args.command {
        Command::Section => {
            let section =
                release_manifest::release_section(&changelog, &args.version, args.dry_run)?;
            (section.text, section.fallback)
        }
        Command::Assemble { pub_date, dir } => {
            let assembled =
                release_manifest::assemble(dir, &changelog, &args.version, pub_date, args.dry_run)?;
            (assembled.notes, assembled.fallback)
        }
        Command::Local {
            pub_date,
            dir,
            platform,
            package,
        } => {
            let section = release_manifest::release_section(&changelog, &args.version, true)?;
            let value = release_manifest::local_manifest(
                dir,
                &args.version,
                &section.text,
                pub_date,
                platform,
                package,
            )?;
            println!("{value}");
            return Ok(());
        }
    };
    if let Some(name) = fallback {
        eprintln!(
            "release-manifest: {} has no section for {}; this dry run uses the {name} section",
            args.changelog.display(),
            args.version
        );
    }
    print!("{text}");
    if !text.ends_with('\n') {
        println!();
    }
    Ok(())
}

fn main() -> ExitCode {
    let Some(args) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("release-manifest: {err}");
            ExitCode::FAILURE
        }
    }
}

//! `stemma`: the command line of the Stemma specification.

mod commands;
mod config;
mod lake;
mod library;
mod report;
mod templates;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(version, about = "Mathematics in Lean, written with agents")]
struct Cli {
    /// Print the result as JSON, for agents.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a Stemma library.
    Init {
        /// The directory of the library.
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// The library's Lean name. By default, from the directory's name.
        #[arg(long)]
        name: Option<String>,
        /// The library's title. By default, its name.
        #[arg(long)]
        title: Option<String>,
        /// Do not depend on Mathlib.
        #[arg(long)]
        no_mathlib: bool,
        /// Use Stemma's Lean package from this directory instead of its release.
        #[arg(long, env = "STEMMA_LEAN")]
        stemma_lean: Option<PathBuf>,
        /// Do not create a git repository.
        #[arg(long)]
        no_git: bool,
    },
    /// Create a module and add it to the table of contents.
    New {
        /// The module's name, with or without the library's prefix.
        module: String,
        /// Create a Lean module instead of a document.
        #[arg(long)]
        lean: bool,
        /// The module's title. By default, the last part of its name.
        #[arg(long)]
        title: Option<String>,
        /// Place the module after this one in the table of contents.
        #[arg(long)]
        after: Option<String>,
    },
    /// Run every check of the specification.
    Check,
    /// Show the state of every environment.
    Status,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Init {
            dir,
            name,
            title,
            no_mathlib,
            stemma_lean,
            no_git,
        } => commands::init(
            commands::InitOptions {
                dir,
                name,
                title,
                mathlib: !no_mathlib,
                stemma_lean,
                git: !no_git,
            },
            cli.json,
        )
        .map(|()| true),
        Commands::New {
            module,
            lean,
            title,
            after,
        } => commands::new(
            commands::NewOptions {
                module,
                lean,
                title,
                after,
            },
            cli.json,
        )
        .map(|()| true),
        Commands::Check => commands::check(cli.json),
        Commands::Status => commands::status(cli.json),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

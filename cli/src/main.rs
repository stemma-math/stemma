//! `stemma`: the command line of the Stemma specification.

mod agent;
mod agents_md;
mod commands;
mod config;
mod init;
mod lake;
mod library;
mod readback;
mod report;
mod share;
mod sign;
mod site;
mod templates;
mod ui;
mod verify;

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
    /// Create a Stemma library. In a terminal, it asks what it needs.
    Init {
        /// The directory of the library.
        dir: Option<PathBuf>,
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
        /// Download the dependencies (`lake update`, and Mathlib's cache).
        #[arg(long)]
        update: bool,
        /// Commit the new library.
        #[arg(long)]
        commit: bool,
        /// Add this remote as origin.
        #[arg(long)]
        remote: Option<String>,
        /// Push main to the remote.
        #[arg(long)]
        push: bool,
        /// Ask nothing: take every default.
        #[arg(long, short)]
        yes: bool,
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
    /// Build the library's site and serve it locally.
    Preview {
        /// Build the site without serving it.
        #[arg(long)]
        no_serve: bool,
        /// The port to serve on (or the next free one).
        #[arg(long, default_value_t = 8000)]
        port: u16,
    },
    /// Bring `main` in, and open or update the pull request.
    Share,
    /// Make blind read-backs of environments' Lean, and show them beside their prose.
    Readback {
        /// Read back these labels. By default, every central environment
        /// without a current read-back.
        labels: Vec<String>,
        /// Read back every environment of this module.
        #[arg(long)]
        module: Option<String>,
        /// Make them again even when they are current.
        #[arg(long)]
        force: bool,
        /// The agent that makes them.
        #[arg(long, value_enum, default_value = "claude")]
        agent: readback::Translator,
        /// The agent's model.
        #[arg(long)]
        model: Option<String>,
        /// Make them without serving the page.
        #[arg(long)]
        no_serve: bool,
        /// The port to serve on (or the next free one).
        #[arg(long, default_value_t = 8001)]
        port: u16,
    },
    /// Sign central environments, in your own terminal.
    Sign {
        /// Sign only these labels.
        labels: Vec<String>,
        /// Write the signature files without committing them.
        #[arg(long)]
        no_commit: bool,
    },
    /// Check what the forge requires of a pull request: signatures and policy.
    Verify {
        /// The base to compare with. By default, the pull request's base, or
        /// `origin/main`.
        #[arg(long)]
        base: Option<String>,
    },
    /// Hooks agents run; not meant to be called by hand.
    #[command(hide = true)]
    Hook {
        /// The event: `session-start`.
        event: String,
    },
    /// Start Claude Code, equipped to work in the library.
    Claude(AgentArgs),
    /// Start Codex, equipped to work in the library.
    Codex(AgentArgs),
}

#[derive(clap::Args)]
struct AgentArgs {
    /// Print the command instead of running it.
    #[arg(long)]
    dry_run: bool,
    /// Arguments passed on to the agent.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Runs a hook for an agent: prints what the agent should know.
fn hook(event: &str) -> anyhow::Result<bool> {
    match event {
        "session-start" => {
            let library = library::Library::find(std::path::Path::new("."))?;
            println!("{}", agent::session_summary(&library));
            Ok(true)
        }
        other => anyhow::bail!("unknown hook event '{other}'"),
    }
}

/// Starts an agent, or prints how it would be started.
fn start(agent: agent::Agent, args: AgentArgs) -> anyhow::Result<bool> {
    let launch = agent::prepare(agent, args.args, args.dry_run)?;
    if args.dry_run {
        let value = serde_json::json!({
            "program": launch.program, "args": launch.args,
            "dir": launch.dir, "branch": launch.branch,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(true);
    }
    if let Some(branch) = &launch.branch {
        ui::note(format!("Working on the branch {branch}."));
    }
    launch.run()
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
            update,
            commit,
            remote,
            push,
            yes,
        } => init::init(
            init::Options {
                dir,
                name,
                title,
                mathlib: no_mathlib.then_some(false),
                stemma_lean,
                git: no_git.then_some(false),
                update: update.then_some(true),
                commit: commit.then_some(true),
                remote,
                push: push.then_some(true),
                yes,
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
        Commands::Preview { no_serve, port } => site::preview(!no_serve, port, cli.json),
        Commands::Readback {
            labels,
            module,
            force,
            agent,
            model,
            no_serve,
            port,
        } => readback::readback(
            readback::Options {
                labels,
                module,
                force,
                translator: agent,
                model,
                serve: !no_serve,
                port,
            },
            cli.json,
        ),
        Commands::Share => share::share(cli.json),
        Commands::Sign { labels, no_commit } => sign::sign(labels, !no_commit),
        Commands::Verify { base } => verify::verify(base, cli.json),
        Commands::Hook { event } => hook(&event),
        Commands::Claude(args) => start(agent::Agent::Claude, args),
        Commands::Codex(args) => start(agent::Agent::Codex, args),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!(
                "{} {error:#}",
                owo_colors::OwoColorize::if_supports_color(
                    &"✗",
                    owo_colors::Stream::Stderr,
                    |t| { owo_colors::OwoColorize::red(t).to_string() }
                )
            );
            ExitCode::FAILURE
        }
    }
}

//! `stemma`: the command line of the Stemma specification.

mod agent;
mod agents_md;
mod branches;
mod card;
mod commands;
mod config;
mod forge;
mod help;
mod init;
mod interact;
mod keys;
mod lake;
mod library;
mod readback;
mod report;
mod scope;
mod share;
mod sign;
mod site;
mod sources;
mod templates;
mod ui;
mod upgrade;
mod verify;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

#[derive(Parser)]
#[command(
    version,
    about = "Mathematics in Lean, written with agents",
    styles = help::styles()
)]
pub(crate) struct Cli {
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
        /// Publish the library's site to GitHub Pages from main (`[site]
        /// publish` in stemma.toml).
        #[arg(long)]
        publish_site: bool,
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
        /// Do not set the repository up on the forge after pushing main.
        #[arg(long)]
        no_forge: bool,
        /// Another member, by forge account (repeatable). You are always one.
        #[arg(long = "member")]
        members: Vec<String>,
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
    /// Share your branch: bring main in, and open or update the pull request.
    Share {
        /// Share even when the pull request lacks signatures or approvals you
        /// could give it.
        #[arg(long)]
        anyway: bool,
        /// Share up to this commit of the branch, rather than all of it.
        #[arg(long, value_name = "COMMIT")]
        upto: Option<String>,
        /// The title of a new pull request.
        #[arg(long)]
        title: Option<String>,
        /// The description of a new pull request.
        #[arg(long)]
        body: Option<String>,
        /// What to do with commits made elsewhere on the share branch: bring them
        /// into your branch, or discard them.
        #[arg(long, value_enum, value_name = "WHAT")]
        foreign: Option<share::Foreign>,
        /// Do not build the library first: the pull request's checks still do.
        #[arg(long)]
        no_build: bool,
        /// Ask nothing: take what stemma would do.
        #[arg(long, short)]
        yes: bool,
    },
    /// Make blind read-backs of environments' Lean, and serve a page that sets
    /// them beside their prose, for a person to read.
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
        /// Print the text of the read-backs (of the labels given, or every
        /// one) instead of making any: only when the person asks for it.
        #[arg(long, conflicts_with_all = ["force", "notes"])]
        content: bool,
        /// Print the person's marks and notes (on the labels given, or every
        /// one) instead of making any.
        #[arg(long, conflicts_with = "force")]
        notes: bool,
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
    /// Sign central environments and approve changes, in your own terminal.
    Sign {
        /// Sign only these labels.
        labels: Vec<String>,
        /// The base the change is compared with, for approvals.
        #[arg(long, default_value = "origin/main")]
        base: String,
        /// Write the signature files without committing them.
        #[arg(long)]
        no_commit: bool,
    },
    /// Register the key you sign with, in a change a maintainer approves.
    Key {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Check that a change carries the signatures and approvals it needs.
    Verify {
        /// The base the change is compared with. By default, `origin/main`.
        #[arg(long)]
        base: Option<String>,
        /// Check only signatures and approvals, without building the library.
        #[arg(long)]
        no_build: bool,
    },
    /// Show or set the personal remote your working branches are saved to.
    Remote {
        /// The personal remote's URL. Working branches are saved there, and
        /// only share branches go to the group's repository.
        url: Option<String>,
        /// Save working branches to the group's repository again.
        #[arg(long)]
        unset: bool,
    },
    /// Move the library to this version of stemma.
    Upgrade {
        /// Show what would change without changing anything.
        #[arg(long)]
        dry_run: bool,
        /// Do not update the dependencies (`lake update`).
        #[arg(long)]
        no_update: bool,
        /// Do not commit the upgrade.
        #[arg(long)]
        no_commit: bool,
        /// Do not set the repository up on the forge.
        #[arg(long)]
        no_forge: bool,
        /// Ask nothing: upgrade.
        #[arg(long, short)]
        yes: bool,
    },
    /// Hooks agents run; not meant to be called by hand.
    #[command(hide = true)]
    Hook {
        /// The event: `session-start` or `pre-tool-use`.
        event: String,
    },
    /// Start an agent harness, equipped to work in the library: claude,
    /// codex, deepseek or opencode.
    Agent {
        /// The harness.
        #[arg(value_enum)]
        harness: agent::Harness,
        #[command(flatten)]
        args: AgentArgs,
    },
    /// Start Claude Code, equipped to work in the library (`stemma agent claude`).
    Claude(AgentArgs),
    /// Start Codex, equipped to work in the library (`stemma agent codex`).
    Codex(AgentArgs),
}

#[derive(Subcommand)]
enum KeyAction {
    /// Add your signing key to your entry in stemma.toml, on a branch.
    Add {
        /// The public key to add. By default, the one git signs with
        /// (`user.signingkey`).
        #[arg(long)]
        key: Option<PathBuf>,
        /// The member the key is for. By default, you.
        #[arg(long)]
        member: Option<String>,
        /// Do not commit the change.
        #[arg(long)]
        no_commit: bool,
    },
}

#[derive(clap::Args)]
struct AgentArgs {
    /// Print the command instead of running it.
    #[arg(long)]
    dry_run: bool,
    /// Work on this branch: an existing one, or a new working branch
    /// (work/<person>-<topic>) from an up-to-date main.
    #[arg(long, value_name = "NAME")]
    branch: Option<String>,
    /// Work on the current branch.
    #[arg(long, conflicts_with = "branch")]
    here: bool,
    /// Delete your working branches whose content is all in main, here and on
    /// their remote.
    #[arg(long)]
    delete_merged: bool,
    /// Ask nothing: take what stemma would do.
    #[arg(long, short)]
    yes: bool,
    /// Arguments passed on to the agent.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// Starts an agent, or prints how it would be started.
fn start(harness: agent::Harness, args: AgentArgs, json: bool) -> anyhow::Result<bool> {
    let request = branches::Request {
        branch: args.branch,
        here: args.here,
        delete_merged: args.delete_merged,
    };
    let mode = interact::Mode::detect(json, args.yes);
    let launch = agent::prepare(harness, args.args, args.dry_run, &request, mode)?;
    if args.dry_run {
        let env: serde_json::Map<String, serde_json::Value> = launch
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect();
        let value = serde_json::json!({
            "harness": harness.name(), "program": launch.program, "args": launch.args,
            "env": env, "dir": launch.dir, "branch": launch.branch,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(true);
    }
    if let Some(branch) = &launch.branch
        && !mode.interactive()
    {
        ui::note(format!("Working on the branch {branch}."));
    }
    launch.run()
}

fn main() -> ExitCode {
    let command = Cli::command();
    let template = help::template(&command);
    let matches = command.help_template(template).get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => error.exit(),
    };
    let result = match cli.command {
        Commands::Init {
            dir,
            name,
            title,
            no_mathlib,
            publish_site,
            stemma_lean,
            no_git,
            update,
            commit,
            remote,
            push,
            no_forge,
            members,
            yes,
        } => init::init(
            init::Options {
                dir,
                name,
                title,
                mathlib: no_mathlib.then_some(false),
                publish: publish_site.then_some(true),
                stemma_lean,
                git: no_git.then_some(false),
                update: update.then_some(true),
                commit: commit.then_some(true),
                remote,
                push: push.then_some(true),
                forge: no_forge.then_some(false),
                members,
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
            content,
            notes,
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
                content,
                notes,
            },
            cli.json,
        ),
        Commands::Share {
            anyway,
            upto,
            title,
            body,
            foreign,
            no_build,
            yes,
        } => share::share(
            share::Options {
                anyway,
                upto,
                title,
                body,
                yes,
                foreign,
                build: !no_build,
            },
            cli.json,
        ),
        Commands::Sign {
            labels,
            base,
            no_commit,
        } => sign::sign(labels, base, !no_commit),
        Commands::Key {
            action:
                KeyAction::Add {
                    key,
                    member,
                    no_commit,
                },
        } => keys::key_add(key, member, !no_commit, cli.json),
        Commands::Verify { base, no_build } => verify::verify(base, !no_build, cli.json),
        Commands::Upgrade {
            dry_run,
            no_update,
            no_commit,
            no_forge,
            yes,
        } => upgrade::upgrade(
            upgrade::Options {
                dry_run,
                update: !no_update,
                commit: !no_commit,
                forge: !no_forge,
                yes,
            },
            cli.json,
        ),
        Commands::Remote { url, unset } => branches::remote_command(url, unset, cli.json),
        Commands::Hook { event } => agent::hook(&event, cli.json),
        Commands::Agent { harness, args } => start(harness, args, cli.json),
        Commands::Claude(args) => start(agent::Harness::Claude, args, cli.json),
        Commands::Codex(args) => start(agent::Harness::Codex, args, cli.json),
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

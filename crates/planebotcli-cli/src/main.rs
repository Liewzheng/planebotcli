//! `planebotcli` — CLI for Plane.so project management (Rust rewrite).

use std::time::Duration;

use clap::{Parser, Subcommand};
use planebotcli_cache::Cache;
use planebotcli_client::PlaneClient;
use planebotcli_core::{PlaneError, load_config};
use planebotcli_format::{output_json, output_table};
use planebotcli_types::Project;

#[derive(Parser)]
#[command(
    name = "planebotcli",
    version,
    about = "CLI for Plane.so project management."
)]
struct Cli {
    /// Output JSON to stdout instead of a table on stderr.
    #[arg(long, global = true)]
    json: bool,

    /// Bypass the disk cache for this command.
    #[arg(long, global = true)]
    no_cache: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the current authenticated user.
    Whoami,
    /// Manage projects.
    Project {
        #[command(subcommand)]
        command: ProjectCmd,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// List projects.
    #[command(alias = "ls")]
    List,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("Error: {}", err);
        if let Some(hint) = err.hint() {
            eprintln!("Hint: {hint}");
        }
        std::process::exit(err.exit_code());
    }
}

async fn run(cli: Cli) -> Result<(), PlaneError> {
    let cfg = load_config()?;
    let client = PlaneClient::new(&cfg)?;

    match cli.command {
        Command::Whoami => {
            let me = client.get_me().await?;
            if cli.json {
                output_json(&me);
            } else {
                let rows = vec![
                    vec!["id".to_string(), me.id.clone()],
                    vec![
                        "display_name".to_string(),
                        me.display_name.clone().unwrap_or_default(),
                    ],
                    vec![
                        "first_name".to_string(),
                        me.first_name.clone().unwrap_or_default(),
                    ],
                    vec![
                        "last_name".to_string(),
                        me.last_name.clone().unwrap_or_default(),
                    ],
                    vec!["email".to_string(), me.email.clone().unwrap_or_default()],
                ];
                output_table(&["Field", "Value"], &rows);
            }
        }
        Command::Project {
            command: ProjectCmd::List,
        } => {
            let cache = Cache::new();
            let key = format!("projects:{}", cfg.workspace);
            let projects: Vec<Project> = if cli.no_cache {
                let projects = client.list_projects().await?;
                cache.set(&key, &projects);
                projects
            } else {
                match cache.get::<Vec<Project>>(&key, Duration::from_secs(60)) {
                    Some(projects) => projects,
                    None => {
                        let projects = client.list_projects().await?;
                        cache.set(&key, &projects);
                        projects
                    }
                }
            };
            if cli.json {
                output_json(&projects);
            } else {
                let rows: Vec<Vec<String>> = projects
                    .iter()
                    .map(|p| {
                        vec![
                            p.id.clone(),
                            p.name.clone().unwrap_or_default(),
                            p.identifier.clone().unwrap_or_default(),
                            p.created_at.clone().unwrap_or_default(),
                        ]
                    })
                    .collect();
                output_table(&["ID", "Name", "Identifier", "Created"], &rows);
            }
        }
    }
    Ok(())
}

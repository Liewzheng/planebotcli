//! planebotcli — command tree and handlers (Rust rewrite of the Python CLI).

mod render;

use std::collections::HashMap;
use std::time::Duration;

use clap::{Parser, Subcommand};
use planebotcli_cache::Cache;
use planebotcli_client::PlaneClient;
use planebotcli_core::{PlaneError, load_config};
use planebotcli_format::{output_json, output_table};
use planebotcli_resolve::{
    locate_work_item_across, locate_work_item_in_project, resolve_project, resolve_user_query,
};
use planebotcli_types::{
    CommentWrite, Label, LabelWrite, Project, ProjectWrite, State, StateWrite, WorkItem,
    WorkItemWrite,
};
use render::{Lookups, comment_json, work_item_view};
use serde_json::Value;

#[derive(Parser)]
#[command(
    name = "planebotcli",
    version,
    about = "CLI for Plane.so project management."
)]
pub struct Cli {
    /// Output JSON to stdout instead of a table on stderr.
    #[arg(long, global = true)]
    pub json: bool,

    /// Bypass the disk cache for this command.
    #[arg(long, global = true)]
    pub no_cache: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show the current authenticated user.
    Whoami,
    /// Manage projects.
    Project {
        #[command(subcommand)]
        command: ProjectCmd,
    },
    /// Manage work items.
    #[command(
        visible_alias = "work-item",
        visible_alias = "issues",
        visible_alias = "issue"
    )]
    Wi {
        #[command(subcommand)]
        command: WiCmd,
    },
    /// Manage comments on work items.
    Comment {
        #[command(subcommand)]
        command: CommentCmd,
    },
    /// Manage project labels.
    Label {
        #[command(subcommand)]
        command: LabelCmd,
    },
    /// Manage project states.
    State {
        #[command(subcommand)]
        command: StateCmd,
    },
    /// Workspace members.
    User {
        #[command(subcommand)]
        command: UserCmd,
    },
    /// Manage the local disk cache.
    Cache {
        #[command(subcommand)]
        command: CacheCmd,
    },
}

#[derive(Subcommand)]
pub enum UserCmd {
    /// List workspace members.
    #[command(alias = "ls")]
    List,
}

#[derive(Subcommand)]
pub enum CacheCmd {
    /// Clear the disk cache.
    Clear,
}

/// Options shared by every `comment` command.
#[derive(Subcommand)]
pub enum CommentCmd {
    /// List comments on a work item.
    #[command(alias = "ls")]
    List {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Maximum results to show.
        #[arg(long, short = 'l')]
        limit: Option<usize>,
    },
    /// Add a comment to a work item.
    Create {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Comment text (plain text, converted to HTML).
        #[arg(long, short = 'b')]
        body: Option<String>,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Update a comment on a work item.
    Update {
        /// Comment UUID.
        comment_id: String,
        /// Work item identifier (ABC-123), UUID, or name.
        #[arg(long)]
        issue: String,
        /// New comment text (plain text, converted to HTML).
        #[arg(long, short = 'b')]
        body: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Delete a comment from a work item.
    Delete {
        /// Comment UUID.
        comment_id: String,
        /// Work item identifier (ABC-123), UUID, or name.
        #[arg(long)]
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects.
    #[command(alias = "ls")]
    List,
    /// Show project details.
    Show {
        /// Project name, identifier, or UUID.
        project: String,
    },
    /// Create a new project.
    Create {
        /// Project name.
        name: String,
        /// Short project identifier (e.g. FE, BE). Auto-generated if not specified.
        #[arg(long, short = 'i')]
        identifier: Option<String>,
        /// Project description.
        #[arg(long, short = 'd')]
        description: Option<String>,
    },
    /// Update a project.
    Update {
        /// Project name, identifier, or UUID.
        project: String,
        /// New project name.
        #[arg(long)]
        name: Option<String>,
        /// New project identifier.
        #[arg(long, short = 'i')]
        identifier: Option<String>,
        /// New project description.
        #[arg(long, short = 'd')]
        description: Option<String>,
    },
    /// Delete a project.
    Delete {
        /// Project name, identifier, or UUID.
        project: String,
    },
}

#[derive(Subcommand)]
pub enum LabelCmd {
    /// List labels in a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Show label details.
    Show {
        /// Label name or UUID.
        label: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new label.
    Create {
        /// Label name.
        name: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Label color (hex, e.g. #FF0000).
        #[arg(long)]
        color: Option<String>,
    },
    /// Update a label.
    Update {
        /// Label name or UUID.
        label: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New label name.
        #[arg(long)]
        name: Option<String>,
        /// New color (hex, e.g. #FF0000).
        #[arg(long)]
        color: Option<String>,
    },
    /// Delete a label.
    Delete {
        /// Label name or UUID.
        label: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum StateCmd {
    /// List states in a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Show state details.
    Show {
        /// State name or UUID.
        state: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new state.
    Create {
        /// State name.
        name: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// State group: backlog, unstarted, started, completed, cancelled.
        #[arg(long)]
        group: Option<String>,
        /// State color (hex, e.g. #FFA500).
        #[arg(long)]
        color: Option<String>,
    },
    /// Update a state.
    Update {
        /// State name or UUID.
        state: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New state name.
        #[arg(long)]
        name: Option<String>,
        /// New group: backlog, unstarted, started, completed, cancelled.
        #[arg(long)]
        group: Option<String>,
        /// New color (hex, e.g. #FFA500).
        #[arg(long)]
        color: Option<String>,
    },
    /// Delete a state.
    Delete {
        /// State name or UUID.
        state: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum WiCmd {
    /// List work items.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID. If omitted, lists from all projects.
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Filter by assignee name or 'me'.
        #[arg(long)]
        assignee: Option<String>,
        /// Filter by state name (comma-separated).
        #[arg(long)]
        state: Option<String>,
        /// Filter by label name (comma-separated).
        #[arg(long)]
        labels: Option<String>,
        /// Filter by parent work item identifier (ABC-123), UUID, or name.
        #[arg(long)]
        parent: Option<String>,
        /// Sort by: created (default), updated.
        #[arg(long)]
        sort: Option<String>,
        /// Maximum results to show.
        #[arg(long, short = 'l')]
        limit: Option<usize>,
    },
    /// Show work item details.
    Show {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Skip fetching the work item's comments.
        #[arg(long)]
        no_comments: bool,
    },
    /// Create a new work item.
    Create {
        /// Work item title.
        title: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Assignee name, email, or 'me'.
        #[arg(long, visible_alias = "assign")]
        assignee: Option<String>,
        /// State name (e.g. 'Todo', 'In Progress').
        #[arg(long)]
        state: Option<String>,
        /// Comma-separated label names.
        #[arg(long)]
        labels: Option<String>,
        /// Priority: urgent, high, medium, low, none (or 0-4).
        #[arg(long)]
        priority: Option<String>,
        /// Parent work item identifier (ABC-123) for creating sub-issues.
        #[arg(long)]
        parent: Option<String>,
        /// Work item description (plain text, wrapped in a paragraph).
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// Start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// Target end date (YYYY-MM-DD).
        #[arg(long)]
        target_date: Option<String>,
    },
    /// Update a work item.
    Update {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New state name.
        #[arg(long)]
        state: Option<String>,
        /// New priority: urgent, high, medium, low, none.
        #[arg(long)]
        priority: Option<String>,
        /// New assignee name or 'me'.
        #[arg(long, visible_alias = "assign")]
        assignee: Option<String>,
        /// Comma-separated labels to set.
        #[arg(long)]
        labels: Option<String>,
        /// Remove all labels.
        #[arg(long)]
        clear_labels: bool,
        /// New title.
        #[arg(long)]
        name: Option<String>,
        /// New description (plain text, wrapped in a paragraph).
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// New start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// New target end date (YYYY-MM-DD).
        #[arg(long)]
        target_date: Option<String>,
    },
    /// Delete a work item.
    Delete {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Search work items by text.
    Search {
        /// Search query string.
        query: String,
        /// Limit search to a specific project.
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Maximum results to show.
        #[arg(long, short = 'l')]
        limit: Option<usize>,
    },
    /// Assign a work item (defaults to yourself).
    Assign {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Assignee name, email, or 'me' (default).
        #[arg(long, visible_alias = "assign")]
        assignee: Option<String>,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

/// Run the parsed CLI and return the first error (mapped to an exit code).
pub async fn run(cli: Cli) -> Result<(), PlaneError> {
    let cfg = load_config()?;
    let client = PlaneClient::with_cache(&cfg, cli.no_cache)?;

    match cli.command {
        Command::Whoami => cmd_whoami(&client, cli.json).await,
        Command::Project { command } => match command {
            ProjectCmd::List => cmd_project_list(&client, &cfg, cli.no_cache, cli.json).await,
            ProjectCmd::Show { project } => cmd_project_show(&client, &project, cli.json).await,
            ProjectCmd::Create {
                name,
                identifier,
                description,
            } => {
                cmd_project_create(
                    &client,
                    &name,
                    identifier.as_deref(),
                    description.as_deref(),
                    cli.json,
                )
                .await
            }
            ProjectCmd::Update {
                project,
                name,
                identifier,
                description,
            } => {
                cmd_project_update(
                    &client,
                    &project,
                    name.as_deref(),
                    identifier.as_deref(),
                    description.as_deref(),
                    cli.json,
                )
                .await
            }
            ProjectCmd::Delete { project } => cmd_project_delete(&client, &project).await,
        },
        Command::Wi { command } => match command {
            WiCmd::List {
                project,
                assignee,
                state,
                labels,
                parent,
                sort,
                limit,
            } => {
                let opts = WiListOptions {
                    project: project.as_deref(),
                    assignee: assignee.as_deref(),
                    state: state.as_deref(),
                    labels: labels.as_deref(),
                    parent: parent.as_deref(),
                    sort: sort.as_deref(),
                    limit: limit.unwrap_or(50),
                };
                cmd_wi_list(&client, &cfg.base_url, &opts, cli.json).await
            }
            WiCmd::Show {
                issue,
                project,
                no_comments,
            } => {
                cmd_wi_show(
                    &client,
                    &cfg.base_url,
                    &issue,
                    project.as_deref(),
                    no_comments,
                    cli.json,
                )
                .await
            }
            WiCmd::Create {
                title,
                project,
                assignee,
                state,
                labels,
                priority,
                parent,
                description,
                start_date,
                target_date,
            } => {
                let opts = CreateOpts {
                    project: project.as_deref(),
                    assignee: assignee.as_deref(),
                    state: state.as_deref(),
                    labels: labels.as_deref(),
                    priority: priority.as_deref(),
                    parent: parent.as_deref(),
                    description: description.as_deref(),
                    start_date: start_date.as_deref(),
                    target_date: target_date.as_deref(),
                };
                cmd_wi_create(&client, &cfg.base_url, &title, &opts, cli.json).await
            }
            WiCmd::Update {
                issue,
                project,
                state,
                priority,
                assignee,
                labels,
                clear_labels,
                name,
                description,
                start_date,
                target_date,
            } => {
                let opts = UpdateOpts {
                    project: project.as_deref(),
                    state: state.as_deref(),
                    priority: priority.as_deref(),
                    assignee: assignee.as_deref(),
                    labels: labels.as_deref(),
                    clear_labels,
                    name: name.as_deref(),
                    description: description.as_deref(),
                    start_date: start_date.as_deref(),
                    target_date: target_date.as_deref(),
                };
                cmd_wi_update(&client, &cfg.base_url, &issue, &opts, cli.json).await
            }
            WiCmd::Delete { issue, project } => {
                cmd_wi_delete(&client, &issue, project.as_deref()).await
            }
            WiCmd::Search {
                query,
                project,
                limit,
            } => {
                cmd_wi_search(
                    &client,
                    &cfg.base_url,
                    &query,
                    project.as_deref(),
                    limit.unwrap_or(20),
                    cli.json,
                )
                .await
            }
            WiCmd::Assign {
                issue,
                assignee,
                project,
            } => cmd_wi_assign(&client, &issue, assignee.as_deref(), project.as_deref()).await,
        },
        Command::Comment { command } => match command {
            CommentCmd::List {
                issue,
                project,
                limit,
            } => {
                cmd_comment_list(
                    &client,
                    &issue,
                    project.as_deref(),
                    limit.unwrap_or(50),
                    cli.json,
                )
                .await
            }
            CommentCmd::Create {
                issue,
                body,
                project,
            } => {
                let body = body.ok_or_else(|| PlaneError::Validation {
                    message: "Missing --body for comment create.".into(),
                    hint: Some("Example: planebotcli comment create PROJ-1 --body \"text\"".into()),
                })?;
                cmd_comment_write(&client, &issue, project.as_deref(), None, &body, cli.json).await
            }
            CommentCmd::Update {
                comment_id,
                issue,
                body,
                project,
            } => {
                cmd_comment_write(
                    &client,
                    &issue,
                    project.as_deref(),
                    Some(&comment_id),
                    &body,
                    cli.json,
                )
                .await
            }
            CommentCmd::Delete {
                comment_id,
                issue,
                project,
            } => cmd_comment_delete(&client, &issue, project.as_deref(), &comment_id).await,
        },
        Command::Label { command } => match command {
            LabelCmd::List { project } => {
                cmd_label_list(&client, project.as_deref(), cli.json).await
            }
            LabelCmd::Show { label, project } => {
                cmd_label_show(&client, &label, project.as_deref(), cli.json).await
            }
            LabelCmd::Create {
                name,
                project,
                color,
            } => {
                cmd_label_create(
                    &client,
                    &name,
                    project.as_deref(),
                    color.as_deref(),
                    cli.json,
                )
                .await
            }
            LabelCmd::Update {
                label,
                project,
                name,
                color,
            } => {
                cmd_label_update(
                    &client,
                    &label,
                    project.as_deref(),
                    name.as_deref(),
                    color.as_deref(),
                    cli.json,
                )
                .await
            }
            LabelCmd::Delete { label, project } => {
                cmd_label_delete(&client, &label, project.as_deref()).await
            }
        },
        Command::State { command } => match command {
            StateCmd::List { project } => {
                cmd_state_list(&client, project.as_deref(), cli.json).await
            }
            StateCmd::Show { state, project } => {
                cmd_state_show(&client, &state, project.as_deref(), cli.json).await
            }
            StateCmd::Create {
                name,
                project,
                group,
                color,
            } => {
                cmd_state_create(
                    &client,
                    &name,
                    project.as_deref(),
                    group.as_deref(),
                    color.as_deref(),
                    cli.json,
                )
                .await
            }
            StateCmd::Update {
                state,
                project,
                name,
                group,
                color,
            } => {
                cmd_state_update(
                    &client,
                    &state,
                    project.as_deref(),
                    name.as_deref(),
                    group.as_deref(),
                    color.as_deref(),
                    cli.json,
                )
                .await
            }
            StateCmd::Delete { state, project } => {
                cmd_state_delete(&client, &state, project.as_deref()).await
            }
        },
        Command::User { command } => match command {
            UserCmd::List => cmd_user_list(&client, cli.json).await,
        },
        Command::Cache { command } => match command {
            CacheCmd::Clear => {
                Cache::new().clear();
                eprintln!("Cache cleared.");
                Ok(())
            }
        },
    }
}

async fn cmd_whoami(client: &PlaneClient, json: bool) -> Result<(), PlaneError> {
    let me = client.get_me().await?;
    if json {
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
    Ok(())
}

async fn cmd_project_list(
    client: &PlaneClient,
    cfg: &planebotcli_core::Config,
    no_cache: bool,
    json: bool,
) -> Result<(), PlaneError> {
    let cache = Cache::new();
    let key = format!("projects:{}", cfg.workspace);
    let projects: Vec<Project> = if no_cache {
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
    if json {
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
    Ok(())
}

/// Print a single project: JSON to stdout, or a Field/Value table to stderr.
fn output_project_view(project: &Project, json: bool) {
    if json {
        output_json(project);
        return;
    }
    let rows = vec![
        vec!["id".to_string(), project.id.clone()],
        vec![
            "identifier".to_string(),
            project.identifier.clone().unwrap_or_default(),
        ],
        vec!["name".to_string(), project.name.clone().unwrap_or_default()],
        vec![
            "description".to_string(),
            project.description.clone().unwrap_or_default(),
        ],
        vec![
            "created_at".to_string(),
            project.created_at.clone().unwrap_or_default(),
        ],
        vec![
            "updated_at".to_string(),
            project.updated_at.clone().unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

async fn cmd_project_show(
    client: &PlaneClient,
    project: &str,
    json: bool,
) -> Result<(), PlaneError> {
    let project = resolve_project(project, client).await?;
    output_project_view(&project, json);
    Ok(())
}

async fn cmd_project_create(
    client: &PlaneClient,
    name: &str,
    identifier: Option<&str>,
    description: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    // The Plane API requires an identifier on create; mirror the Python CLI's
    // default of the first three name characters uppercased when `-i` is absent.
    let identifier = normalize_identifier(identifier)
        .unwrap_or_else(|| name.chars().take(3).collect::<String>().to_uppercase());
    let write = ProjectWrite {
        name: Some(name.to_string()),
        identifier: Some(identifier),
        description: description.map(str::to_string),
    };
    let created = client.create_project(&write).await?;
    output_project_view(&created, json);
    Ok(())
}

async fn cmd_project_update(
    client: &PlaneClient,
    project: &str,
    name: Option<&str>,
    identifier: Option<&str>,
    description: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let resolved = resolve_project(project, client).await?;
    let mut write = ProjectWrite::default();
    if let Some(name) = name {
        write.name = Some(name.to_string());
    }
    if let Some(identifier) = normalize_identifier(identifier) {
        write.identifier = Some(identifier);
    }
    if let Some(description) = description {
        write.description = Some(description.to_string());
    }
    let updated = client.update_project(&resolved.id, &write).await?;
    output_project_view(&updated, json);
    Ok(())
}

async fn cmd_project_delete(client: &PlaneClient, project: &str) -> Result<(), PlaneError> {
    let resolved = resolve_project(project, client).await?;
    let name = resolved.name.clone().unwrap_or_else(|| resolved.id.clone());
    client.delete_project(&resolved.id).await?;
    eprintln!("Project '{name}' deleted.");
    Ok(())
}

/// Uppercase a user-supplied project identifier; None/empty means "not given".
fn normalize_identifier(identifier: Option<&str>) -> Option<String> {
    identifier
        .filter(|i| !i.is_empty())
        .map(|i| i.to_uppercase())
}

/// Options for `wi ls` (grouped to keep the handler signature small).
struct WiListOptions<'a> {
    project: Option<&'a str>,
    assignee: Option<&'a str>,
    state: Option<&'a str>,
    labels: Option<&'a str>,
    parent: Option<&'a str>,
    sort: Option<&'a str>,
    limit: usize,
}

async fn cmd_wi_list(
    client: &PlaneClient,
    base_url: &str,
    opts: &WiListOptions<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = opts.project;
    let assignee = opts.assignee;
    let state = opts.state;
    let labels = opts.labels;
    let parent = opts.parent;
    let sort = opts.sort;
    let limit = opts.limit;
    let workspace = client.workspace().to_string();

    // Member map (workspace-scoped) for assignee name resolution and filters.
    let member_map: HashMap<String, String> = client
        .list_members()
        .await?
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();

    let projects: Vec<Project> = match project {
        Some(q) => vec![resolve_project(q, client).await?],
        None => client.list_projects().await?,
    };

    // Each row keeps the raw item (for uuid-based filters) and its rendered view.
    let mut rows: Vec<(WorkItem, serde_json::Value)> = Vec::new();
    let multi = projects.len() > 1;
    for (i, proj) in projects.iter().enumerate() {
        if multi && i > 0 {
            // Cross-project listings fire many requests; pace them to stay
            // under the instance rate limit (the Python line relied on its
            // disk cache for the same reason).
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        let project_id = proj.id.clone();
        let identifier = proj.identifier.clone().unwrap_or_default();
        let items = client.list_work_items(&project_id).await?;
        let states = client.list_states(&project_id).await?;
        let labels_list = client.list_labels(&project_id).await?;
        let state_map: HashMap<String, String> = states
            .into_iter()
            .map(|s| (s.id, s.name.unwrap_or_default()))
            .collect();
        let label_map: HashMap<String, String> = labels_list
            .into_iter()
            .map(|l| (l.id, l.name.unwrap_or_default()))
            .collect();
        let lookups = Lookups {
            state_map,
            label_map,
            member_map: member_map.clone(),
        };
        rows.extend(items.into_iter().map(|item| {
            let view = work_item_view(&item, &identifier, &lookups, base_url, &workspace);
            (item, view)
        }));
    }

    // --assignee filter (by member id, exact)
    if let Some(q) = assignee {
        let (user_id, _) = resolve_user_query(q, client).await?;
        rows.retain(|(item, _)| work_item_has_assignee(item, &user_id));
    }
    // --state filter: comma tokens, substring match on the resolved state name
    if let Some(q) = state {
        let tokens: Vec<String> = q
            .split(',')
            .filter_map(|t| {
                let t = t.trim().to_lowercase();
                (!t.is_empty()).then_some(t)
            })
            .collect();
        if !tokens.is_empty() {
            rows.retain(|(_, view)| {
                let name = view["state_detail_name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase();
                tokens.iter().any(|t| name.contains(t.as_str()))
            });
        }
    }
    // --labels filter: comma tokens, substring match on resolved label names
    if let Some(q) = labels {
        let tokens: Vec<String> = q
            .split(',')
            .filter_map(|t| {
                let t = t.trim().to_lowercase();
                (!t.is_empty()).then_some(t)
            })
            .collect();
        if !tokens.is_empty() {
            rows.retain(|(_, view)| {
                let names = view["label_detail_names"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_lowercase())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                tokens
                    .iter()
                    .any(|t| names.iter().any(|n| n.contains(t.as_str())))
            });
        }
    }
    // --parent filter: resolve the parent reference among the fetched rows
    if let Some(q) = parent {
        let parent_id = resolve_parent_id(q, &rows)?;
        rows.retain(|(item, _)| item.parent.as_deref() == Some(parent_id.as_str()));
    }

    // Sort: created (default) or updated, newest first.
    let updated = sort == Some("updated");
    rows.sort_by(|(a, _), (b, _)| {
        let key = |it: &WorkItem| {
            if updated {
                it.updated_at.clone().unwrap_or_default()
            } else {
                it.created_at.clone().unwrap_or_default()
            }
        };
        key(b).cmp(&key(a))
    });
    rows.truncate(limit);

    if json {
        let values: Vec<serde_json::Value> = rows.into_iter().map(|(_, v)| v).collect();
        output_json(&values);
    } else {
        let table_rows: Vec<Vec<String>> = rows
            .into_iter()
            .map(|(_, v)| {
                vec![
                    v["sequence_id"].as_str().unwrap_or("").to_string(),
                    v["name"].as_str().unwrap_or("").to_string(),
                    v["priority"].as_str().unwrap_or("").to_string(),
                    v["state_detail_name"].as_str().unwrap_or("").to_string(),
                    v["assignee_names"].as_str().unwrap_or("").to_string(),
                    fmt_ts(v["created_at"].as_str().unwrap_or("")),
                ]
            })
            .collect();
        output_table(
            &["ID", "Title", "Priority", "State", "Assignees", "Created"],
            &table_rows,
        );
    }
    Ok(())
}

async fn cmd_wi_show(
    client: &PlaneClient,
    base_url: &str,
    issue: &str,
    project: Option<&str>,
    no_comments: bool,
    json: bool,
) -> Result<(), PlaneError> {
    let workspace = client.workspace().to_string();

    // Locate the item: project-scoped when -p is given, otherwise across all.
    let located = match project {
        Some(p) => {
            let proj = resolve_project(p, client).await?;
            let mut found = locate_work_item_in_project(issue, &proj, client).await?;
            if found.project_id.is_empty() {
                found.project_id = proj.id.clone();
            }
            found
        }
        None => locate_work_item_across(issue, client).await?,
    };

    let detail = client
        .get_work_item(&located.project_id, &located.item.id)
        .await?;
    let states = client.list_states(&located.project_id).await?;
    let label_rows = client.list_labels(&located.project_id).await?;
    let members = client.list_members().await?;
    let state_map: HashMap<String, String> = states
        .into_iter()
        .map(|s| (s.id, s.name.unwrap_or_default()))
        .collect();
    let label_map: HashMap<String, String> = label_rows
        .into_iter()
        .map(|l| (l.id, l.name.unwrap_or_default()))
        .collect();
    let member_map: HashMap<String, String> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();
    let lookups = Lookups {
        state_map,
        label_map,
        member_map,
    };
    let mut view = work_item_view(
        &detail,
        &located.project_identifier,
        &lookups,
        base_url,
        &workspace,
    );

    // Comments are a secondary enrichment: a fetch failure degrades to null
    // instead of aborting the command.
    if !no_comments {
        match client.list_comments(&located.project_id, &detail.id).await {
            Ok(comments) => {
                let items: Vec<Value> = comments
                    .iter()
                    .map(|c| comment_json(c, &lookups.member_map))
                    .collect();
                view["comments"] = Value::Array(items);
            }
            Err(e) => {
                view["comments"] = Value::Null;
                eprintln!("[warn] failed to load comments: {e}");
            }
        }
    }

    if json {
        output_json(&view);
    } else {
        let fields: Vec<(&str, &str)> = vec![
            ("id", "UUID"),
            ("sequence_id", "Sequence ID"),
            ("name", "Title"),
            ("description_stripped", "Description"),
            ("priority", "Priority"),
            ("state_detail_name", "State"),
            ("assignee_names", "Assignees"),
            ("label_names", "Labels"),
            ("estimate_display", "Estimate"),
            ("start_date", "Start Date"),
            ("target_date", "Target Date"),
            ("created_at", "Created"),
            ("updated_at", "Updated"),
            ("web_url", "Web URL"),
        ];
        let rows: Vec<Vec<String>> = fields
            .iter()
            .map(|(key, _)| {
                vec![
                    key.to_string(),
                    view[key].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        output_table(&["Field", "Value"], &rows);

        if !no_comments {
            eprintln!();
            match view.get("comments") {
                Some(Value::Array(items)) if !items.is_empty() => {
                    let comment_rows: Vec<Vec<String>> = items
                        .iter()
                        .map(|c| {
                            vec![
                                fmt_ts(c["created_at"].as_str().unwrap_or("")),
                                c["actor_name"].as_str().unwrap_or("").to_string(),
                                c["comment_stripped"].as_str().unwrap_or("").to_string(),
                            ]
                        })
                        .collect();
                    output_table(&["Created", "Actor", "Comment"], &comment_rows);
                }
                Some(Value::Array(_)) => eprintln!("Comments: (none)"),
                _ => eprintln!("Comments: (failed to load)"),
            }
        }
    }
    Ok(())
}

/// Options for `wi create` (grouped to keep handler signatures small).
struct CreateOpts<'a> {
    project: Option<&'a str>,
    assignee: Option<&'a str>,
    state: Option<&'a str>,
    labels: Option<&'a str>,
    priority: Option<&'a str>,
    parent: Option<&'a str>,
    description: Option<&'a str>,
    start_date: Option<&'a str>,
    target_date: Option<&'a str>,
}

/// Options for `wi update`.
struct UpdateOpts<'a> {
    project: Option<&'a str>,
    state: Option<&'a str>,
    priority: Option<&'a str>,
    assignee: Option<&'a str>,
    labels: Option<&'a str>,
    clear_labels: bool,
    name: Option<&'a str>,
    description: Option<&'a str>,
    start_date: Option<&'a str>,
    target_date: Option<&'a str>,
}

/// Validate a YYYY-MM-DD date flag; None passes through.
fn validate_date(value: Option<&str>, flag: &str) -> Result<Option<String>, PlaneError> {
    let Some(value) = value else { return Ok(None) };
    let ok = value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value[..4].chars().all(|c| c.is_ascii_digit())
        && value[5..7].chars().all(|c| c.is_ascii_digit())
        && value[8..10].chars().all(|c| c.is_ascii_digit());
    if !ok {
        return Err(PlaneError::Validation {
            message: format!("Invalid date for {flag}: {value:?} (expected YYYY-MM-DD)."),
            hint: Some(format!("Example: {flag} 2026-09-15")),
        });
    }
    Ok(Some(value.to_string()))
}

/// Normalize and validate a priority (word or 0-4); error lists valid options.
fn normalize_priority(raw: Option<&str>) -> Result<Option<String>, PlaneError> {
    let Some(raw) = raw else { return Ok(None) };
    let canonical = match raw.trim().to_lowercase().as_str() {
        "0" | "none" => "none",
        "1" | "urgent" => "urgent",
        "2" | "high" => "high",
        "3" | "medium" => "medium",
        "4" | "low" => "low",
        other => {
            return Err(PlaneError::Validation {
                message: format!("Invalid priority: {other:?}"),
                hint: Some("Valid values: urgent, high, medium, low, none (or 1-4, 0).".into()),
            });
        }
    };
    Ok(Some(canonical.to_string()))
}

/// Check `--state`/`--labels` against the project and resolve them to ids,
/// mirroring the Python write-before validation: a missing name fails with the
/// available list instead of a bare not-found error.
async fn resolve_state_label_ids(
    client: &PlaneClient,
    project_id: &str,
    state: Option<&str>,
    labels: Option<&str>,
) -> Result<(Option<String>, Vec<String>), PlaneError> {
    let available_states = client.list_states(project_id).await?;
    let available_labels = client.list_labels(project_id).await?;
    let project_identifier = client
        .get_project(project_id)
        .await?
        .identifier
        .unwrap_or_else(|| project_id.to_string());

    let state_id = match state {
        Some(query) if !query.trim().is_empty() => {
            let query = query.trim();
            let found = if planebotcli_resolve::is_uuid(query) {
                available_states.iter().find(|s| s.id == query)
            } else {
                planebotcli_resolve::find_best_match(query, &available_states, |s| {
                    s.name.as_deref().unwrap_or("")
                })
                .map(|m| m.item)
            };
            match found {
                Some(s) => Some(s.id.clone()),
                None => {
                    let names: Vec<&str> = available_states
                        .iter()
                        .filter_map(|s| s.name.as_deref())
                        .collect();
                    return Err(PlaneError::Validation {
                        message: format!(
                            "State '{query}' not found in project {project_identifier}. Available: {}",
                            names.join(", ")
                        ),
                        hint: None,
                    });
                }
            }
        }
        _ => None,
    };

    let mut label_ids = Vec::new();
    if let Some(labels) = labels {
        for raw in labels.split(',') {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            let found = if planebotcli_resolve::is_uuid(name) {
                available_labels.iter().find(|l| l.id == name)
            } else {
                planebotcli_resolve::find_best_match(name, &available_labels, |l| {
                    l.name.as_deref().unwrap_or("")
                })
                .map(|m| m.item)
            };
            match found {
                Some(l) => label_ids.push(l.id.clone()),
                None => {
                    let names: Vec<&str> = available_labels
                        .iter()
                        .filter_map(|l| l.name.as_deref())
                        .collect();
                    return Err(PlaneError::Validation {
                        message: format!(
                            "Label '{name}' not found in project {project_identifier}. Available: {}",
                            names.join(", ")
                        ),
                        hint: None,
                    });
                }
            }
        }
    }
    Ok((state_id, label_ids))
}

async fn cmd_wi_create(
    client: &PlaneClient,
    base_url: &str,
    title: &str,
    opts: &CreateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let project = opts.project.ok_or_else(|| PlaneError::Validation {
        message: "Project is required for this command.".into(),
        hint: Some("Use -p/--project <name-or-id> to specify the project.".into()),
    })?;
    let proj = resolve_project(project, client).await?;
    let project_id = proj.id.clone();
    let project_identifier = proj.identifier.clone().unwrap_or_default();

    let start_date = validate_date(opts.start_date, "--start-date")?;
    let target_date = validate_date(opts.target_date, "--target-date")?;
    let priority = normalize_priority(opts.priority)?;
    let (state_id, label_ids) =
        resolve_state_label_ids(client, &project_id, opts.state, opts.labels).await?;

    let mut write = WorkItemWrite {
        name: Some(title.to_string()),
        ..Default::default()
    };
    if let Some(description) = opts.description {
        write.description_html = Some(format!("<p>{description}</p>"));
    }
    if let Some(p) = priority {
        write.priority = Some(p);
    }
    if let Some(state) = state_id {
        write.state = Some(state);
    }
    if opts.labels.is_some() {
        write.labels = Some(label_ids);
    }
    if let Some(assignee) = opts.assignee {
        let (user_id, _) = resolve_user_query(assignee, client).await?;
        write.assignees = Some(vec![user_id]);
    }
    if let Some(parent) = opts.parent {
        let located = locate_work_item_in_project(parent, &proj, client).await?;
        write.parent = Some(located.item.id);
    }
    if let Some(d) = start_date {
        write.start_date = Some(d);
    }
    if let Some(d) = target_date {
        write.target_date = Some(d);
    }

    let created = client.create_work_item(&project_id, &write).await?;
    output_work_item_view(
        client,
        base_url,
        &created,
        &project_identifier,
        json,
        "Work Item Created",
    )
    .await
}

async fn cmd_wi_update(
    client: &PlaneClient,
    base_url: &str,
    issue: &str,
    opts: &UpdateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, opts.project).await?;
    let project_id = located.project_id.clone();
    let project_identifier = located.project_identifier.clone();

    let start_date = validate_date(opts.start_date, "--start-date")?;
    let target_date = validate_date(opts.target_date, "--target-date")?;
    let priority = normalize_priority(opts.priority)?;
    let labels_query = if opts.clear_labels { None } else { opts.labels };
    let (state_id, label_ids) =
        resolve_state_label_ids(client, &project_id, opts.state, labels_query).await?;

    let mut write = WorkItemWrite::default();
    if let Some(name) = opts.name {
        write.name = Some(name.to_string());
    }
    if let Some(description) = opts.description {
        write.description_html = Some(format!("<p>{description}</p>"));
    }
    if let Some(p) = priority {
        write.priority = Some(p);
    }
    if let Some(state) = state_id {
        write.state = Some(state);
    }
    if let Some(assignee) = opts.assignee {
        let (user_id, _) = resolve_user_query(assignee, client).await?;
        write.assignees = Some(vec![user_id]);
    }
    if opts.labels.is_some() || opts.clear_labels {
        write.labels = Some(label_ids);
    }
    if let Some(d) = start_date.as_ref() {
        write.start_date = Some(d.clone());
    }
    if let Some(d) = target_date.as_ref() {
        write.target_date = Some(d.clone());
    }

    let updated = client
        .update_work_item(&project_id, &located.item.id, &write)
        .await?;

    // Read-back verification (ADR-0007): the API can answer 200 while ignoring
    // date fields; confirm the server applied what we asked.
    for (flag, asked, got) in [
        ("--start-date", start_date, updated.start_date.clone()),
        ("--target-date", target_date, updated.target_date.clone()),
    ] {
        if let Some(asked) = asked
            && got.as_deref() != Some(asked.as_str())
        {
            return Err(PlaneError::Api {
                message: format!("The server did not apply {flag}: asked {asked:?}, got {got:?}."),
            });
        }
    }

    output_work_item_view(
        client,
        base_url,
        &updated,
        &project_identifier,
        json,
        "Work Item Updated",
    )
    .await
}

/// Enrich and print a freshly written work item (maps fetched for names).
async fn output_work_item_view(
    client: &PlaneClient,
    base_url: &str,
    item: &planebotcli_types::WorkItem,
    project_identifier: &str,
    json: bool,
    _title: &str,
) -> Result<(), PlaneError> {
    let workspace = client.workspace().to_string();
    let states = client
        .list_states(item.project.as_deref().unwrap_or(""))
        .await?;
    let labels_list = client
        .list_labels(item.project.as_deref().unwrap_or(""))
        .await?;
    let members = client.list_members().await?;
    let state_map: HashMap<String, String> = states
        .into_iter()
        .map(|s| (s.id, s.name.unwrap_or_default()))
        .collect();
    let label_map: HashMap<String, String> = labels_list
        .into_iter()
        .map(|l| (l.id, l.name.unwrap_or_default()))
        .collect();
    let member_map: HashMap<String, String> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();
    let lookups = Lookups {
        state_map,
        label_map,
        member_map,
    };
    let view = work_item_view(item, project_identifier, &lookups, base_url, &workspace);
    if json {
        output_json(&view);
    } else {
        let fields: Vec<(&str, &str)> = vec![
            ("id", "UUID"),
            ("sequence_id", "Sequence ID"),
            ("name", "Title"),
            ("priority", "Priority"),
            ("state_detail_name", "State"),
            ("assignee_names", "Assignees"),
            ("label_names", "Labels"),
            ("start_date", "Start Date"),
            ("target_date", "Target Date"),
        ];
        let rows: Vec<Vec<String>> = fields
            .iter()
            .map(|(key, _)| {
                vec![
                    key.to_string(),
                    view[key].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        output_table(&["Field", "Value"], &rows);
    }
    Ok(())
}

/// Locate a work item reference, project-scoped when `-p` is given.
async fn locate_issue(
    client: &PlaneClient,
    issue: &str,
    project: Option<&str>,
) -> Result<planebotcli_resolve::LocatedWorkItem, PlaneError> {
    match project {
        Some(p) => {
            let proj = resolve_project(p, client).await?;
            let mut found = locate_work_item_in_project(issue, &proj, client).await?;
            if found.project_id.is_empty() {
                found.project_id = proj.id.clone();
            }
            Ok(found)
        }
        None => locate_work_item_across(issue, client).await,
    }
}

async fn cmd_comment_list(
    client: &PlaneClient,
    issue: &str,
    project: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let mut comments = client
        .list_comments(&located.project_id, &located.item.id)
        .await?;
    let members = client.list_members().await?;
    let member_map: HashMap<String, String> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();

    // Newest `limit`, rendered oldest → newest.
    comments.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let start = comments.len().saturating_sub(limit);
    let views: Vec<Value> = comments[start..]
        .iter()
        .map(|c| comment_json(c, &member_map))
        .collect();

    if json {
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = views
            .iter()
            .map(|c| {
                vec![
                    fmt_ts(c["created_at"].as_str().unwrap_or("")),
                    c["actor_name"].as_str().unwrap_or("").to_string(),
                    c["comment_stripped"].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        output_table(&["Created", "Actor", "Comment"], &rows);
    }
    Ok(())
}

/// Create or update a comment (create when `comment_id` is None).
async fn cmd_comment_write(
    client: &PlaneClient,
    issue: &str,
    project: Option<&str>,
    comment_id: Option<&str>,
    body: &str,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let comment_html = planebotcli_html::body_to_html(body);
    let write = CommentWrite { comment_html };
    let created = match comment_id {
        None => {
            client
                .create_comment(&located.project_id, &located.item.id, &write)
                .await?
        }
        Some(cid) => {
            client
                .update_comment(&located.project_id, &located.item.id, cid, &write)
                .await?
        }
    };
    let members = client.list_members().await?;
    let member_map: HashMap<String, String> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();
    let view = comment_json(&created, &member_map);
    if json {
        output_json(&view);
    } else {
        eprintln!(
            "Comment {} {}.",
            view["id"],
            if comment_id.is_none() {
                "added"
            } else {
                "updated"
            }
        );
    }
    Ok(())
}

async fn cmd_comment_delete(
    client: &PlaneClient,
    issue: &str,
    project: Option<&str>,
    comment_id: &str,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    client
        .delete_comment(&located.project_id, &located.item.id, comment_id)
        .await?;
    eprintln!("Comment {comment_id} deleted.");
    Ok(())
}

/// Does the raw work item carry the given member among its assignees?
fn work_item_has_assignee(item: &WorkItem, user_id: &str) -> bool {
    let Some(Value::Array(items)) = item.assignees.as_ref() else {
        return false;
    };
    items.iter().any(|v| match v {
        Value::String(id) => id == user_id,
        Value::Object(o) => o.get("id").and_then(Value::as_str) == Some(user_id),
        _ => false,
    })
}

/// Resolve a `--parent` reference (UUID, full identifier, or fuzzy name) to a
/// work item UUID using the already-fetched rows.
fn resolve_parent_id(
    query: &str,
    rows: &[(WorkItem, serde_json::Value)],
) -> Result<String, PlaneError> {
    if planebotcli_resolve::is_uuid(query) {
        return Ok(query.to_string());
    }
    let lower = query.to_lowercase();
    for (item, view) in rows {
        if view["sequence_id"]
            .as_str()
            .map(|s| s.eq_ignore_ascii_case(query))
            .unwrap_or(false)
        {
            return Ok(item.id.clone());
        }
        if item
            .name
            .as_deref()
            .map(|n| n.to_lowercase().contains(&lower))
            .unwrap_or(false)
        {
            return Ok(item.id.clone());
        }
    }
    Err(PlaneError::NotFound {
        message: format!("Parent work item not found in the listed items: {query}"),
    })
}

/// ISO `2026-09-08T04:54:06.446Z` → `2026-09-08 04:54:06` for table display.
fn fmt_ts(s: &str) -> String {
    let bytes = s.as_bytes();
    if s.len() >= 20 && bytes[10] == b'T' && s.ends_with('Z') {
        s[..19].replace('T', " ")
    } else {
        s.to_string()
    }
}

async fn cmd_wi_delete(
    client: &PlaneClient,
    issue: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let name = located
        .item
        .name
        .clone()
        .unwrap_or_else(|| located.item.id.clone());
    client
        .delete_work_item(&located.project_id, &located.item.id)
        .await?;
    eprintln!("Work item '{name}' deleted.");
    Ok(())
}

async fn cmd_wi_search(
    client: &PlaneClient,
    base_url: &str,
    query: &str,
    _project: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<(), PlaneError> {
    let workspace = client.workspace().to_string();
    let members = client.list_members().await?;
    let member_map: HashMap<String, String> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m.id, name)
        })
        .collect();
    let items = client.search_work_items(query).await?;
    let lookups = Lookups {
        state_map: HashMap::new(),
        label_map: HashMap::new(),
        member_map,
    };
    let views: Vec<Value> = items
        .iter()
        .take(limit)
        .map(|item| {
            let identifier = item
                .project_detail
                .as_ref()
                .and_then(|d| d.identifier.clone())
                .unwrap_or_default();
            work_item_view(item, &identifier, &lookups, base_url, &workspace)
        })
        .collect();
    if json {
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = views
            .iter()
            .map(|v| {
                vec![
                    v["sequence_id"].as_str().unwrap_or("").to_string(),
                    v["name"].as_str().unwrap_or("").to_string(),
                    v["priority"].as_str().unwrap_or("").to_string(),
                    v["state_detail_name"].as_str().unwrap_or("").to_string(),
                    v["assignee_names"].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        output_table(&["ID", "Title", "Priority", "State", "Assignees"], &rows);
    }
    Ok(())
}

async fn cmd_wi_assign(
    client: &PlaneClient,
    issue: &str,
    assignee: Option<&str>,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let (user_id, user_name) = resolve_user_query(assignee.unwrap_or("me"), client).await?;
    let write = WorkItemWrite {
        assignees: Some(vec![user_id]),
        ..Default::default()
    };
    client
        .update_work_item(&located.project_id, &located.item.id, &write)
        .await?;
    eprintln!("Work item assigned to {user_name}.");
    Ok(())
}

async fn cmd_user_list(client: &PlaneClient, json: bool) -> Result<(), PlaneError> {
    let members = client.list_members().await?;
    if json {
        output_json(&members);
    } else {
        let rows: Vec<Vec<String>> = members
            .iter()
            .map(|m| {
                vec![
                    m.id.clone(),
                    m.full_name(),
                    m.email.clone().unwrap_or_default(),
                ]
            })
            .collect();
        output_table(&["ID", "Name", "Email"], &rows);
    }
    Ok(())
}

/// Resolve the `-p` flag shared by every project-scoped command; the same
/// Validation error `wi create` raises when the project is missing.
async fn require_project(
    client: &PlaneClient,
    project: Option<&str>,
) -> Result<Project, PlaneError> {
    let project = project.ok_or_else(|| PlaneError::Validation {
        message: "Project is required for this command.".into(),
        hint: Some("Use -p/--project <name-or-id> to specify the project.".into()),
    })?;
    resolve_project(project, client).await
}

/// Resolve a label by UUID or fuzzy name against a project's labels.
async fn resolve_label(
    client: &PlaneClient,
    project_id: &str,
    query: &str,
) -> Result<Label, PlaneError> {
    let labels = client.list_labels(project_id).await?;
    let found = if planebotcli_resolve::is_uuid(query) {
        labels.iter().find(|l| l.id.eq_ignore_ascii_case(query))
    } else {
        planebotcli_resolve::find_best_match(query, &labels, |l| l.name.as_deref().unwrap_or(""))
            .map(|m| m.item)
    };
    match found {
        Some(label) => Ok(label.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Label not found: {query}"),
        }),
    }
}

/// Resolve a state by UUID or fuzzy name against a project's states.
async fn resolve_state(
    client: &PlaneClient,
    project_id: &str,
    query: &str,
) -> Result<State, PlaneError> {
    let states = client.list_states(project_id).await?;
    let found = if planebotcli_resolve::is_uuid(query) {
        states.iter().find(|s| s.id.eq_ignore_ascii_case(query))
    } else {
        planebotcli_resolve::find_best_match(query, &states, |s| s.name.as_deref().unwrap_or(""))
            .map(|m| m.item)
    };
    match found {
        Some(state) => Ok(state.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("State not found: {query}"),
        }),
    }
}

/// Valid state group values (Plane's built-in groups).
const STATE_GROUPS: [&str; 5] = ["backlog", "unstarted", "started", "completed", "cancelled"];

/// Validate a `--group` flag; returns the canonical lowercased value.
fn validate_group(value: Option<&str>) -> Result<Option<String>, PlaneError> {
    let Some(value) = value else { return Ok(None) };
    let trimmed = value.trim().to_lowercase();
    if !STATE_GROUPS.contains(&trimmed.as_str()) {
        return Err(PlaneError::Validation {
            message: format!("Invalid state group: {value:?}"),
            hint: Some(format!("Valid values: {}.", STATE_GROUPS.join(", "))),
        });
    }
    Ok(Some(trimmed))
}

/// Print a single label: JSON to stdout, or a Field/Value table to stderr.
fn output_label_view(label: &Label, json: bool) {
    if json {
        output_json(label);
        return;
    }
    let rows = vec![
        vec!["id".to_string(), label.id.clone()],
        vec!["name".to_string(), label.name.clone().unwrap_or_default()],
        vec!["color".to_string(), label.color.clone().unwrap_or_default()],
        vec![
            "description".to_string(),
            label.description.clone().unwrap_or_default(),
        ],
        vec![
            "parent".to_string(),
            label.parent.clone().unwrap_or_default(),
        ],
        vec![
            "created_at".to_string(),
            label.created_at.clone().unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

/// Print a single state: JSON to stdout, or a Field/Value table to stderr.
fn output_state_view(state: &State, json: bool) {
    if json {
        output_json(state);
        return;
    }
    let sequence = state
        .sequence
        .as_ref()
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    let rows = vec![
        vec!["id".to_string(), state.id.clone()],
        vec!["name".to_string(), state.name.clone().unwrap_or_default()],
        vec!["group".to_string(), state.group.clone().unwrap_or_default()],
        vec!["color".to_string(), state.color.clone().unwrap_or_default()],
        vec!["sequence".to_string(), sequence],
        vec![
            "created_at".to_string(),
            state.created_at.clone().unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

async fn cmd_label_list(
    client: &PlaneClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let labels = client.list_labels(&proj.id).await?;
    if json {
        output_json(&labels);
    } else {
        let rows: Vec<Vec<String>> = labels
            .iter()
            .map(|l| {
                vec![
                    l.id.clone(),
                    l.name.clone().unwrap_or_default(),
                    l.color.clone().unwrap_or_default(),
                    l.description.clone().unwrap_or_default(),
                ]
            })
            .collect();
        output_table(&["ID", "Name", "Color", "Description"], &rows);
    }
    Ok(())
}

async fn cmd_label_show(
    client: &PlaneClient,
    label: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let label = resolve_label(client, &proj.id, label).await?;
    output_label_view(&label, json);
    Ok(())
}

async fn cmd_label_create(
    client: &PlaneClient,
    name: &str,
    project: Option<&str>,
    color: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let mut write = LabelWrite {
        name: Some(name.to_string()),
        ..Default::default()
    };
    if let Some(color) = color {
        write.color = Some(color.to_string());
    }
    let created = client.create_label(&proj.id, &write).await?;
    output_label_view(&created, json);
    Ok(())
}

async fn cmd_label_update(
    client: &PlaneClient,
    label: &str,
    project: Option<&str>,
    name: Option<&str>,
    color: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_label(client, &proj.id, label).await?;
    let mut write = LabelWrite::default();
    if let Some(name) = name {
        write.name = Some(name.to_string());
    }
    if let Some(color) = color {
        write.color = Some(color.to_string());
    }
    let updated = client.update_label(&proj.id, &found.id, &write).await?;
    output_label_view(&updated, json);
    Ok(())
}

async fn cmd_label_delete(
    client: &PlaneClient,
    label: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_label(client, &proj.id, label).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    client.delete_label(&proj.id, &found.id).await?;
    eprintln!("Label '{name}' deleted.");
    Ok(())
}

async fn cmd_state_list(
    client: &PlaneClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let states = client.list_states(&proj.id).await?;
    if json {
        output_json(&states);
    } else {
        let rows: Vec<Vec<String>> = states
            .iter()
            .map(|s| {
                let sequence = s
                    .sequence
                    .as_ref()
                    .map(|v| match v {
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => s.clone(),
                        _ => String::new(),
                    })
                    .unwrap_or_default();
                vec![
                    s.id.clone(),
                    s.name.clone().unwrap_or_default(),
                    s.color.clone().unwrap_or_default(),
                    s.group.clone().unwrap_or_default(),
                    sequence,
                ]
            })
            .collect();
        output_table(&["ID", "Name", "Color", "Group", "Sequence"], &rows);
    }
    Ok(())
}

async fn cmd_state_show(
    client: &PlaneClient,
    state: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let state = resolve_state(client, &proj.id, state).await?;
    output_state_view(&state, json);
    Ok(())
}

async fn cmd_state_create(
    client: &PlaneClient,
    name: &str,
    project: Option<&str>,
    group: Option<&str>,
    color: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let group = validate_group(group)?;
    let mut write = StateWrite {
        name: Some(name.to_string()),
        color: Some(color.unwrap_or("#000000").to_string()),
        ..Default::default()
    };
    if let Some(group) = group {
        write.group = Some(group);
    }
    let created = client.create_state(&proj.id, &write).await?;
    output_state_view(&created, json);
    Ok(())
}

async fn cmd_state_update(
    client: &PlaneClient,
    state: &str,
    project: Option<&str>,
    name: Option<&str>,
    group: Option<&str>,
    color: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_state(client, &proj.id, state).await?;
    let group = validate_group(group)?;
    let mut write = StateWrite::default();
    if let Some(name) = name {
        write.name = Some(name.to_string());
    }
    if let Some(group) = group {
        write.group = Some(group);
    }
    if let Some(color) = color {
        write.color = Some(color.to_string());
    }
    let updated = client.update_state(&proj.id, &found.id, &write).await?;
    output_state_view(&updated, json);
    Ok(())
}

async fn cmd_state_delete(
    client: &PlaneClient,
    state: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_state(client, &proj.id, state).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    client.delete_state(&proj.id, &found.id).await?;
    eprintln!("State '{name}' deleted.");
    Ok(())
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_validation() {
        assert_eq!(
            validate_date(Some("2026-09-15"), "--target-date")
                .unwrap()
                .as_deref(),
            Some("2026-09-15")
        );
        assert!(validate_date(None, "--target-date").unwrap().is_none());
        assert!(validate_date(Some("2026/09/15"), "--target-date").is_err());
        assert!(validate_date(Some("15-09-2026"), "--target-date").is_err());
    }

    #[test]
    fn priority_normalization() {
        assert_eq!(
            normalize_priority(Some("urgent")).unwrap().as_deref(),
            Some("urgent")
        );
        assert_eq!(
            normalize_priority(Some("2")).unwrap().as_deref(),
            Some("high")
        );
        assert_eq!(
            normalize_priority(Some("none")).unwrap().as_deref(),
            Some("none")
        );
        assert_eq!(normalize_priority(None).unwrap(), None);
        assert!(normalize_priority(Some("bogus")).is_err());
    }
}

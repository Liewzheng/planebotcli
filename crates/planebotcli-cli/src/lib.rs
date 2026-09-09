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
use planebotcli_types::{Project, WorkItem};
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
}

#[derive(Subcommand)]
pub enum ProjectCmd {
    /// List projects.
    #[command(alias = "ls")]
    List,
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
}

/// Run the parsed CLI and return the first error (mapped to an exit code).
pub async fn run(cli: Cli) -> Result<(), PlaneError> {
    let cfg = load_config()?;
    let client = PlaneClient::new(&cfg)?;

    match cli.command {
        Command::Whoami => cmd_whoami(&client, cli.json).await,
        Command::Project { command } => match command {
            ProjectCmd::List => cmd_project_list(&client, &cfg, cli.no_cache, cli.json).await,
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

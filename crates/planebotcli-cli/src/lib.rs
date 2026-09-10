//! planebotcli — command tree and handlers (Rust rewrite of the Python CLI).

mod render;

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use clap::{Parser, Subcommand};
use planebotcli_cache::Cache;
use planebotcli_client::{PageScope, PbotClient};
use planebotcli_core::{PlaneError, config_file_path, load_config, save_config};
use planebotcli_format::{output_json, output_table};
use planebotcli_resolve::{
    locate_work_item_across, locate_work_item_in_project, resolve_project, resolve_user_query,
};
use planebotcli_types::{
    Attachment, CommentWrite, Cycle, CycleWrite, IntakeIssueWrite, IntakeItem, IntakeWrite, Label,
    LabelWrite, Module, ModuleWrite, Page, PageWrite, Project, ProjectWrite, State, StateWrite,
    WorkItem, WorkItemWrite,
};
use render::{
    Lookups, attachment_json, comment_json, compose_sequence_id, intake_status_label, intake_view,
    sub_issue_summaries, work_item_view,
};
use serde_json::{Value, json};

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
    /// Configure credentials interactively (base URL, API key, workspace).
    Configure,
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
    /// Manage work item relations (blocking, duplicates, links).
    #[command(visible_alias = "relation")]
    Relations {
        #[command(subcommand)]
        command: RelationsCmd,
    },
    /// Manage work item attachments (file uploads).
    #[command(visible_alias = "attachments")]
    Attachment {
        #[command(subcommand)]
        command: AttachmentCmd,
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
    /// Manage modules.
    Module {
        #[command(subcommand)]
        command: ModuleCmd,
    },
    /// Manage cycles (sprints).
    Cycle {
        #[command(subcommand)]
        command: CycleCmd,
    },
    /// Manage project intake queues.
    Intake {
        #[command(subcommand)]
        command: IntakeCmd,
    },
    /// Manage documents (pages).
    ///
    /// Without -p/--project, commands operate on workspace pages; with -p they
    /// operate on the project's pages.
    #[command(
        visible_alias = "docs",
        visible_alias = "document",
        visible_alias = "documents"
    )]
    Doc {
        #[command(subcommand)]
        command: DocCmd,
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
        /// Comment text (plain text, converted to HTML). Exactly one of
        /// --body or --body-md is required.
        #[arg(long, short = 'b')]
        body: Option<String>,
        /// Comment text in a markdown subset: headings, lists, fenced and
        /// inline code, bold/italic, auto-linked URLs — converted to HTML.
        /// Mutually exclusive with --body.
        #[arg(long)]
        body_md: Option<String>,
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
        /// New comment text (plain text, converted to HTML). Exactly one of
        /// --body or --body-md is required.
        #[arg(long, short = 'b')]
        body: Option<String>,
        /// New comment text in a markdown subset: headings, lists, fenced and
        /// inline code, bold/italic, auto-linked URLs — converted to HTML.
        /// Mutually exclusive with --body.
        #[arg(long)]
        body_md: Option<String>,
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

/// Manage work item relations.
///
/// Relations are directional: `relations ls` shows the eight buckets the API
/// returns, and `relations add` links one issue to others under a single type.
#[derive(Subcommand)]
pub enum RelationsCmd {
    /// List a work item's relations.
    #[command(alias = "ls")]
    List {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create relations from a work item to one or more others.
    Add {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Relation type: blocking, blocked_by, duplicate, relates_to,
        /// start_before, start_after, finish_before, finish_after.
        #[arg(long = "type")]
        relation_type: String,
        /// Related work item identifier (ABC-123), UUID, or name (repeatable).
        #[arg(long, short = 't')]
        to: Vec<String>,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Remove a relation (not supported by the API).
    ///
    /// The Plane API exposes relation creation but no delete endpoint, so this
    /// always fails with a validation error instead of silently doing nothing.
    #[command(alias = "rm")]
    Remove {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Relation type to remove.
        #[arg(long = "type")]
        relation_type: String,
        /// Related work item identifier (ABC-123), UUID, or name (repeatable).
        #[arg(long, short = 't')]
        to: Vec<String>,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

/// Manage work item attachments.
#[derive(Subcommand)]
pub enum AttachmentCmd {
    /// Upload a file attachment to a work item.
    ///
    /// Registers the upload, pushes the bytes to the presigned URL, marks the
    /// attachment uploaded, and verifies the server recorded it (a 2xx is not
    /// proof of a write — see ADR-0007).
    #[command(alias = "upload", visible_alias = "new")]
    Attach {
        /// Work item identifier (ABC-123), UUID, or name.
        issue: String,
        /// Path to the file to upload.
        #[arg(long, short = 'f')]
        file: String,
        /// Project name/ID (required for name-based lookup).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Upload even if an attachment with the same file name already exists.
        #[arg(long)]
        force: bool,
    },
    /// List attachments on a work item.
    #[command(alias = "ls")]
    List {
        /// Work item identifier (ABC-123), UUID, or name.
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
pub enum ModuleCmd {
    /// List modules in a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Show module details.
    Show {
        /// Module name or UUID.
        module: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new module.
    Create {
        /// Module name.
        name: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Module description.
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// Start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// End date (YYYY-MM-DD).
        #[arg(long)]
        end_date: Option<String>,
        /// Status: backlog, planned, in-progress, paused, completed, cancelled.
        #[arg(long)]
        status: Option<String>,
    },
    /// Update a module.
    Update {
        /// Module name or UUID.
        module: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New module name.
        #[arg(long)]
        name: Option<String>,
        /// New description.
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// New status: backlog, planned, in-progress, paused, completed, cancelled.
        #[arg(long)]
        status: Option<String>,
        /// New start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// New end date (YYYY-MM-DD).
        #[arg(long)]
        end_date: Option<String>,
    },
    /// Delete a module.
    Delete {
        /// Module name or UUID.
        module: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum CycleCmd {
    /// List cycles in a project.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Show cycle details.
    Show {
        /// Cycle name or UUID.
        cycle: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new cycle.
    Create {
        /// Cycle name.
        name: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Cycle description.
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// Start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// End date (YYYY-MM-DD).
        #[arg(long)]
        end_date: Option<String>,
    },
    /// Update a cycle.
    Update {
        /// Cycle name or UUID.
        cycle: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// New cycle name.
        #[arg(long)]
        name: Option<String>,
        /// New start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// New end date (YYYY-MM-DD).
        #[arg(long)]
        end_date: Option<String>,
    },
    /// Delete a cycle.
    Delete {
        /// Cycle name or UUID.
        cycle: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Add a work item to a cycle.
    AddItem {
        /// Cycle name or UUID.
        cycle: String,
        /// Work item identifier (ABC-123), UUID, or name.
        work_item: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Remove a work item from a cycle.
    RemoveItem {
        /// Cycle name or UUID.
        cycle: String,
        /// Work item identifier (ABC-123), UUID, or name.
        work_item: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// List work items in a cycle.
    Items {
        /// Cycle name or UUID.
        cycle: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum IntakeCmd {
    /// List items in a project's intake queue.
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new item in a project's intake queue.
    #[command(alias = "new")]
    Create {
        /// Item title.
        name: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
        /// Item description (plain text; HTML-escaped into a paragraph tag).
        #[arg(long, short = 'd')]
        description: Option<String>,
        /// Priority: none, low, medium, high, urgent. Default: none.
        #[arg(long, short = 'P')]
        priority: Option<String>,
    },
    /// Accept (triage) an intake item, converting it into a regular work item.
    ///
    /// Requires the project Admin role; other roles get an error instead of a
    /// silent no-op.
    Accept {
        /// Work item UUID — the "Issue ID" column of `intake ls` (not the
        /// intake wrapper ID).
        issue_id: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Decline (reject) an intake item.
    ///
    /// Requires the project Admin role; other roles get an error instead of a
    /// silent no-op.
    Decline {
        /// Work item UUID — the "Issue ID" column of `intake ls` (not the
        /// intake wrapper ID).
        issue_id: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Delete an intake item.
    ///
    /// For any status other than 'accepted' this also permanently deletes the
    /// underlying work item, not just the intake queue entry.
    Delete {
        /// Work item UUID — the "Issue ID" column of `intake ls` (not the
        /// intake wrapper ID).
        issue_id: String,
        /// Project name, identifier, or UUID (required).
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Check whether a project has intake enabled.
    Enabled {
        /// Project name, identifier, or UUID.
        project: String,
    },
}

#[derive(Subcommand)]
pub enum DocCmd {
    /// List documents (workspace pages, or project pages with -p).
    #[command(alias = "ls")]
    List {
        /// Project name, identifier, or UUID. If omitted, lists workspace pages.
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Show document details.
    #[command(alias = "read")]
    Show {
        /// Page name or UUID.
        doc: String,
        /// Project name, identifier, or UUID. If omitted, looks up a workspace page.
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Create a new document.
    #[command(alias = "new")]
    Create {
        /// Document title.
        #[arg(long)]
        title: String,
        /// Document content (plain text, converted to HTML).
        #[arg(long, short = 'c')]
        content: Option<String>,
        /// Content written in native markdown (headings, lists, code, bold/italic,
        /// links) — converted to HTML. Mutually exclusive with --content.
        #[arg(long)]
        content_md: Option<String>,
        /// Content as raw HTML, stored verbatim (rich layout). Mutually
        /// exclusive with --content and --content-md.
        #[arg(long)]
        content_html: Option<String>,
        /// Project name, identifier, or UUID. If omitted, creates a workspace page.
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Update a document.
    Update {
        /// Page name or UUID.
        doc: String,
        /// New document title.
        #[arg(long)]
        title: Option<String>,
        /// New content (plain text, converted to HTML).
        #[arg(long, short = 'c')]
        content: Option<String>,
        /// New content in native markdown — converted to HTML. Mutually
        /// exclusive with --content.
        #[arg(long)]
        content_md: Option<String>,
        /// New content as raw HTML, stored verbatim. Mutually exclusive with
        /// --content and --content-md.
        #[arg(long)]
        content_html: Option<String>,
        /// Project name, identifier, or UUID. If omitted, operates on a workspace page.
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Archive a document (trash it) without deleting.
    ///
    /// Sets archived_at = today and verifies the archive landed; the page
    /// stays recoverable in the web UI trash.
    Archive {
        /// Page name or UUID.
        doc: String,
        /// Project name, identifier, or UUID. If omitted, operates on a workspace page.
        #[arg(long, short = 'p')]
        project: Option<String>,
    },
    /// Delete a document.
    ///
    /// Pages must be archived before the API accepts a DELETE, so this first
    /// archives the page (archived_at = today) and verifies the archive
    /// landed, then deletes it.
    Delete {
        /// Page name or UUID.
        doc: String,
        /// Project name, identifier, or UUID. If omitted, operates on a workspace page.
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
        /// Work item description in a markdown subset: headings, lists, fenced
        /// and inline code, bold/italic, auto-linked URLs — converted to HTML.
        /// Mutually exclusive with --description.
        #[arg(long)]
        desc_md: Option<String>,
        /// Start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// Target end date (YYYY-MM-DD).
        #[arg(long)]
        target_date: Option<String>,
        /// Image file path to embed in the description (repeatable). The image
        /// is uploaded and an img tag is appended to the description.
        #[arg(long, short = 'i')]
        image: Vec<String>,
        /// Upload images even if an attachment with the same file name already
        /// exists.
        #[arg(long)]
        force: bool,
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
        /// New description in a markdown subset: headings, lists, fenced and
        /// inline code, bold/italic, auto-linked URLs — converted to HTML.
        /// Mutually exclusive with --description.
        #[arg(long)]
        desc_md: Option<String>,
        /// New start date (YYYY-MM-DD).
        #[arg(long)]
        start_date: Option<String>,
        /// New target end date (YYYY-MM-DD).
        #[arg(long)]
        target_date: Option<String>,
        /// Set the parent work item (ABC-123), UUID, or name. Must live in the
        /// same project as the issue. Mutually exclusive with --clear-parent.
        #[arg(long)]
        parent: Option<String>,
        /// Remove the parent, making the work item top-level. Mutually
        /// exclusive with --parent.
        #[arg(long)]
        clear_parent: bool,
        /// Image file path to embed in the description (repeatable). The image
        /// is uploaded and appended to the existing description, or to the new
        /// --description (or --desc-md) if one is given.
        #[arg(long, short = 'i')]
        image: Vec<String>,
        /// Upload images even if an attachment with the same file name already
        /// exists.
        #[arg(long)]
        force: bool,
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
    // `configure` is fully offline: it must not require an existing config
    // file or a client, so it dispatches before config loading.
    if matches!(cli.command, Command::Configure) {
        return cmd_configure();
    }

    let cfg = load_config()?;
    let client = PbotClient::with_cache(&cfg, cli.no_cache)?;

    match cli.command {
        Command::Configure => unreachable!("configure is handled before config load"),
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
                desc_md,
                start_date,
                target_date,
                image,
                force,
            } => {
                let opts = CreateOpts {
                    project: project.as_deref(),
                    assignee: assignee.as_deref(),
                    state: state.as_deref(),
                    labels: labels.as_deref(),
                    priority: priority.as_deref(),
                    parent: parent.as_deref(),
                    description: description.as_deref(),
                    desc_md: desc_md.as_deref(),
                    start_date: start_date.as_deref(),
                    target_date: target_date.as_deref(),
                    image: &image,
                    force,
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
                desc_md,
                start_date,
                target_date,
                parent,
                clear_parent,
                image,
                force,
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
                    desc_md: desc_md.as_deref(),
                    start_date: start_date.as_deref(),
                    target_date: target_date.as_deref(),
                    parent: parent.as_deref(),
                    clear_parent,
                    image: &image,
                    force,
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
                body_md,
                project,
            } => {
                let markdown = body_md.is_some();
                if body.is_some() && markdown {
                    return Err(PlaneError::validation_with_hint(
                        "--body and --body-md are mutually exclusive.",
                        "Pass the comment text via only one of them.",
                    ));
                }
                let Some(body) = body.or(body_md) else {
                    return Err(PlaneError::validation_with_hint(
                        "One of --body or --body-md is required.",
                        "Example: planebotcli comment create PROJ-1 --body-md '## Notes'",
                    ));
                };
                cmd_comment_write(
                    &client,
                    &issue,
                    project.as_deref(),
                    None,
                    &body,
                    markdown,
                    cli.json,
                )
                .await
            }
            CommentCmd::Update {
                comment_id,
                issue,
                body,
                body_md,
                project,
            } => {
                let markdown = body_md.is_some();
                if body.is_some() && markdown {
                    return Err(PlaneError::validation_with_hint(
                        "--body and --body-md are mutually exclusive.",
                        "Pass the comment text via only one of them.",
                    ));
                }
                let Some(body) = body.or(body_md) else {
                    return Err(PlaneError::validation_with_hint(
                        "One of --body or --body-md is required.",
                        "Example: planebotcli comment update COMMENT-ID ISSUE --body-md '## Notes'",
                    ));
                };
                cmd_comment_write(
                    &client,
                    &issue,
                    project.as_deref(),
                    Some(&comment_id),
                    &body,
                    markdown,
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
        Command::Relations { command } => match command {
            RelationsCmd::List { issue, project } => {
                cmd_relations_list(&client, &issue, project.as_deref(), cli.json).await
            }
            RelationsCmd::Add {
                issue,
                relation_type,
                to,
                project,
            } => {
                let opts = RelationAddOpts {
                    relation_type: &relation_type,
                    to: &to,
                    project: project.as_deref(),
                };
                cmd_relations_add(&client, &issue, &opts, cli.json).await
            }
            RelationsCmd::Remove { .. } => cmd_relations_remove(),
        },
        Command::Attachment { command } => match command {
            AttachmentCmd::Attach {
                issue,
                file,
                project,
                force,
            } => {
                cmd_attachment_attach(&client, &issue, &file, project.as_deref(), force, cli.json)
                    .await
            }
            AttachmentCmd::List { issue, project } => {
                cmd_attachment_list(&client, &issue, project.as_deref(), cli.json).await
            }
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
        Command::Module { command } => match command {
            ModuleCmd::List { project } => {
                cmd_module_list(&client, project.as_deref(), cli.json).await
            }
            ModuleCmd::Show { module, project } => {
                cmd_module_show(&client, &module, project.as_deref(), cli.json).await
            }
            ModuleCmd::Create {
                name,
                project,
                description,
                start_date,
                end_date,
                status,
            } => {
                let opts = ModuleCreateOpts {
                    project: project.as_deref(),
                    description: description.as_deref(),
                    start_date: start_date.as_deref(),
                    end_date: end_date.as_deref(),
                    status: status.as_deref(),
                };
                cmd_module_create(&client, &name, &opts, cli.json).await
            }
            ModuleCmd::Update {
                module,
                project,
                name,
                description,
                status,
                start_date,
                end_date,
            } => {
                let opts = ModuleUpdateOpts {
                    project: project.as_deref(),
                    name: name.as_deref(),
                    description: description.as_deref(),
                    status: status.as_deref(),
                    start_date: start_date.as_deref(),
                    end_date: end_date.as_deref(),
                };
                cmd_module_update(&client, &module, &opts, cli.json).await
            }
            ModuleCmd::Delete { module, project } => {
                cmd_module_delete(&client, &module, project.as_deref()).await
            }
        },
        Command::Cycle { command } => match command {
            CycleCmd::List { project } => {
                cmd_cycle_list(&client, project.as_deref(), cli.json).await
            }
            CycleCmd::Show { cycle, project } => {
                cmd_cycle_show(&client, &cycle, project.as_deref(), cli.json).await
            }
            CycleCmd::Create {
                name,
                project,
                description,
                start_date,
                end_date,
            } => {
                let opts = CycleCreateOpts {
                    project: project.as_deref(),
                    description: description.as_deref(),
                    start_date: start_date.as_deref(),
                    end_date: end_date.as_deref(),
                };
                cmd_cycle_create(&client, &name, &opts, cli.json).await
            }
            CycleCmd::Update {
                cycle,
                project,
                name,
                start_date,
                end_date,
            } => {
                let opts = CycleUpdateOpts {
                    project: project.as_deref(),
                    name: name.as_deref(),
                    start_date: start_date.as_deref(),
                    end_date: end_date.as_deref(),
                };
                cmd_cycle_update(&client, &cycle, &opts, cli.json).await
            }
            CycleCmd::Delete { cycle, project } => {
                cmd_cycle_delete(&client, &cycle, project.as_deref()).await
            }
            CycleCmd::AddItem {
                cycle,
                work_item,
                project,
            } => cmd_cycle_add_item(&client, &cycle, &work_item, project.as_deref()).await,
            CycleCmd::RemoveItem {
                cycle,
                work_item,
                project,
            } => cmd_cycle_remove_item(&client, &cycle, &work_item, project.as_deref()).await,
            CycleCmd::Items { cycle, project } => {
                cmd_cycle_items(&client, &cfg.base_url, &cycle, project.as_deref(), cli.json).await
            }
        },
        Command::Intake { command } => match command {
            IntakeCmd::List { project } => {
                cmd_intake_list(&client, project.as_deref(), cli.json).await
            }
            IntakeCmd::Create {
                name,
                project,
                description,
                priority,
            } => {
                cmd_intake_create(
                    &client,
                    &name,
                    project.as_deref(),
                    description.as_deref(),
                    priority.as_deref(),
                    cli.json,
                )
                .await
            }
            IntakeCmd::Accept { issue_id, project } => {
                cmd_intake_triage(&client, &issue_id, project.as_deref(), 1, cli.json).await
            }
            IntakeCmd::Decline { issue_id, project } => {
                cmd_intake_triage(&client, &issue_id, project.as_deref(), -1, cli.json).await
            }
            IntakeCmd::Delete { issue_id, project } => {
                cmd_intake_delete(&client, &issue_id, project.as_deref()).await
            }
            IntakeCmd::Enabled { project } => cmd_intake_enabled(&client, &project, cli.json).await,
        },
        Command::User { command } => match command {
            UserCmd::List => cmd_user_list(&client, cli.json).await,
        },
        Command::Doc { command } => match command {
            DocCmd::List { project } => cmd_doc_list(&client, project.as_deref(), cli.json).await,
            DocCmd::Show { doc, project } => {
                cmd_doc_show(&client, &doc, project.as_deref(), cli.json).await
            }
            DocCmd::Create {
                title,
                content,
                content_md,
                content_html,
                project,
            } => {
                cmd_doc_create(
                    &client,
                    &title,
                    content.as_deref(),
                    content_md.as_deref(),
                    content_html.as_deref(),
                    project.as_deref(),
                    cli.json,
                )
                .await
            }
            DocCmd::Update {
                doc,
                title,
                content,
                content_md,
                content_html,
                project,
            } => {
                cmd_doc_update(
                    &client,
                    &doc,
                    title.as_deref(),
                    content.as_deref(),
                    content_md.as_deref(),
                    content_html.as_deref(),
                    project.as_deref(),
                    cli.json,
                )
                .await
            }
            DocCmd::Archive { doc, project } => {
                cmd_doc_archive(&client, &doc, project.as_deref()).await
            }
            DocCmd::Delete { doc, project } => {
                cmd_doc_delete(&client, &doc, project.as_deref()).await
            }
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

/// Configure credentials interactively, mirroring `app.py::configure`.
///
/// Prompts for the base URL, API key, and workspace slug, saves them to
/// `~/.plane_api` (chmod 600), then clears the disk cache (it may hold data
/// from a different instance). Fully offline — no existing config or client
/// required. A missing field prints an error and exits 1 (Python behavior).
fn cmd_configure() -> Result<(), PlaneError> {
    let base_url = configure_prompt("Plane base URL (e.g. https://api.plane.so): ");
    let api_key = configure_prompt("API key: ");
    let workspace = configure_prompt("Workspace slug: ");

    if base_url.is_empty() || api_key.is_empty() || workspace.is_empty() {
        // Python exits 1 here (not one of the PlaneError codes).
        return Err(PlaneError::Other(anyhow::anyhow!(
            "All fields are required."
        )));
    }

    save_config(&base_url, &api_key, &workspace).map_err(|e| PlaneError::Other(e.into()))?;
    Cache::new().clear();
    println!("Cache cleared.");
    println!("\nConfiguration saved to {}.", config_file_path().display());
    Ok(())
}

/// Print a prompt to stdout and read one trimmed line from stdin.
fn configure_prompt(prompt: &str) -> String {
    use std::io::Write;

    let mut line = String::new();
    print!("{prompt}");
    let _ = std::io::stdout().flush();
    let _ = std::io::stdin().read_line(&mut line);
    line.trim().to_string()
}

async fn cmd_whoami(client: &PbotClient, json: bool) -> Result<(), PlaneError> {
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
    client: &PbotClient,
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
    client: &PbotClient,
    project: &str,
    json: bool,
) -> Result<(), PlaneError> {
    let project = resolve_project(project, client).await?;
    output_project_view(&project, json);
    Ok(())
}

async fn cmd_project_create(
    client: &PbotClient,
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
    client: &PbotClient,
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

async fn cmd_project_delete(client: &PbotClient, project: &str) -> Result<(), PlaneError> {
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
    client: &PbotClient,
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
    client: &PbotClient,
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

    // Sub-issues: the project's work items parented to this one, so a parent
    // set or cleared here is visible on read-back.
    let project_items = client.list_work_items(&located.project_id).await?;
    view["sub_issues"] = Value::Array(sub_issue_summaries(
        &project_items,
        &detail.id,
        &located.project_identifier,
        &lookups,
    ));

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
            ("parent", "Parent"),
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

        if let Some(Value::Array(items)) = view.get("sub_issues")
            && !items.is_empty()
        {
            eprintln!();
            let sub_rows: Vec<Vec<String>> = items
                .iter()
                .map(|sub| {
                    let seq = sub["sequence_id"].as_str().unwrap_or("");
                    let id = sub["id"].as_str().unwrap_or("");
                    vec![
                        if seq.is_empty() {
                            short_uuid(id)
                        } else {
                            seq.to_string()
                        },
                        sub["name"].as_str().unwrap_or("").to_string(),
                        sub["state_detail_name"].as_str().unwrap_or("").to_string(),
                    ]
                })
                .collect();
            output_table(&["Sub-issue", "Title", "State"], &sub_rows);
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
    desc_md: Option<&'a str>,
    start_date: Option<&'a str>,
    target_date: Option<&'a str>,
    /// Image paths to upload and embed in the description (`-i`, repeatable).
    image: &'a [String],
    force: bool,
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
    desc_md: Option<&'a str>,
    start_date: Option<&'a str>,
    target_date: Option<&'a str>,
    /// New parent work item reference (`--parent`).
    parent: Option<&'a str>,
    /// Clear the parent (`--clear-parent`).
    clear_parent: bool,
    /// Image paths to upload and embed in the description (`-i`, repeatable).
    image: &'a [String],
    force: bool,
}

/// `--description` and `--desc-md` are mutually exclusive (Python message).
fn validate_description_flags(
    description: Option<&str>,
    desc_md: Option<&str>,
) -> Result<(), PlaneError> {
    if description.is_some() && desc_md.is_some() {
        return Err(PlaneError::validation_with_hint(
            "--description and --desc-md are mutually exclusive.",
            "Pass the description via only one of them.",
        ));
    }
    Ok(())
}

/// Trim a `--parent` reference and reject a blank one: an empty query would
/// fuzzy-match the first work item in the project.
fn validate_parent_query(query: &str) -> Result<&str, PlaneError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "--parent requires a work item identifier, UUID, or name.",
            "Use --clear-parent to remove the parent instead.",
        ));
    }
    Ok(trimmed)
}

/// Reject an empty `--to` list and blank targets (a blank entry would match an
/// arbitrary work item).
fn validate_relation_targets(targets: &[String]) -> Result<(), PlaneError> {
    if targets.is_empty() {
        return Err(PlaneError::validation_with_hint(
            "At least one --to is required.",
            "Example: pbot relations add PLANECLI-38 --type blocking --to PLANECLI-39",
        ));
    }
    if targets.iter().any(|target| target.trim().is_empty()) {
        return Err(PlaneError::validation(
            "--to requires a work item identifier, UUID, or name.",
        ));
    }
    Ok(())
}

/// How a `--parent` reference relates to the issue's own project, decided
/// before any server lookup.
#[derive(Debug, PartialEq, Eq)]
enum ParentReference {
    /// A UUID, bare name, or same-project `ABC-123` — resolve it in the project.
    SameProject,
    /// `ABC-123` carrying another project's identifier.
    CrossProject(String),
}

/// The project identifier prefix of an `ABC-123` reference (`None` for UUIDs,
/// bare names, and prefixed-looking text such as `Release-2026` whose prefix is
/// not identifier-shaped).
fn identifier_prefix(query: &str) -> Option<&str> {
    if planebotcli_resolve::is_uuid(query) {
        return None;
    }
    let (prefix, suffix) = query.rsplit_once('-')?;
    let prefix_is_identifier = !prefix.is_empty()
        && prefix
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !prefix_is_identifier || suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(prefix)
}

/// Classify a `--parent` reference against the issue's project identifier. An
/// unknown project identifier (`""`) can never prove a reference foreign, so it
/// resolves within the project.
fn classify_parent_reference(query: &str, project_identifier: &str) -> ParentReference {
    match identifier_prefix(query) {
        Some(prefix)
            if !project_identifier.is_empty()
                && !prefix.eq_ignore_ascii_case(project_identifier) =>
        {
            ParentReference::CrossProject(prefix.to_string())
        }
        _ => ParentReference::SameProject,
    }
}

/// Up to 15 `PROJ-123` labels for the "parent not found" message, keeping it
/// short on large projects.
fn parent_candidates(items: &[WorkItem], project_identifier: &str) -> Vec<String> {
    items
        .iter()
        .map(|item| compose_sequence_id(item.sequence_id.as_ref(), project_identifier))
        .filter(|label| !label.is_empty())
        .take(15)
        .collect()
}

/// Seed the description parts that `-i/--image` img tags are appended to,
/// mirroring the Python precedence exactly: `--desc-md` wins over a non-empty
/// `--description`, and `existing` (the stored `description_html`, update
/// only) is the last fallback.
fn embed_seed_parts(
    desc_md: Option<&str>,
    description: Option<&str>,
    existing: Option<&str>,
) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(md) = desc_md {
        parts.push(planebotcli_html::md_to_html(md));
    } else if let Some(description) = description.filter(|d| !d.is_empty()) {
        parts.push(format!("<p>{description}</p>"));
    } else if let Some(existing) = existing {
        parts.push(existing.to_string());
    }
    parts
}

/// `<p><img src="{asset_id}" /></p>` — embed an uploaded asset in a work item
/// description.
///
/// The web editor stores the asset UUID in `src` and resolves it at render
/// time; a full path or URL here is mistaken for an asset id and the image
/// fails to load (Python `embed_html`).
fn embed_html(asset_id: &str) -> String {
    format!(r#"<p><img src="{asset_id}" /></p>"#)
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
    client: &PbotClient,
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
    client: &PbotClient,
    base_url: &str,
    title: &str,
    opts: &CreateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    validate_description_flags(opts.description, opts.desc_md)?;
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
    if let Some(md) = opts.desc_md {
        write.description_html = Some(planebotcli_html::md_to_html(md));
    } else if opts.image.is_empty() {
        // With `-i` the description is seeded into the follow-up PATCH instead
        // (Python: `elif description and not image`).
        if let Some(description) = opts.description {
            write.description_html = Some(format!("<p>{description}</p>"));
        }
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

    // Embed images: the item is created first (uploads need the issue to
    // exist), then each image is uploaded and its img tag is appended to the
    // description via a follow-up PATCH (Python `wi create` image flow).
    let final_item = if opts.image.is_empty() {
        created
    } else {
        let mut parts = embed_seed_parts(opts.desc_md, opts.description, None);
        for path in opts.image {
            let asset_id =
                upload_embed_image(client, &project_id, &created.id, path, opts.force).await?;
            parts.push(embed_html(&asset_id));
        }
        let image_write = WorkItemWrite {
            description_html: Some(parts.concat()),
            ..Default::default()
        };
        client
            .update_work_item(&project_id, &created.id, &image_write)
            .await?
    };

    output_work_item_view(
        client,
        base_url,
        &final_item,
        &project_identifier,
        json,
        "Work Item Created",
    )
    .await
}

async fn cmd_wi_update(
    client: &PbotClient,
    base_url: &str,
    issue: &str,
    opts: &UpdateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    validate_description_flags(opts.description, opts.desc_md)?;
    let located = locate_issue(client, issue, opts.project).await?;
    let project_id = located.project_id.clone();
    let project_identifier = located.project_identifier.clone();
    // Validated before any write, so a bad parent never half-applies an update.
    let parent_action = resolve_parent_action(client, &located, opts).await?;

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
    if !opts.image.is_empty() {
        // Embed images: upload each, then append its img tag — to the new
        // --description (or --desc-md) when given, otherwise to the existing
        // stored description_html (Python `wi update` image flow). All fields
        // land in the single PATCH below.
        let mut parts = embed_seed_parts(
            opts.desc_md,
            opts.description,
            located.item.description_html.as_deref(),
        );
        for path in opts.image {
            let asset_id =
                upload_embed_image(client, &project_id, &located.item.id, path, opts.force).await?;
            parts.push(embed_html(&asset_id));
        }
        write.description_html = Some(parts.concat());
    } else if let Some(md) = opts.desc_md {
        write.description_html = Some(planebotcli_html::md_to_html(md));
    } else if let Some(description) = opts.description {
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

    // The parent lives on its own endpoint; re-read the item so the printed
    // view reflects the parent the server now holds.
    let updated = match parent_action {
        Some(parent) => {
            client
                .set_work_item_parent(&project_id, &located.item.id, parent.as_deref())
                .await?;
            client.get_work_item(&project_id, &located.item.id).await?
        }
        None => updated,
    };

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

/// Resolve `--parent`/`--clear-parent` into the value
/// [`PbotClient::set_work_item_parent`] needs: `None` when neither flag was
/// given, `Some(None)` to clear, `Some(Some(uuid))` to set.
///
/// Every rejection is a `PlaneError::Validation` (exit 5) raised before the
/// write: mutual exclusion, a cross-project reference, the issue as its own
/// parent, and an unresolvable reference.
async fn resolve_parent_action(
    client: &PbotClient,
    located: &planebotcli_resolve::LocatedWorkItem,
    opts: &UpdateOpts<'_>,
) -> Result<Option<Option<String>>, PlaneError> {
    if opts.clear_parent {
        if opts.parent.is_some() {
            return Err(PlaneError::validation_with_hint(
                "--parent and --clear-parent are mutually exclusive.",
                "Pass one of them, or neither to leave the parent unchanged.",
            ));
        }
        return Ok(Some(None));
    }
    let Some(query) = opts.parent else {
        return Ok(None);
    };
    let query = validate_parent_query(query)?;

    if let ParentReference::CrossProject(prefix) =
        classify_parent_reference(query, &located.project_identifier)
    {
        return Err(PlaneError::Validation {
            message: format!(
                "Work item {query} belongs to project '{prefix}', but this work item is in {} — a parent must be in the same project.",
                located.project_identifier
            ),
            hint: None,
        });
    }

    let items = client.list_work_items(&located.project_id).await?;
    let proj = project_stub(located);
    match locate_work_item_in_project(query, &proj, client).await {
        Ok(target) => {
            validate_resolved_parent(
                query,
                &located.item.id,
                &located.project_id,
                &located.project_identifier,
                &target.item.id,
                &target.project_id,
            )?;
            Ok(Some(Some(target.item.id)))
        }
        Err(PlaneError::NotFound { .. }) => {
            let candidates = parent_candidates(&items, &located.project_identifier);
            let available = if candidates.is_empty() {
                "(none)".to_string()
            } else {
                candidates.join(", ")
            };
            Err(PlaneError::Validation {
                message: format!(
                    "Parent work item not found in project {}: {query}. Available: {available}",
                    located.project_identifier
                ),
                hint: None,
            })
        }
        Err(e) => Err(e),
    }
}

/// Validate a resolved parent before the write: a work item cannot parent
/// itself, and the parent must live in the same project as the issue. Both are
/// `Validation` errors (exit 5), keeping them unit-testable without a client.
fn validate_resolved_parent(
    query: &str,
    issue_id: &str,
    issue_project_id: &str,
    issue_project_identifier: &str,
    target_id: &str,
    target_project_id: &str,
) -> Result<(), PlaneError> {
    if target_id == issue_id {
        return Err(PlaneError::validation(format!(
            "A work item cannot be its own parent: {query}."
        )));
    }
    if !target_project_id.is_empty() && target_project_id != issue_project_id {
        return Err(PlaneError::Validation {
            message: format!(
                "Work item {query} is in project {target_project_id}, but this work item is in {issue_project_identifier} — a parent must be in the same project."
            ),
            hint: None,
        });
    }
    Ok(())
}

/// Enrich and print a freshly written work item (maps fetched for names).
async fn output_work_item_view(
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
///
/// `body` is the raw user text; `markdown` selects `md_to_html` over
/// `body_to_html`.
async fn cmd_comment_write(
    client: &PbotClient,
    issue: &str,
    project: Option<&str>,
    comment_id: Option<&str>,
    body: &str,
    markdown: bool,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let comment_html = if markdown {
        planebotcli_html::md_to_html(body)
    } else {
        planebotcli_html::body_to_html(body)
    };
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
    client: &PbotClient,
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

/// The relation types the Plane API accepts, in bucket order.
const RELATION_TYPES: [&str; 8] = [
    "blocking",
    "blocked_by",
    "duplicate",
    "relates_to",
    "start_before",
    "start_after",
    "finish_before",
    "finish_after",
];

/// Options for `relations add`.
struct RelationAddOpts<'a> {
    relation_type: &'a str,
    to: &'a [String],
    project: Option<&'a str>,
}

/// Validate a `--type` value against the eight the API accepts. Hyphens are
/// taken as a spelling of underscores (`relates-to` == `relates_to`).
fn normalize_relation_type(raw: &str) -> Result<&'static str, PlaneError> {
    let normalized = raw.trim().to_ascii_lowercase().replace('-', "_");
    RELATION_TYPES
        .iter()
        .copied()
        .find(|known| *known == normalized)
        .ok_or_else(|| PlaneError::Validation {
            message: format!(
                "Unknown relation type '{raw}'. Allowed: {}.",
                RELATION_TYPES.join(", ")
            ),
            hint: None,
        })
}

/// Flatten the API's relation buckets into `(type, project_id, issue_id)`
/// rows in [`RELATION_TYPES`] order; empty buckets are omitted.
fn relation_rows(relations: &Value) -> Vec<(String, String, String)> {
    let mut rows = Vec::new();
    for relation_type in RELATION_TYPES {
        let Some(Value::Array(entries)) = relations.get(relation_type) else {
            continue;
        };
        for entry in entries {
            rows.push((
                relation_type.to_string(),
                entry
                    .get("project_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                entry
                    .get("issue_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
    }
    rows
}

/// A minimal `Project` carrying only the id and identifier of a located work
/// item's project, for the project-scoped resolvers (no extra request).
fn project_stub(located: &planebotcli_resolve::LocatedWorkItem) -> Project {
    Project {
        id: located.project_id.clone(),
        identifier: Some(located.project_identifier.clone()),
        ..Default::default()
    }
}

/// `PROJ-123` for a located work item, falling back to its UUID.
fn issue_label(located: &planebotcli_resolve::LocatedWorkItem) -> String {
    let label = compose_sequence_id(
        located.item.sequence_id.as_ref(),
        &located.project_identifier,
    );
    if label.is_empty() {
        located.item.id.clone()
    } else {
        label
    }
}

/// First eight characters of a UUID, for display when no label resolves.
fn short_uuid(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Map related issue UUIDs to `PROJ-123` labels, listing the work items of
/// every project the buckets mention (cached; the issue's own project is known
/// up front and needs no extra project lookup).
async fn relation_issue_labels(
    client: &PbotClient,
    rows: &[(String, String, String)],
    own_project_id: &str,
    own_identifier: &str,
) -> Result<HashMap<String, String>, PlaneError> {
    let mut project_ids = vec![own_project_id.to_string()];
    for (_, project_id, _) in rows {
        if !project_id.is_empty() && !project_ids.iter().any(|p| p == project_id) {
            project_ids.push(project_id.clone());
        }
    }
    let mut labels = HashMap::new();
    for project_id in project_ids {
        let identifier = if project_id == own_project_id {
            own_identifier.to_string()
        } else {
            client
                .get_project(&project_id)
                .await?
                .identifier
                .unwrap_or_default()
        };
        for item in client.list_work_items(&project_id).await? {
            let label = compose_sequence_id(item.sequence_id.as_ref(), &identifier);
            if !label.is_empty() {
                labels.insert(item.id, label);
            }
        }
    }
    Ok(labels)
}

/// `relations ls` — list a work item's relations. `--json` prints the raw
/// relation buckets; the table flattens them into one row per related issue.
async fn cmd_relations_list(
    client: &PbotClient,
    issue: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let relations = client
        .list_relations(&located.project_id, &located.item.id)
        .await?;
    if json {
        output_json(&relations);
        return Ok(());
    }
    let rows = relation_rows(&relations);
    if rows.is_empty() {
        eprintln!("No relations for {}.", issue_label(&located));
        return Ok(());
    }
    let labels = relation_issue_labels(
        client,
        &rows,
        &located.project_id,
        &located.project_identifier,
    )
    .await?;
    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|(relation_type, _, issue_id)| {
            vec![
                relation_type.clone(),
                labels
                    .get(issue_id)
                    .cloned()
                    .unwrap_or_else(|| short_uuid(issue_id)),
                issue_id.clone(),
            ]
        })
        .collect();
    output_table(&["Type", "Related", "Issue ID"], &table_rows);
    Ok(())
}

/// `relations add` — create one relation type from `issue` to every `--to`
/// target. Targets are resolved inside the issue's project and validated
/// before the write.
async fn cmd_relations_add(
    client: &PbotClient,
    issue: &str,
    opts: &RelationAddOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let relation_type = normalize_relation_type(opts.relation_type)?;
    validate_relation_targets(opts.to)?;
    let located = locate_issue(client, issue, opts.project).await?;
    let proj = project_stub(&located);
    let mut target_ids = Vec::with_capacity(opts.to.len());
    for target in opts.to {
        let found = locate_work_item_in_project(target, &proj, client).await?;
        if found.item.id == located.item.id {
            return Err(PlaneError::validation(format!(
                "A work item cannot be related to itself: {target}."
            )));
        }
        target_ids.push(found.item.id);
    }
    let response = client
        .create_relations(
            &located.project_id,
            &located.item.id,
            relation_type,
            &target_ids,
        )
        .await?;
    if json {
        output_json(&response);
    } else {
        eprintln!(
            "Linked {} ({relation_type}) to {} work item(s).",
            issue_label(&located),
            target_ids.len()
        );
    }
    Ok(())
}

/// `relations rm` — always fails: the Plane API exposes relation creation but
/// no delete endpoint, so a silent no-op would be misleading.
fn cmd_relations_remove() -> Result<(), PlaneError> {
    Err(PlaneError::validation_with_hint(
        "Removing a relation is not supported: the Plane API has no relations delete endpoint.",
        "Tracked in PLANECLI-38; use the Plane web UI to remove a relation for now.",
    ))
}

/// Guess a MIME type from a filename extension, defaulting to
/// application/octet-stream (the common-type subset of Python's
/// `mimetypes.guess_type`, which the Python `_guess_mime` wraps).
fn guess_mime(filename: &str) -> String {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "tif" | "tiff" => "image/tiff",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "7z" => "application/x-7z-compressed",
        "rar" => "application/vnd.rar",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "rtf" => "application/rtf",
        _ => "application/octet-stream",
    };
    mime.to_string()
}

/// Render a JSON scalar (number/string/bool/null) as a table cell.
fn scalar_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

async fn cmd_attachment_list(
    client: &PbotClient,
    issue: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let attachments = client
        .list_attachments(&located.project_id, &located.item.id)
        .await?;
    if json {
        let views: Vec<Value> = attachments.iter().map(attachment_json).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = attachments
            .iter()
            .map(|a| {
                let view = attachment_json(a);
                vec![
                    view["id"].as_str().unwrap_or("").to_string(),
                    view["name"].as_str().unwrap_or("").to_string(),
                    view["type"].as_str().unwrap_or("").to_string(),
                    scalar_cell(&view["size"]),
                    scalar_cell(&view["is_uploaded"]),
                    fmt_ts(view["created_at"].as_str().unwrap_or("")),
                ]
            })
            .collect();
        output_table(
            &["ID", "Name", "Type", "Size", "Uploaded", "Created"],
            &rows,
        );
    }
    Ok(())
}

/// A locally preflighted upload file: the display name, guessed MIME type,
/// byte count, and filesystem path (Python's isfile/getsize/_guess_mime
/// triple).
struct UploadFile<'a> {
    path: &'a Path,
    name: String,
    mime: String,
    size: u64,
}

/// Validate that `file` names an existing regular file and gather what the
/// upload endpoints need (Python's os.path.isfile/getsize + `_guess_mime`).
fn preflight_upload(file: &str) -> Result<UploadFile<'_>, PlaneError> {
    let path = Path::new(file);
    let meta = std::fs::metadata(path).map_err(|_| PlaneError::Validation {
        message: format!("File not found: {file}"),
        hint: None,
    })?;
    if !meta.is_file() {
        return Err(PlaneError::Validation {
            message: format!("File not found: {file}"),
            hint: None,
        });
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| PlaneError::Validation {
            message: format!("Invalid file name: {file}"),
            hint: None,
        })?
        .to_string();
    let mime = guess_mime(&name);
    Ok(UploadFile {
        path,
        name,
        mime,
        size: meta.len(),
    })
}

/// v1 ISSUE_ATTACHMENT upload shared by `attachment attach` and the `-i`
/// fallback: duplicate-name check -> register -> presigned push -> finalize ->
/// read the list back and verify the asset landed (a 2xx is not proof of a
/// write, see ADR-0007). Mirrors the Python `upload_attachment`; the Rust
/// convention refuses duplicates instead of prompting.
async fn upload_attachment_v1(
    client: &PbotClient,
    project_id: &str,
    work_item_id: &str,
    file: &UploadFile<'_>,
    force: bool,
) -> Result<Attachment, PlaneError> {
    // Duplicate guard: same-named attachments are easy to create by accident
    // and hard to tell apart in the web UI (deleting the wrong one breaks
    // embedded images). Refuse unless --force.
    let listed = client.list_attachments(project_id, work_item_id).await?;
    let existing = listed
        .iter()
        .filter(|a| !a.is_deleted.unwrap_or(false) && a.name() == file.name)
        .count();
    if existing > 0 && !force {
        return Err(PlaneError::Validation {
            message: format!(
                "Upload cancelled: '{}' is already attached ({existing}x).",
                file.name
            ),
            hint: Some("Re-run with --force to upload anyway.".into()),
        });
    }

    // 1. Register the attachment -> a presigned upload target + asset id.
    let created = client
        .register_attachment(project_id, work_item_id, &file.name, &file.mime, file.size)
        .await?;
    let asset_id = created
        .get("asset_id")
        .and_then(Value::as_str)
        .or_else(|| created.get("id").and_then(Value::as_str))
        .ok_or_else(|| PlaneError::Api {
            message: "the register response did not include an attachment asset id.".into(),
        })?
        .to_string();
    let upload_data = created.get("upload_data").ok_or_else(|| PlaneError::Api {
        message: "the register response did not include presigned upload data.".into(),
    })?;

    // 2. Push the bytes to the presigned URL. The signature lives in the
    // multipart form fields — no X-Api-Key header on this request.
    let bytes = std::fs::read(file.path).map_err(|e| PlaneError::Api {
        message: format!("failed to read {}: {e}", file.path.display()),
    })?;
    client
        .upload_to_presigned(upload_data, &file.name, &file.mime, bytes)
        .await?;

    // 3. Mark the attachment uploaded, then read the list back and verify the
    // asset is present with is_uploaded=true (see ADR-0007).
    client
        .finalize_attachment(project_id, work_item_id, &asset_id)
        .await?;
    let after = client.list_attachments(project_id, work_item_id).await?;
    let mine = after
        .iter()
        .find(|a| a.id == asset_id && a.is_uploaded.unwrap_or(false))
        .cloned()
        .ok_or_else(|| PlaneError::Api {
            message: "Attachment was not confirmed by the server after upload.".into(),
        })?;
    Ok(mine)
}

/// Upload an image for description embedding and return the asset UUID
/// (mirrors the Python `upload_embed_image`).
///
/// Prefers the native app assets v2 endpoint (`entity_type=ISSUE_DESCRIPTION`
/// — the image renders inline and does not clutter the attachment list).
/// Deployments whose app endpoints are session-only answer 401/403/404, in
/// which case the upload falls back to a v1 ISSUE_ATTACHMENT.
async fn upload_embed_image(
    client: &PbotClient,
    project_id: &str,
    work_item_id: &str,
    file: &str,
    force: bool,
) -> Result<String, PlaneError> {
    let file = preflight_upload(file)?;

    // Try the app assets v2 endpoint first; the raw status decides the
    // fallback (the endpoint is session-only on many deployments).
    let (status, created) = client
        .register_description_asset(project_id, work_item_id, &file.name, &file.mime, file.size)
        .await?;
    if matches!(status, 401 | 403 | 404) {
        let attachment =
            upload_attachment_v1(client, project_id, work_item_id, &file, force).await?;
        return Ok(attachment.id);
    }
    if status != 200 {
        return Err(PlaneError::Api {
            message: format!("Asset upload failed: HTTP {status}"),
        });
    }

    let asset_id = created
        .get("asset_id")
        .and_then(Value::as_str)
        .ok_or_else(|| PlaneError::Api {
            message: "the asset register response did not include an asset id.".into(),
        })?
        .to_string();
    let upload_data = created.get("upload_data").ok_or_else(|| PlaneError::Api {
        message: "the asset register response did not include presigned upload data.".into(),
    })?;

    // Push the bytes to the presigned URL (signature lives in the fields).
    let bytes = std::fs::read(file.path).map_err(|e| PlaneError::Api {
        message: format!("failed to read {}: {e}", file.path.display()),
    })?;
    client
        .upload_to_presigned(upload_data, &file.name, &file.mime, bytes)
        .await?;

    let (status, _) = client
        .confirm_description_asset(project_id, &asset_id)
        .await?;
    if !matches!(status, 200 | 204) {
        return Err(PlaneError::Api {
            message: format!("Asset confirm failed: HTTP {status}"),
        });
    }

    // A 2xx on the confirm PATCH is not proof the write landed (ADR-0007):
    // read the asset back through the v1 workspace-scoped asset endpoint.
    let (status, _) = client.get_workspace_asset(&asset_id).await?;
    if status != 200 {
        return Err(PlaneError::Api {
            message: "Image asset was not confirmed by the server after upload.".into(),
        });
    }
    Ok(asset_id)
}

/// Upload a file to a work item (mirrors the Python `upload_attachment`):
/// preflight -> [`upload_attachment_v1`] -> output.
async fn cmd_attachment_attach(
    client: &PbotClient,
    issue: &str,
    file: &str,
    project: Option<&str>,
    force: bool,
    json: bool,
) -> Result<(), PlaneError> {
    let located = locate_issue(client, issue, project).await?;
    let file = preflight_upload(file)?;
    let size = file.size;
    let name = file.name.clone();

    let mine =
        upload_attachment_v1(client, &located.project_id, &located.item.id, &file, force).await?;

    if json {
        output_json(&attachment_json(&mine));
    } else {
        eprintln!("Attached {name} ({size} bytes) to {issue}.");
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

async fn cmd_wi_delete(
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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

async fn cmd_user_list(client: &PbotClient, json: bool) -> Result<(), PlaneError> {
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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
    client: &PbotClient,
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

/// The pages scope a `doc` command targets: the project's pages when `-p` is
/// given, the workspace's pages otherwise (mirrors the Python commands, where
/// every scope decision is `project → project pages, else workspace pages`).
async fn page_scope(client: &PbotClient, project: Option<&str>) -> Result<PageScope, PlaneError> {
    match project {
        Some(p) => {
            let proj = resolve_project(p, client).await?;
            Ok(PageScope::Project(proj.id))
        }
        None => Ok(PageScope::Workspace),
    }
}

/// Resolve a page reference by UUID (direct fetch) or fuzzy name (against the
/// scope's page list), like the other resource resolvers.
async fn resolve_page(
    client: &PbotClient,
    scope: &PageScope,
    query: &str,
) -> Result<Page, PlaneError> {
    if planebotcli_resolve::is_uuid(query) {
        return client.get_page(scope, query).await;
    }
    let pages = client.list_pages(scope).await?;
    let found =
        planebotcli_resolve::find_best_match(query, &pages, |p| p.name.as_deref().unwrap_or(""));
    match found {
        Some(m) => Ok(m.item.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Page not found: {query}"),
        }),
    }
}

/// Text of a page's body with HTML tags stripped, mirroring the Python
/// `_enrich_doc` `content_text` derivation.
fn page_content_text(page: &Page) -> String {
    page.description_html
        .as_deref()
        .map(planebotcli_html::strip_html_tags)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Serialize a page to JSON with the computed `content_text` field added, the
/// same shape the Python CLI outputs for `--json`.
fn page_json(page: &Page) -> Value {
    let value = serde_json::to_value(page).unwrap_or(Value::Null);
    let Value::Object(mut map) = value else {
        return Value::Null;
    };
    map.insert(
        "content_text".to_string(),
        Value::String(page_content_text(page)),
    );
    Value::Object(map)
}

/// Print a single page: JSON to stdout, or a Field/Value table to stderr.
fn output_page_view(page: &Page, json: bool) {
    if json {
        output_json(&page_json(page));
        return;
    }
    let rows = vec![
        vec!["id".to_string(), page.id.clone()],
        vec!["name".to_string(), page.name.clone().unwrap_or_default()],
        vec!["content_text".to_string(), page_content_text(page)],
        vec![
            "created_at".to_string(),
            fmt_ts(page.created_at.as_deref().unwrap_or("")),
        ],
        vec![
            "updated_at".to_string(),
            fmt_ts(page.updated_at.as_deref().unwrap_or("")),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

async fn cmd_doc_list(
    client: &PbotClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    // Python parity: listing documents is project-scoped only.
    let pid = match project {
        Some(p) => resolve_project(p, client).await?.id,
        None => {
            return Err(PlaneError::Validation {
                message: "Project is required for listing documents.".into(),
                hint: Some("Use -p/--project <name-or-id> to specify the project.".into()),
            });
        }
    };
    let scope = planebotcli_client::PageScope::Project(pid);
    let pages = client.list_pages(&scope).await?;
    if json {
        let views: Vec<Value> = pages.iter().map(page_json).collect();
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = pages
            .iter()
            .map(|p| {
                vec![
                    p.id.clone(),
                    p.name.clone().unwrap_or_default(),
                    fmt_ts(p.created_at.as_deref().unwrap_or("")),
                    fmt_ts(p.updated_at.as_deref().unwrap_or("")),
                ]
            })
            .collect();
        output_table(&["ID", "Title", "Created", "Updated"], &rows);
    }
    Ok(())
}

async fn cmd_doc_show(
    client: &PbotClient,
    doc: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let scope = page_scope(client, project).await?;
    let page = resolve_page(client, &scope, doc).await?;
    output_page_view(&page, json);
    Ok(())
}

/// Build a page description_html from the mutually exclusive content flags:
/// `--content-html` verbatim, `--content-md` via md_to_html, `--content` plain
/// text via body_to_html; an empty body becomes an empty paragraph (the API
/// field is mandatory).
fn doc_description(
    content: Option<&str>,
    content_md: Option<&str>,
    content_html: Option<&str>,
) -> Result<String, PlaneError> {
    let given = [
        content.is_some(),
        content_md.is_some(),
        content_html.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();
    if given > 1 {
        return Err(PlaneError::Validation {
            message: "--content, --content-md and --content-html are mutually exclusive.".into(),
            hint: Some("Pass the page content via only one of them.".into()),
        });
    }
    if let Some(h) = content_html {
        return Ok(h.to_string());
    }
    if let Some(m) = content_md {
        return Ok(planebotcli_html::md_to_html(m));
    }
    if let Some(c) = content {
        return Ok(planebotcli_html::body_to_html(c));
    }
    Ok("<p></p>".to_string())
}

async fn cmd_doc_create(
    client: &PbotClient,
    title: &str,
    content: Option<&str>,
    content_md: Option<&str>,
    content_html: Option<&str>,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let scope = page_scope(client, project).await?;
    let write = PageWrite {
        name: Some(title.to_string()),
        description_html: Some(doc_description(content, content_md, content_html)?),
    };
    let created = client.create_page(&scope, &write).await?;
    output_page_view(&created, json);
    Ok(())
}

// A content trio (plain/md/html) plus issue/title/project is inherent to the
// command; grouping further would obscure the clap surface.
#[allow(clippy::too_many_arguments)]
async fn cmd_doc_update(
    client: &PbotClient,
    doc: &str,
    title: Option<&str>,
    content: Option<&str>,
    content_md: Option<&str>,
    content_html: Option<&str>,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let scope = page_scope(client, project).await?;
    let found = resolve_page(client, &scope, doc).await?;
    let mut write = PageWrite::default();
    // Title uses truthiness like the Python `if title:`, so `--title ""` is
    // skipped.
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        write.name = Some(t.to_string());
    }
    if content.is_some() || content_md.is_some() || content_html.is_some() {
        write.description_html = Some(doc_description(content, content_md, content_html)?);
    }
    let updated = client.update_page(&scope, &found.id, &write).await?;
    output_page_view(&updated, json);
    Ok(())
}

/// Delete (trash) a page: archive it first, then DELETE it.
///
/// The API only deletes pages that are already archived and rejects DELETE on
/// an unarchived page with a 400; a bare `is_archived` flag is silently
/// ignored (ADR-0007). So this archives with `archived_at` = today, verifies
/// the response actually carries the date, and only then deletes.
async fn cmd_doc_archive(
    client: &PbotClient,
    doc: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let scope = page_scope(client, project).await?;
    let found = resolve_page(client, &scope, doc).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    let today = today_ymd();
    let raw = client.archive_page(&scope, &found.id, &today).await?;
    if raw.get("archived_at").and_then(Value::as_str) != Some(today.as_str()) {
        return Err(PlaneError::Api {
            message: "the document was not archived. Archiving may require a higher project role."
                .into(),
        });
    }
    eprintln!("Document '{name}' archived.");
    Ok(())
}

async fn cmd_doc_delete(
    client: &PbotClient,
    doc: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let scope = page_scope(client, project).await?;
    let found = resolve_page(client, &scope, doc).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    let today = today_ymd();
    let raw = client.archive_page(&scope, &found.id, &today).await?;
    if raw.get("archived_at").and_then(Value::as_str) != Some(today.as_str()) {
        return Err(PlaneError::Api {
            message: "the document was not archived, so it was not deleted. \
                      Archiving may require a higher project role."
                .into(),
        });
    }
    client.delete_page(&scope, &found.id).await?;
    eprintln!("Document '{name}' deleted.");
    Ok(())
}

/// Today's date as `YYYY-MM-DD`, computed from the system clock (UTC).
fn today_ymd() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Convert days since 1970-01-01 to a (year, month, day) civil date (Howard
/// Hinnant's `civil_from_days` algorithm), so the archive date needs no
/// external date library.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Options for `module create` (grouped to keep the handler signature small).
struct ModuleCreateOpts<'a> {
    project: Option<&'a str>,
    description: Option<&'a str>,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
    status: Option<&'a str>,
}

/// Options for `module update`.
struct ModuleUpdateOpts<'a> {
    project: Option<&'a str>,
    name: Option<&'a str>,
    description: Option<&'a str>,
    status: Option<&'a str>,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
}

/// Options for `cycle create`.
struct CycleCreateOpts<'a> {
    project: Option<&'a str>,
    description: Option<&'a str>,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
}

/// Options for `cycle update`.
struct CycleUpdateOpts<'a> {
    project: Option<&'a str>,
    name: Option<&'a str>,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
}

/// Valid module status values (Plane's ModuleStatusEnum).
const MODULE_STATUSES: [&str; 6] = [
    "backlog",
    "planned",
    "in-progress",
    "paused",
    "completed",
    "cancelled",
];

/// Normalize and validate a `--status` flag. Accepts the same aliases as the
/// Python CLI (English plus Portuguese spellings); error lists valid values.
fn normalize_module_status(value: Option<&str>) -> Result<Option<String>, PlaneError> {
    let Some(value) = value else { return Ok(None) };
    let trimmed = value.trim().to_lowercase();
    let canonical = match trimmed.as_str() {
        "backlog" => "backlog",
        "planned" | "planejado" => "planned",
        "in-progress" | "in progress" | "in_progress" | "inprogress" | "em andamento" => {
            "in-progress"
        }
        "paused" | "pausado" => "paused",
        "completed" | "concluido" | "concluído" => "completed",
        "cancelled" | "canceled" | "cancelado" => "cancelled",
        _ => {
            return Err(PlaneError::Validation {
                message: format!("Invalid status '{value}'."),
                hint: Some(format!("Valid values: {}.", MODULE_STATUSES.join(", "))),
            });
        }
    };
    Ok(Some(canonical.to_string()))
}

/// Resolve a module by UUID or fuzzy name against a project's modules.
async fn resolve_module(
    client: &PbotClient,
    project_id: &str,
    query: &str,
) -> Result<Module, PlaneError> {
    if planebotcli_resolve::is_uuid(query) {
        return client
            .get_module(project_id, query)
            .await
            .map_err(|e| match e {
                PlaneError::NotFound { .. } => PlaneError::NotFound {
                    message: format!("Module not found: {query}"),
                },
                other => other,
            });
    }
    let modules = client.list_modules(project_id).await?;
    let found =
        planebotcli_resolve::find_best_match(query, &modules, |m| m.name.as_deref().unwrap_or(""));
    match found {
        Some(m) => Ok(m.item.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Module not found: {query}"),
        }),
    }
}

/// Resolve a cycle by UUID or fuzzy name against a project's cycles.
async fn resolve_cycle(
    client: &PbotClient,
    project_id: &str,
    query: &str,
) -> Result<Cycle, PlaneError> {
    if planebotcli_resolve::is_uuid(query) {
        return client
            .get_cycle(project_id, query)
            .await
            .map_err(|e| match e {
                PlaneError::NotFound { .. } => PlaneError::NotFound {
                    message: format!("Cycle not found: {query}"),
                },
                other => other,
            });
    }
    let cycles = client.list_cycles(project_id).await?;
    let found =
        planebotcli_resolve::find_best_match(query, &cycles, |c| c.name.as_deref().unwrap_or(""));
    match found {
        Some(c) => Ok(c.item.clone()),
        None => Err(PlaneError::NotFound {
            message: format!("Cycle not found: {query}"),
        }),
    }
}

/// Print a single module: JSON to stdout, or a Field/Value table to stderr.
fn output_module_view(module: &Module, json: bool) {
    if json {
        output_json(module);
        return;
    }
    let rows = vec![
        vec!["id".to_string(), module.id.clone()],
        vec!["name".to_string(), module.name.clone().unwrap_or_default()],
        vec![
            "description".to_string(),
            module.description.clone().unwrap_or_default(),
        ],
        vec![
            "status".to_string(),
            module.status.clone().unwrap_or_default(),
        ],
        vec![
            "start_date".to_string(),
            module.start_date.clone().unwrap_or_default(),
        ],
        vec![
            "target_date".to_string(),
            module.target_date.clone().unwrap_or_default(),
        ],
        vec![
            "created_at".to_string(),
            module.created_at.clone().unwrap_or_default(),
        ],
        vec![
            "updated_at".to_string(),
            module.updated_at.clone().unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

/// Print a single cycle: JSON to stdout, or a Field/Value table to stderr.
fn output_cycle_view(cycle: &Cycle, json: bool) {
    if json {
        output_json(cycle);
        return;
    }
    let count = |v: &Option<i64>| v.map(|n| n.to_string()).unwrap_or_default();
    let rows = vec![
        vec!["id".to_string(), cycle.id.clone()],
        vec!["name".to_string(), cycle.name.clone().unwrap_or_default()],
        vec![
            "description".to_string(),
            cycle.description.clone().unwrap_or_default(),
        ],
        vec![
            "start_date".to_string(),
            cycle.start_date.clone().unwrap_or_default(),
        ],
        vec![
            "end_date".to_string(),
            cycle.end_date.clone().unwrap_or_default(),
        ],
        vec![
            "owned_by".to_string(),
            cycle.owned_by.clone().unwrap_or_default(),
        ],
        vec!["total_issues".to_string(), count(&cycle.total_issues)],
        vec![
            "completed_issues".to_string(),
            count(&cycle.completed_issues),
        ],
        vec!["started_issues".to_string(), count(&cycle.started_issues)],
        vec![
            "unstarted_issues".to_string(),
            count(&cycle.unstarted_issues),
        ],
        vec!["backlog_issues".to_string(), count(&cycle.backlog_issues)],
        vec![
            "cancelled_issues".to_string(),
            count(&cycle.cancelled_issues),
        ],
        vec![
            "created_at".to_string(),
            cycle.created_at.clone().unwrap_or_default(),
        ],
        vec![
            "updated_at".to_string(),
            cycle.updated_at.clone().unwrap_or_default(),
        ],
    ];
    output_table(&["Field", "Value"], &rows);
}

async fn cmd_module_list(
    client: &PbotClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let mut modules = client.list_modules(&proj.id).await?;
    // Newest first, mirroring the Python list default sort.
    modules.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    if json {
        output_json(&modules);
    } else {
        let rows: Vec<Vec<String>> = modules
            .iter()
            .map(|m| {
                vec![
                    m.id.clone(),
                    m.name.clone().unwrap_or_default(),
                    m.status.clone().unwrap_or_default(),
                    m.start_date.clone().unwrap_or_default(),
                    m.target_date.clone().unwrap_or_default(),
                ]
            })
            .collect();
        output_table(&["ID", "Name", "Status", "Start", "End"], &rows);
    }
    Ok(())
}

async fn cmd_module_show(
    client: &PbotClient,
    module: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let module = resolve_module(client, &proj.id, module).await?;
    output_module_view(&module, json);
    Ok(())
}

async fn cmd_module_create(
    client: &PbotClient,
    name: &str,
    opts: &ModuleCreateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    // Validate flags before touching the network (Python normalizes status first).
    let start_date = validate_date(opts.start_date, "--start-date")?;
    let end_date = validate_date(opts.end_date, "--end-date")?;
    let status = normalize_module_status(opts.status)?;
    let proj = require_project(client, opts.project).await?;
    let mut write = ModuleWrite {
        name: Some(name.to_string()),
        ..Default::default()
    };
    if let Some(description) = opts.description {
        write.description = Some(description.to_string());
    }
    if let Some(d) = start_date {
        write.start_date = Some(d);
    }
    // The API stores the end date under target_date.
    if let Some(d) = end_date {
        write.target_date = Some(d);
    }
    if let Some(s) = status {
        write.status = Some(s);
    }
    let created = client.create_module(&proj.id, &write).await?;
    output_module_view(&created, json);
    Ok(())
}

async fn cmd_module_update(
    client: &PbotClient,
    module: &str,
    opts: &ModuleUpdateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    // Validate flags before touching the network (Python normalizes status first).
    let status = normalize_module_status(opts.status)?;
    let proj = require_project(client, opts.project).await?;
    let found = resolve_module(client, &proj.id, module).await?;
    let start_date = validate_date(opts.start_date, "--start-date")?;
    let end_date = validate_date(opts.end_date, "--end-date")?;
    let mut write = ModuleWrite::default();
    if let Some(name) = opts.name {
        write.name = Some(name.to_string());
    }
    if let Some(description) = opts.description {
        write.description = Some(description.to_string());
    }
    if let Some(s) = status {
        write.status = Some(s);
    }
    if let Some(d) = start_date {
        write.start_date = Some(d);
    }
    if let Some(d) = end_date {
        write.target_date = Some(d);
    }
    let updated = client.update_module(&proj.id, &found.id, &write).await?;
    output_module_view(&updated, json);
    Ok(())
}

async fn cmd_module_delete(
    client: &PbotClient,
    module: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_module(client, &proj.id, module).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    client.delete_module(&proj.id, &found.id).await?;
    eprintln!("Module '{name}' deleted.");
    Ok(())
}

async fn cmd_cycle_list(
    client: &PbotClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let mut cycles = client.list_cycles(&proj.id).await?;
    // Newest first, mirroring the Python list default sort.
    cycles.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    if json {
        output_json(&cycles);
    } else {
        let count = |v: &Option<i64>| v.map(|n| n.to_string()).unwrap_or_default();
        let rows: Vec<Vec<String>> = cycles
            .iter()
            .map(|c| {
                vec![
                    c.id.clone(),
                    c.name.clone().unwrap_or_default(),
                    c.start_date.clone().unwrap_or_default(),
                    c.end_date.clone().unwrap_or_default(),
                    count(&c.total_issues),
                    count(&c.completed_issues),
                ]
            })
            .collect();
        output_table(&["ID", "Name", "Start", "End", "Issues", "Done"], &rows);
    }
    Ok(())
}

async fn cmd_cycle_show(
    client: &PbotClient,
    cycle: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let cycle = resolve_cycle(client, &proj.id, cycle).await?;
    output_cycle_view(&cycle, json);
    Ok(())
}

async fn cmd_cycle_create(
    client: &PbotClient,
    name: &str,
    opts: &CycleCreateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let start_date = validate_date(opts.start_date, "--start-date")?;
    let end_date = validate_date(opts.end_date, "--end-date")?;
    let proj = require_project(client, opts.project).await?;
    // Cycles are owned by the authenticated user on create (like the Python
    // SDK's CreateCycle, which requires owned_by).
    let me = client.get_me().await?;
    let mut write = CycleWrite {
        name: Some(name.to_string()),
        owned_by: Some(me.id),
        project_id: Some(proj.id.clone()),
        ..Default::default()
    };
    if let Some(description) = opts.description {
        write.description = Some(description.to_string());
    }
    if let Some(d) = start_date {
        write.start_date = Some(d);
    }
    if let Some(d) = end_date {
        write.end_date = Some(d);
    }
    let created = client.create_cycle(&proj.id, &write).await?;
    output_cycle_view(&created, json);
    Ok(())
}

async fn cmd_cycle_update(
    client: &PbotClient,
    cycle: &str,
    opts: &CycleUpdateOpts<'_>,
    json: bool,
) -> Result<(), PlaneError> {
    let start_date = validate_date(opts.start_date, "--start-date")?;
    let end_date = validate_date(opts.end_date, "--end-date")?;
    let proj = require_project(client, opts.project).await?;
    let found = resolve_cycle(client, &proj.id, cycle).await?;
    let mut write = CycleWrite::default();
    if let Some(name) = opts.name {
        write.name = Some(name.to_string());
    }
    if let Some(d) = start_date {
        write.start_date = Some(d);
    }
    if let Some(d) = end_date {
        write.end_date = Some(d);
    }
    let updated = client.update_cycle(&proj.id, &found.id, &write).await?;
    output_cycle_view(&updated, json);
    Ok(())
}

async fn cmd_cycle_delete(
    client: &PbotClient,
    cycle: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found = resolve_cycle(client, &proj.id, cycle).await?;
    let name = found.name.clone().unwrap_or_else(|| found.id.clone());
    client.delete_cycle(&proj.id, &found.id).await?;
    eprintln!("Cycle '{name}' deleted.");
    Ok(())
}

async fn cmd_cycle_add_item(
    client: &PbotClient,
    cycle: &str,
    work_item: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found_cycle = resolve_cycle(client, &proj.id, cycle).await?;
    let located = locate_work_item_in_project(work_item, &proj, client).await?;
    client
        .add_work_item_to_cycle(&proj.id, &found_cycle.id, &located.item.id)
        .await?;
    let name = found_cycle
        .name
        .clone()
        .unwrap_or_else(|| found_cycle.id.clone());
    eprintln!("Work item added to cycle '{name}'.");
    Ok(())
}

async fn cmd_cycle_remove_item(
    client: &PbotClient,
    cycle: &str,
    work_item: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found_cycle = resolve_cycle(client, &proj.id, cycle).await?;
    let located = locate_work_item_in_project(work_item, &proj, client).await?;
    client
        .remove_work_item_from_cycle(&proj.id, &found_cycle.id, &located.item.id)
        .await?;
    let name = found_cycle
        .name
        .clone()
        .unwrap_or_else(|| found_cycle.id.clone());
    eprintln!("Work item removed from cycle '{name}'.");
    Ok(())
}

async fn cmd_cycle_items(
    client: &PbotClient,
    base_url: &str,
    cycle: &str,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let found_cycle = resolve_cycle(client, &proj.id, cycle).await?;
    let workspace = client.workspace().to_string();
    let identifier = proj.identifier.clone().unwrap_or_default();

    let items = client
        .list_cycle_work_items(&proj.id, &found_cycle.id)
        .await?;
    let states = client.list_states(&proj.id).await?;
    let labels_list = client.list_labels(&proj.id).await?;
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
    let views: Vec<Value> = items
        .iter()
        .map(|item| work_item_view(item, &identifier, &lookups, base_url, &workspace))
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
                ]
            })
            .collect();
        output_table(&["ID", "Title", "Priority", "State"], &rows);
    }
    Ok(())
}

/// Valid intake priorities — words only. The Python intake rejects the numeric
/// aliases (`0`-`4`) that `wi create` accepts, and an explicit empty string.
const INTAKE_PRIORITIES: [&str; 5] = ["none", "low", "medium", "high", "urgent"];

/// Normalize and validate an intake `--priority` flag. An explicit `--priority
/// ""` reaches this validator and is rejected (never silently defaulted).
fn normalize_intake_priority(value: &str) -> Result<String, PlaneError> {
    let key = value.trim().to_lowercase();
    if !INTAKE_PRIORITIES.contains(&key.as_str()) {
        return Err(PlaneError::Validation {
            message: format!("Invalid priority '{value}'."),
            hint: Some(format!("Valid values: {}.", INTAKE_PRIORITIES.join(", "))),
        });
    }
    Ok(key)
}

/// HTML-escape a plain-text fragment, mirroring Python's `html.escape`.
/// Intake descriptions are escaped so tags render as text (unlike `wi
/// create`, which passes raw HTML through untouched).
fn html_escape_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

async fn cmd_intake_list(
    client: &PbotClient,
    project: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let items = client.list_intake(&proj.id).await?;
    let views: Vec<Value> = items.iter().map(intake_view).collect();
    if json {
        output_json(&views);
    } else {
        let rows: Vec<Vec<String>> = views
            .iter()
            .map(|v| {
                vec![
                    v["name"].as_str().unwrap_or("").to_string(),
                    v["priority"].as_str().unwrap_or("").to_string(),
                    v["status"].as_str().unwrap_or("").to_string(),
                    fmt_ts(v["created_at"].as_str().unwrap_or("")),
                    v["issue_id"].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        output_table(
            &["Name", "Priority", "Status", "Created", "Issue ID"],
            &rows,
        );
    }
    Ok(())
}

async fn cmd_intake_create(
    client: &PbotClient,
    name: &str,
    project: Option<&str>,
    description: Option<&str>,
    priority: Option<&str>,
    json: bool,
) -> Result<(), PlaneError> {
    // Validate before touching the network (Python normalizes priority first).
    let priority = match priority {
        Some(raw) => normalize_intake_priority(raw)?,
        None => "none".to_string(),
    };
    let proj = require_project(client, project).await?;
    let write = IntakeWrite {
        issue: IntakeIssueWrite {
            name: name.to_string(),
            description_html: description.map(|d| format!("<p>{}</p>", html_escape_text(d))),
            priority: Some(priority),
        },
    };
    let created = client.create_intake(&proj.id, &write).await?;
    output_intake_item(&created, json);
    Ok(())
}

/// Print a single intake row: JSON to stdout, or a Field/Value table to stderr
/// (Python's `INTAKE_FIELDS` ordering and labels).
fn output_intake_item(item: &IntakeItem, json: bool) {
    if json {
        output_json(&intake_view(item));
        return;
    }
    let view = intake_view(item);
    let s = |key: &str| view[key].as_str().unwrap_or("").to_string();
    let rows = vec![
        vec!["Issue ID".to_string(), s("issue_id")],
        vec!["Intake ID".to_string(), s("id")],
        vec!["Name".to_string(), s("name")],
        vec!["Priority".to_string(), s("priority")],
        vec!["Status".to_string(), s("status")],
        vec!["Created".to_string(), fmt_ts(&s("created_at"))],
        vec!["Updated".to_string(), fmt_ts(&s("updated_at"))],
    ];
    output_table(&["Field", "Value"], &rows);
}

/// Shared accept/decline body: PATCH the intake status of a work item and
/// verify the write (ADR-0007). The API answers HTTP 200 with the record
/// untouched when the caller is not a project Admin, so a 200 alone is not
/// proof the triage happened.
async fn cmd_intake_triage(
    client: &PbotClient,
    issue_id: &str,
    project: Option<&str>,
    status: i64,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    let raw = client
        .update_intake_status(&proj.id, issue_id, status)
        .await?;
    let current = raw.get("status").and_then(Value::as_i64);
    if current != Some(status) {
        return Err(PlaneError::Api {
            message: format!(
                "the intake status was not changed (still '{}'). Triaging intake items requires the project Admin role.",
                intake_status_label(current)
            ),
        });
    }
    let item: IntakeItem = serde_json::from_value(raw).map_err(|e| PlaneError::Api {
        message: format!("invalid intake response after triage: {e}"),
    })?;
    output_intake_item(&item, json);
    Ok(())
}

async fn cmd_intake_delete(
    client: &PbotClient,
    issue_id: &str,
    project: Option<&str>,
) -> Result<(), PlaneError> {
    let proj = require_project(client, project).await?;
    client.delete_intake(&proj.id, issue_id).await?;
    eprintln!("Intake item {issue_id} deleted.");
    Ok(())
}

async fn cmd_intake_enabled(
    client: &PbotClient,
    project: &str,
    json: bool,
) -> Result<(), PlaneError> {
    let proj = resolve_project(project, client).await?;
    let is_enabled = proj.intake_view.unwrap_or(false);
    let name = proj.name.clone().unwrap_or_else(|| project.to_string());
    if json {
        output_json(&json!({
            "project": name,
            "project_id": proj.id,
            "intake_enabled": is_enabled,
        }));
    } else if is_enabled {
        eprintln!("Intake is enabled for {name} (ID: {})", proj.id);
    } else {
        eprintln!("Intake is NOT enabled for {name} (ID: {})", proj.id);
    }
    Ok(())
}

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

    #[test]
    fn intake_priority_words_only_case_and_space_insensitive() {
        assert_eq!(normalize_intake_priority("urgent").unwrap(), "urgent");
        assert_eq!(normalize_intake_priority("URGENT").unwrap(), "urgent");
        assert_eq!(normalize_intake_priority("  High ").unwrap(), "high");
        assert_eq!(normalize_intake_priority("None").unwrap(), "none");
        // Numeric aliases and the empty string are rejected, unlike `wi`.
        for bad in ["1", "2", "", "  ", "urgnet", "critical"] {
            assert!(
                normalize_intake_priority(bad).is_err(),
                "{bad:?} should fail"
            );
        }
    }

    #[test]
    fn intake_status_labels() {
        assert_eq!(intake_status_label(Some(-2)), "pending");
        assert_eq!(intake_status_label(Some(-1)), "rejected");
        assert_eq!(intake_status_label(Some(0)), "snoozed");
        assert_eq!(intake_status_label(Some(1)), "accepted");
        assert_eq!(intake_status_label(Some(2)), "duplicate");
        assert_eq!(intake_status_label(None), "");
        assert_eq!(intake_status_label(Some(99)), "99"); // unknown code shown raw
    }

    #[test]
    fn html_escape_matches_python_escape() {
        assert_eq!(html_escape_text("a < b & c"), "a &lt; b &amp; c");
        assert_eq!(
            html_escape_text("quote \" and '"),
            "quote &quot; and &#x27;"
        );
        assert_eq!(html_escape_text("plain"), "plain");
    }

    #[test]
    fn intake_view_flattens_issue_detail_and_maps_status() {
        let item = IntakeItem {
            id: "intake-1".into(),
            issue: Some("issue-1".into()),
            status: Some(-2),
            issue_detail: Some(json!({"id": "issue-1", "name": "Bug report", "priority": "high"})),
            ..Default::default()
        };
        let view = intake_view(&item);
        assert_eq!(view["name"], "Bug report");
        assert_eq!(view["priority"], "high");
        assert_eq!(view["issue_id"], "issue-1");
        assert_eq!(view["status"], "pending");
        assert_eq!(view["id"], "intake-1");
    }

    #[test]
    fn intake_view_missing_detail_uses_defaults_and_top_level_issue() {
        let item = IntakeItem {
            id: "intake-1".into(),
            issue: Some("issue-1".into()),
            status: Some(99),
            issue_detail: None,
            ..Default::default()
        };
        let view = intake_view(&item);
        assert_eq!(view["name"], "");
        assert_eq!(view["priority"], "none");
        assert_eq!(view["issue_id"], "issue-1"); // falls back to top-level `issue`
        assert_eq!(view["status"], "99"); // unknown code is shown raw, not hidden
    }

    #[test]
    fn intake_view_issue_id_falls_back_to_issue_detail_id() {
        let item = IntakeItem {
            id: "intake-1".into(),
            issue: None,
            status: None,
            issue_detail: Some(json!({"id": "issue-9", "name": "n"})),
            ..Default::default()
        };
        let view = intake_view(&item);
        assert_eq!(view["issue_id"], "issue-9");
        assert_eq!(view["status"], "");
    }

    #[test]
    fn civil_from_days_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(10_957), (2000, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(20_705), (2026, 9, 9));
        assert_eq!(civil_from_days(20_818), (2026, 12, 31));
        // The rendered archive date is a YYYY-MM-DD string.
        assert_eq!(today_ymd().len(), 10);
    }

    #[test]
    fn page_json_adds_stripped_content_text() {
        let page = Page {
            id: "page-1".into(),
            name: Some("Notes".into()),
            description_html: Some("<p>hello<br/>world</p>".into()),
            ..Default::default()
        };
        let view = page_json(&page);
        assert_eq!(view["content_text"], "helloworld");
        assert_eq!(view["name"], "Notes");
        assert_eq!(view["id"], "page-1");
    }

    #[test]
    fn page_json_empty_description_is_empty_string() {
        let page = Page {
            id: "page-2".into(),
            name: None,
            description_html: None,
            ..Default::default()
        };
        let view = page_json(&page);
        assert_eq!(view["content_text"], "");
    }

    #[test]
    fn mime_guesses_by_extension_and_defaults_to_octet_stream() {
        assert_eq!(guess_mime("shot.png"), "image/png");
        assert_eq!(guess_mime("notes.JPG"), "image/jpeg");
        assert_eq!(guess_mime("spec.pdf"), "application/pdf");
        assert_eq!(guess_mime("archive.tar.gz"), "application/gzip");
        assert_eq!(guess_mime("noextension"), "application/octet-stream");
        assert_eq!(guess_mime("mystery.xyz"), "application/octet-stream");
        assert_eq!(guess_mime(""), "application/octet-stream");
    }

    #[test]
    fn embed_html_holds_only_the_asset_uuid_in_src() {
        // The web editor resolves an asset UUID in `src` at render time; a
        // full path/URL renders "Error loading image" (Python `embed_html`).
        assert_eq!(
            embed_html("a3f1c2d4-0000-4000-8000-1234567890ab"),
            r#"<p><img src="a3f1c2d4-0000-4000-8000-1234567890ab" /></p>"#
        );
    }

    #[test]
    fn embed_seed_desc_md_wins_over_description() {
        let parts = embed_seed_parts(Some("## Title"), Some("plain text"), None);
        assert_eq!(parts, vec!["<h2>Title</h2>"]);
    }

    #[test]
    fn embed_seed_plain_description_is_wrapped() {
        let parts = embed_seed_parts(None, Some("hello"), None);
        assert_eq!(parts, vec!["<p>hello</p>"]);
    }

    #[test]
    fn embed_seed_empty_description_falls_back_to_existing() {
        let parts = embed_seed_parts(None, Some(""), Some("<p>old</p>"));
        assert_eq!(parts, vec!["<p>old</p>"]);
    }

    #[test]
    fn embed_seed_update_appends_to_existing_description() {
        let parts = embed_seed_parts(None, None, Some("<p>old</p>"));
        assert_eq!(parts, vec!["<p>old</p>"]);
    }

    #[test]
    fn embed_seed_create_without_description_starts_empty() {
        // Create has no `existing` fallback: images only, no leading text.
        assert!(embed_seed_parts(None, None, None).is_empty());
        assert!(embed_seed_parts(None, Some(""), None).is_empty());
    }

    #[test]
    fn relation_type_validation_accepts_aliases_and_rejects_unknown() {
        for known in RELATION_TYPES {
            assert_eq!(normalize_relation_type(known).unwrap(), known);
        }
        assert_eq!(normalize_relation_type("BLOCKING").unwrap(), "blocking");
        assert_eq!(normalize_relation_type("relates-to").unwrap(), "relates_to");
        assert_eq!(
            normalize_relation_type("  finish_after  ").unwrap(),
            "finish_after"
        );
        let err = normalize_relation_type("blocks").unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        // The error lists every allowed type.
        for known in RELATION_TYPES {
            assert!(err.to_string().contains(known), "{err}");
        }
    }

    #[test]
    fn relation_rows_flatten_buckets_in_type_order_and_skip_empties() {
        let relations = json!({
            "blocking": [{"project_id": "p1", "issue_id": "wi2"}],
            "blocked_by": [],
            "relates_to": [{"project_id": "p2", "issue_id": "wi9"}],
            "duplicate": [],
            "start_after": [],
            "start_before": [],
            "finish_after": [],
            "finish_before": [],
        });
        assert_eq!(
            relation_rows(&relations),
            vec![
                ("blocking".to_string(), "p1".to_string(), "wi2".to_string()),
                (
                    "relates_to".to_string(),
                    "p2".to_string(),
                    "wi9".to_string()
                ),
            ]
        );
        // A null/absent body flattens to nothing rather than panicking.
        assert!(relation_rows(&Value::Null).is_empty());
    }

    #[test]
    fn parent_reference_classifies_same_and_cross_project() {
        assert_eq!(
            classify_parent_reference("PLANECLI-38", "PLANECLI"),
            ParentReference::SameProject
        );
        assert_eq!(
            classify_parent_reference("planecli-38", "PLANECLI"),
            ParentReference::SameProject
        );
        assert_eq!(
            classify_parent_reference("OTHER-1", "PLANECLI"),
            ParentReference::CrossProject("OTHER".to_string())
        );
        // A UUID's trailing segment is not a sequence number.
        assert_eq!(
            classify_parent_reference("5963a34f-1038-43b2-90aa-cc2b183a30e2", "PLANECLI"),
            ParentReference::SameProject
        );
        // Names are not identifier prefixed, even with a trailing number.
        assert_eq!(
            classify_parent_reference("Some feature name", "PLANECLI"),
            ParentReference::SameProject
        );
        assert_eq!(
            classify_parent_reference("Release-2026", "PLANECLI"),
            ParentReference::SameProject
        );
        // An unknown project identifier can never prove a reference foreign.
        assert_eq!(
            classify_parent_reference("OTHER-1", ""),
            ParentReference::SameProject
        );
    }

    #[test]
    fn parent_self_reference_is_rejected() {
        let err = validate_resolved_parent("PLANECLI-38", "wi1", "p1", "PLANECLI", "wi1", "p1")
            .unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        assert!(err.to_string().contains("own parent"), "{err}");
    }

    #[test]
    fn parent_cross_project_reference_is_rejected() {
        let err = validate_resolved_parent("PLANECLI-38", "wi1", "p1", "PLANECLI", "wi2", "p2")
            .unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        assert!(err.to_string().contains("same project"), "{err}");
        // An empty target project (unresolved) is not a mismatch.
        assert!(
            validate_resolved_parent("PLANECLI-38", "wi1", "p1", "PLANECLI", "wi2", "").is_ok()
        );
    }

    #[test]
    fn parent_same_project_is_accepted() {
        assert!(
            validate_resolved_parent("PLANECLI-38", "wi1", "p1", "PLANECLI", "wi2", "p1").is_ok()
        );
    }

    #[test]
    fn parent_candidates_are_composed_and_capped() {
        let items: Vec<WorkItem> = (1..=40)
            .map(|n| WorkItem {
                id: format!("wi{n}"),
                sequence_id: Some(Value::from(n)),
                ..Default::default()
            })
            .collect();
        let candidates = parent_candidates(&items, "PLANECLI");
        assert_eq!(candidates.len(), 15);
        assert_eq!(candidates[0], "PLANECLI-1");
        assert_eq!(candidates[14], "PLANECLI-15");
        // Without a project identifier nothing composes, so nothing is listed.
        assert!(parent_candidates(&items, "").is_empty());
    }

    #[test]
    fn project_stub_carries_id_and_identifier() {
        let located = planebotcli_resolve::LocatedWorkItem {
            item: WorkItem {
                id: "wi1".into(),
                sequence_id: Some(Value::from(7)),
                ..Default::default()
            },
            project_id: "p1".into(),
            project_identifier: "PLANECLI".into(),
        };
        let stub = project_stub(&located);
        assert_eq!(stub.id, "p1");
        assert_eq!(stub.identifier.as_deref(), Some("PLANECLI"));
        assert_eq!(issue_label(&located), "PLANECLI-7");
        assert_eq!(short_uuid("5963a34f-1038"), "5963a34f");
    }

    #[test]
    fn relations_remove_reports_the_missing_backend_endpoint() {
        let err = cmd_relations_remove().unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        assert!(err.to_string().contains("no relations delete endpoint"));
        assert!(err.hint().is_some());
    }

    #[test]
    fn blank_parent_query_is_rejected_and_trimmed() {
        let err = validate_parent_query("   ").unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        assert!(err.hint().is_some());
        assert_eq!(validate_parent_query(" PLANECLI-3 ").unwrap(), "PLANECLI-3");
    }

    #[test]
    fn relation_targets_must_be_present_and_non_blank() {
        let err = validate_relation_targets(&[]).unwrap_err();
        assert!(matches!(err, PlaneError::Validation { .. }), "{err}");
        assert_eq!(err.exit_code(), 5);
        assert!(
            validate_relation_targets(&["PLANECLI-3".to_string(), "PLANECLI-4".to_string()])
                .is_ok()
        );
        let blank = validate_relation_targets(&["PLANECLI-3".to_string(), " ".to_string()]);
        assert!(matches!(blank, Err(PlaneError::Validation { .. })));
    }
}

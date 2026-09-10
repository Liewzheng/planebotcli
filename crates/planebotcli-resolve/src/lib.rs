//! Fuzzy resolution of resource references: UUID, identifier, or fuzzy name.
//!
//! Mirrors the removed Python line's `utils/fuzzy.py` (rapidfuzz
//! token_sort_ratio, threshold 60) and `utils/resolve.py`.

use planebotcli_client::PlaneClient;
use planebotcli_core::PlaneError;
use planebotcli_types::Project;

/// Minimum similarity (0-100) for a fuzzy match to be accepted.
pub const MATCH_THRESHOLD: f64 = 60.0;

/// token_sort_ratio similarity in [0, 100].
///
/// rapidfuzz (Rust) dropped `token_sort_ratio` after 0.4; it is reimplemented
/// here on top of `fuzz::ratio`, which is exactly what the algorithm does:
/// lowercase, sort the whitespace-separated tokens of each side, join, ratio.
pub fn token_sort_ratio(a: &str, b: &str) -> f64 {
    let sort_tokens = |s: &str| -> String {
        let lower = s.to_lowercase();
        let mut tokens: Vec<&str> = lower.split_whitespace().collect();
        tokens.sort_unstable();
        tokens.join(" ")
    };
    let sa = sort_tokens(a);
    let sb = sort_tokens(b);
    // rapidfuzz (Rust) scores on a 0-1 scale; the CLI contract uses 0-100.
    rapidfuzz::fuzz::ratio(sa.chars(), sb.chars()) * 100.0
}

pub struct BestMatch<T> {
    pub item: T,
    pub score: f64,
}

/// Return the item (and score) closest to `query` with score >= threshold.
/// A blank query never matches — an empty string would otherwise score 100
/// against a resource with an empty name.
pub fn find_best_match<'a, T, F>(query: &str, items: &'a [T], key: F) -> Option<BestMatch<&'a T>>
where
    F: Fn(&T) -> &str,
{
    if query.trim().is_empty() {
        return None;
    }
    let mut best: Option<(f64, &'a T)> = None;
    for item in items {
        let score = token_sort_ratio(query, key(item));
        if score >= MATCH_THRESHOLD && best.is_none_or(|(bs, _)| score > bs) {
            best = Some((score, item));
        }
    }
    best.map(|(score, item)| BestMatch { item, score })
}

/// True when `s` looks like a UUID (8-4-4-4-12 hex).
pub fn is_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lengths = [8usize, 4, 4, 4, 12];
    parts
        .iter()
        .zip(lengths)
        .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}

/// How a project query was matched: exact identifier, exact name, or fuzzy name.
pub enum ProjectMatch {
    ExactIdentifier(Project),
    ExactName(Project),
    Fuzzy(Project),
}

/// Match a project reference: exact identifier first (case-insensitive), then exact
/// name (case-insensitive), then fuzzy name. Exact hits win over a closer fuzzy match
/// — an identifier like `RENG` must never be resolved onto a different project by name.
pub fn match_project(query: &str, projects: &[Project]) -> Option<ProjectMatch> {
    projects
        .iter()
        .find(|p| {
            p.identifier
                .as_deref()
                .is_some_and(|i| i.eq_ignore_ascii_case(query))
        })
        .map(|p| ProjectMatch::ExactIdentifier(p.clone()))
        .or_else(|| {
            projects
                .iter()
                .find(|p| {
                    p.name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(query))
                })
                .map(|p| ProjectMatch::ExactName(p.clone()))
        })
        .or_else(|| {
            find_best_match(query, projects, |p| p.name.as_deref().unwrap_or(""))
                .map(|m| ProjectMatch::Fuzzy(m.item.clone()))
        })
}

/// Resolve a project by UUID, exact identifier, exact name, or fuzzy name.
/// A fuzzy (name-only) fallback prints a warning instead of silently landing on
/// a different project.
pub async fn resolve_project(query: &str, client: &PlaneClient) -> Result<Project, PlaneError> {
    if is_uuid(query) {
        return client.get_project(query).await;
    }
    let projects = client.list_projects().await?;
    match match_project(query, &projects) {
        Some(ProjectMatch::ExactIdentifier(p)) | Some(ProjectMatch::ExactName(p)) => Ok(p),
        Some(ProjectMatch::Fuzzy(p)) => {
            eprintln!(
                "warning: no exact project named or identified '{query}'; matched '{}' by fuzzy name — use the exact name or identifier to avoid landing on the wrong project",
                p.name.as_deref().unwrap_or_default()
            );
            Ok(p)
        }
        None => Err(PlaneError::NotFound {
            message: format!("Project not found: {query}"),
        }),
    }
}

/// Resolve a user reference to an (id, display name) pair: `me` resolves to
/// the authenticated user, a UUID is accepted as-is, otherwise the query is
/// fuzzy-matched against workspace member names.
pub async fn resolve_user_query(
    query: &str,
    client: &PlaneClient,
) -> Result<(String, String), PlaneError> {
    if query.eq_ignore_ascii_case("me") {
        let me = client.get_me().await?;
        let name = me
            .display_name
            .clone()
            .or_else(|| me.first_name.clone())
            .unwrap_or_default();
        return Ok((me.id, name));
    }
    let members = client.list_members().await?;
    if is_uuid(query) {
        if let Some(m) = members.iter().find(|m| m.id == query) {
            return Ok((m.id.clone(), m.full_name()));
        }
        // Not present in the member list: accept the UUID as-is.
        return Ok((query.to_string(), query.to_string()));
    }
    let with_names: Vec<(planebotcli_types::Member, String)> = members
        .into_iter()
        .map(|m| {
            let name = m.full_name();
            (m, name)
        })
        .collect();
    if let Some(m) = find_best_match(query, &with_names, |(_, name)| name.as_str()) {
        return Ok((m.item.0.id.clone(), m.item.1.clone()));
    }
    Err(PlaneError::NotFound {
        message: format!("User not found: {query}"),
    })
}

/// A located work item: the raw item plus the project context it lives in.
#[derive(Debug, Clone)]
pub struct LocatedWorkItem {
    pub item: planebotcli_types::WorkItem,
    pub project_id: String,
    pub project_identifier: String,
}

/// Match one query (UUID, full identifier `PROJ-123`, or name) against a list
/// of (item, project identifier) candidates.
fn match_work_item(
    query: &str,
    candidates: &[(planebotcli_types::WorkItem, &str)],
) -> Option<LocatedWorkItem> {
    let lower = query.to_lowercase();
    for (item, identifier) in candidates {
        let id_matches = item.id.eq_ignore_ascii_case(query);
        let seq = item.sequence_id.as_ref().map(|v| match v {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => String::new(),
        });
        let composed = seq
            .filter(|s| !s.is_empty() && !identifier.is_empty())
            .map(|s| format!("{identifier}-{s}"));
        let seq_matches = composed
            .as_deref()
            .map(|c| c.eq_ignore_ascii_case(query))
            .unwrap_or(false);
        let name = item.name.as_deref().unwrap_or("");
        if id_matches || seq_matches || (!is_uuid(query) && name.to_lowercase().contains(&lower)) {
            return Some(LocatedWorkItem {
                item: item.clone(),
                project_id: item.project.clone().unwrap_or_default(),
                project_identifier: identifier.to_string(),
            });
        }
    }
    // Fuzzy name match over all candidates.
    let with_names: Vec<(&planebotcli_types::WorkItem, &str, String)> = candidates
        .iter()
        .map(|(item, identifier)| (item, *identifier, item.name.clone().unwrap_or_default()))
        .collect();
    if let Some(m) = find_best_match(query, &with_names, |(_, _, name)| name.as_str()) {
        let (item, identifier, _) = m.item;
        return Some(LocatedWorkItem {
            item: (*item).clone(),
            project_id: item.project.clone().unwrap_or_default(),
            project_identifier: identifier.to_string(),
        });
    }
    None
}

/// Locate a work item within a single project (from its work-item list).
pub async fn locate_work_item_in_project(
    query: &str,
    project: &planebotcli_types::Project,
    client: &PlaneClient,
) -> Result<LocatedWorkItem, PlaneError> {
    let items = client.list_work_items(&project.id).await?;
    let identifier = project.identifier.clone().unwrap_or_default();
    let candidates: Vec<(planebotcli_types::WorkItem, &str)> = items
        .iter()
        .map(|i| (i.clone(), identifier.as_str()))
        .collect();
    match_work_item(query, &candidates).ok_or_else(|| PlaneError::NotFound {
        message: format!("Work item not found: {query}"),
    })
}

/// Locate a work item across all projects, matching by UUID, full identifier,
/// or name. Cross-project searches fire many requests, so projects are paced.
pub async fn locate_work_item_across(
    query: &str,
    client: &PlaneClient,
) -> Result<LocatedWorkItem, PlaneError> {
    let projects = client.list_projects().await?;
    let mut candidates: Vec<(planebotcli_types::WorkItem, String)> = Vec::new();
    for (i, project) in projects.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        let items = client.list_work_items(&project.id).await?;
        let identifier = project.identifier.clone().unwrap_or_default();
        candidates.extend(items.into_iter().map(|item| (item, identifier.clone())));
    }
    let refs: Vec<(planebotcli_types::WorkItem, &str)> = candidates
        .iter()
        .map(|(i, s)| (i.clone(), s.as_str()))
        .collect();
    match_work_item(query, &refs).ok_or_else(|| PlaneError::NotFound {
        message: format!("Work item not found: {query}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_close_names() {
        let items = vec!["Frontend".to_string(), "Backend API".to_string()];
        let m = find_best_match("frontend", &items, |s| s).unwrap();
        assert_eq!(*m.item, "Frontend");
    }

    #[test]
    fn rejects_distant_names() {
        let items = vec!["Alpha".to_string()];
        assert!(find_best_match("zzz nonexistent", &items, |s| s).is_none());
    }

    #[test]
    fn detects_uuids() {
        assert!(is_uuid("5963a34f-1038-43b2-90aa-cc2b183a30e2"));
        assert!(!is_uuid("PLANECLI-9"));
        assert!(!is_uuid("not-a-uuid"));
    }

    fn proj(id: &str, name: &str, identifier: &str) -> Project {
        Project {
            id: id.to_string(),
            name: Some(name.to_string()),
            identifier: Some(identifier.to_string()),
            description: None,
            intake_view: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn project_identifier_wins_over_fuzzy_name() {
        // RENG and SIRENA both fuzzy-match "reng"; the exact identifier must win.
        let projects = vec![
            proj("p-reng", "ReviewEngine", "RENG"),
            proj("p-sirena", "Sirena", "SIRENA"),
        ];
        let m = match_project("RENG", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-reng"));
        let m = match_project("reng", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-reng"));
    }

    #[test]
    fn project_exact_name_beats_fuzzy_name() {
        let projects = vec![
            proj("p-plane", "PlaneCommunity", "PLANE"),
            proj("p-planec", "PlaneCLI", "PLANECLI"),
        ];
        // "PLANE" is p-plane's identifier → exact identifier wins.
        let m = match_project("PLANE", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-plane"));
        // "PlaneCLI" is also p-planec's identifier → identifier beats name.
        let m = match_project("PlaneCLI", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactIdentifier(p) if p.id == "p-planec"));
        // "planecommunity" equals p-plane's name (case-insensitive) → exact name.
        let m = match_project("planecommunity", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::ExactName(p) if p.id == "p-plane"));
    }

    #[test]
    fn project_fuzzy_only_when_no_exact_hit() {
        let projects = vec![
            proj("p-reng", "ReviewEngine", "RENG"),
            proj("p-sirena", "Sirena", "SIRENA"),
        ];
        // No exact identifier/name for "Siren"; falls back to fuzzy → Sirena.
        let m = match_project("Siren", &projects).unwrap();
        assert!(matches!(m, ProjectMatch::Fuzzy(p) if p.id == "p-sirena"));
        // A query that matches nothing exactly and is too distant fuzzy-wise → None.
        assert!(match_project("zzz-nothing", &projects).is_none());
    }

    #[test]
    fn project_empty_query_has_no_match() {
        let projects = vec![proj("p-reng", "ReviewEngine", "RENG")];
        assert!(match_project("", &projects).is_none());
        assert!(match_project("   ", &projects).is_none());
    }
}

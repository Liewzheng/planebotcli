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
    if query.trim().is_empty() {
        return None;
    }
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
            let shown = p
                .name
                .as_deref()
                .or(p.identifier.as_deref())
                .unwrap_or_default();
            eprintln!(
                "warning: no exact project named or identified '{query}'; matched '{shown}' — pass the exact name or identifier to target it precisely",
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
///
/// Two passes, with the first one looking only at exact id and identifier-plus-
/// sequence matches. `sequence_id` is unique within a project, so any exact hit
/// in pass 1 is unambiguous and wins over later passes — this prevents a
/// substring-of-name match on an unrelated item from hijacking a short
/// identifier query (e.g. `PLANE-3` being silently returned when some task's
/// title contains the literal `PLANE-3`). Pass 2 keeps the existing
/// substring-then-fuzzy behaviour so callers relying on partial-name matching
/// see no change.
fn match_work_item(
    query: &str,
    candidates: &[(planebotcli_types::WorkItem, &str)],
) -> Option<LocatedWorkItem> {
    let lower = query.to_lowercase();
    // Pass 1: exact UUID or `PROJ-N` match, scanning the whole list. Returning
    // the first hit is safe because these identifiers are unique per project.
    // A UUID-shaped query cannot match any `PROJ-N` composed sequence, so we
    // skip the per-item sequence-id parse in that case.
    let query_is_uuid = is_uuid(query);
    for (item, identifier) in candidates {
        if item.id.eq_ignore_ascii_case(query) {
            return Some(make_located(item, identifier));
        }
        if !query_is_uuid {
            let seq = item.sequence_id.as_ref().map(|v| match v {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            });
            let composed = seq
                .filter(|s| !s.is_empty() && !identifier.is_empty())
                .map(|s| format!("{identifier}-{s}"));
            if composed
                .as_deref()
                .is_some_and(|c| c.eq_ignore_ascii_case(query))
            {
                return Some(make_located(item, identifier));
            }
        }
    }
    // Pass 2: substring-on-name (preserved from before the fix so existing
    // callers' UX is unchanged) and fuzzy fallback.
    for (item, identifier) in candidates {
        let name = item.name.as_deref().unwrap_or("");
        if !is_uuid(query) && name.to_lowercase().contains(&lower) {
            return Some(make_located(item, identifier));
        }
    }
    let with_names: Vec<(&planebotcli_types::WorkItem, &str, String)> = candidates
        .iter()
        .map(|(item, identifier)| (item, *identifier, item.name.clone().unwrap_or_default()))
        .collect();
    if let Some(m) = find_best_match(query, &with_names, |(_, _, name)| name.as_str()) {
        let (item, identifier, _) = m.item;
        return Some(make_located(item, identifier));
    }
    None
}

fn make_located(item: &planebotcli_types::WorkItem, identifier: &str) -> LocatedWorkItem {
    LocatedWorkItem {
        item: item.clone(),
        project_id: item.project.clone().unwrap_or_default(),
        project_identifier: identifier.to_string(),
    }
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

    fn wi(id: &str, name: &str, seq: u64) -> planebotcli_types::WorkItem {
        planebotcli_types::WorkItem {
            id: id.to_string(),
            name: Some(name.to_string()),
            sequence_id: Some(serde_json::Value::Number(seq.into())),
            description_html: None,
            priority: None,
            state: None,
            labels: None,
            assignees: None,
            parent: None,
            start_date: None,
            target_date: None,
            created_at: None,
            updated_at: None,
            project: Some("p-plane".to_string()),
            project_identifier: Some("PLANE".to_string()),
            ..Default::default()
        }
    }

    fn cands<'a>(items: &[&'a planebotcli_types::WorkItem]) -> Vec<(planebotcli_types::WorkItem, &'a str)> {
        items.iter().map(|w| ((*w).clone(), "PLANE")).collect()
    }

    #[test]
    fn seq_match_wins_over_name_substring_hijack() {
        // PLANE-56's title contains the literal substring "PLANE-3" (from a
        // sentence referencing "PLANE-33"); the old code returned PLANE-56 for
        // query "PLANE-3" whenever the API returned it earlier in the list.
        // The new pass-1 sweep finds PLANE-3 by exact sequence match first.
        let wi3 = wi("i-3", "WebUI 配置 allowed_rate_limit", 3);
        let wi56 = wi(
            "i-56",
            "根因定位：Pages 内容整篇复制膨胀为何复发（PLANE-33 修复后的漏网路径）",
            56,
        );
        // Whichever order the API returns them, the query must land on wi3.
        for order in [&[&wi3, &wi56] as &[&_], &[&wi56, &wi3]] {
            let candidates = cands(order);
            let m = match_work_item("PLANE-3", &candidates).unwrap();
            assert_eq!(m.item.id, "i-3", "PLANE-3 must win regardless of order");
        }
    }

    #[test]
    fn seq_match_wins_for_short_prefix_of_existing_seq() {
        // Querying a short identifier that's a prefix of another item's seq
        // (e.g. PLANE-3 vs PLANE-33) must still resolve to the exact item
        // when present, even when the prefix-larger item appears earlier in
        // the list.
        let wi3 = wi("i-3", "Plain task", 3);
        let wi33 = wi("i-33", "PLANE-3 hijack target", 33);
        for order in [&[&wi33, &wi3] as &[&_], &[&wi3, &wi33]] {
            let candidates = cands(order);
            let m = match_work_item("PLANE-3", &candidates).unwrap();
            assert_eq!(m.item.id, "i-3", "PLANE-3 must win regardless of order");
        }
    }

    #[test]
    fn uuid_match_wins_over_name_substring() {
        // If the user passes a UUID-shaped query that happens to appear as a
        // substring in another item's name, the exact UUID match still wins.
        let wi_uuid = wi("11111111-2222-3333-4444-555555555555", "Plain task", 5);
        let wi_other = wi(
            "i-other",
            "see 11111111-2222-3333-4444-555555555555 in the docs",
            7,
        );
        let candidates = cands(&[&wi_other, &wi_uuid]);
        let m = match_work_item("11111111-2222-3333-4444-555555555555", &candidates).unwrap();
        assert_eq!(m.item.id, "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn substring_falls_through_when_no_exact_match() {
        // No exact id/seq match for "login", but a task name does.
        let wi = wi("i-login", "Login bug fix", 1);
        let candidates = cands(&[&wi]);
        let m = match_work_item("login", &candidates).unwrap();
        assert_eq!(m.item.id, "i-login");
    }

    #[test]
    fn missing_short_id_does_not_hijack_via_name() {
        // PLANE-999 is not in the project, but PLANE-56's title contains the
        // substring "PLANE-9" via "PLANE-99". The old code would return
        // PLANE-56; the new code returns None.
        let wi56 = wi("i-56", "see PLANE-99 plans", 56);
        let candidates = cands(&[&wi56]);
        assert!(match_work_item("PLANE-999", &candidates).is_none());
    }
}

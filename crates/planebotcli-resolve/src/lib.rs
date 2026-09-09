//! Fuzzy resolution of resource references: UUID, identifier, or fuzzy name.
//!
//! Mirrors `src/planecli/utils/fuzzy.py` (rapidfuzz token_sort_ratio, threshold
//! 60) and `src/planecli/utils/resolve.py`.

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
pub fn find_best_match<'a, T, F>(query: &str, items: &'a [T], key: F) -> Option<BestMatch<&'a T>>
where
    F: Fn(&T) -> &str,
{
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

/// Resolve a project by UUID, or fuzzy-match its name.
pub async fn resolve_project(query: &str, client: &PlaneClient) -> Result<Project, PlaneError> {
    if is_uuid(query) {
        return client.get_project(query).await;
    }
    let projects = client.list_projects().await?;
    if let Some(m) = find_best_match(query, &projects, |p| p.name.as_deref().unwrap_or("")) {
        return Ok(m.item.clone());
    }
    Err(PlaneError::NotFound {
        message: format!("Project not found: {query}"),
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
}

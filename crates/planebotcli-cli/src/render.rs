//! Build the display view for work items, mirroring the Python CLI's enriched
//! output: resolved `*_name` fields via cached state/label/member maps, a
//! composed `sequence_id` (`PROJ-123`), `description_stripped`, and `web_url`.

use std::collections::HashMap;

use planebotcli_html::strip_html_tags;
use planebotcli_types::{Comment, WorkItem};
use serde_json::{Value, json};

/// Lookup maps used to resolve UUIDs to human names.
#[derive(Debug, Clone, Default)]
pub struct Lookups {
    /// state uuid -> state name
    pub state_map: HashMap<String, String>,
    /// label uuid -> label name
    pub label_map: HashMap<String, String>,
    /// member uuid -> full name
    pub member_map: HashMap<String, String>,
}

/// Returns (id, optional name) from a value that is either a UUID string or an
/// object carrying `id`/`name` (the API alternates between the two shapes).
fn id_and_name(value: &Value) -> (String, Option<String>) {
    match value {
        Value::String(s) => (s.clone(), None),
        Value::Object(map) => (
            map.get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            map.get("name").and_then(Value::as_str).map(str::to_string),
        ),
        _ => (String::new(), None),
    }
}

fn rows(value: Option<&Value>) -> Vec<(String, Option<String>)> {
    match value {
        Some(Value::Array(items)) => items.iter().map(id_and_name).collect(),
        _ => Vec::new(),
    }
}

/// state/assignee/label field: may be a UUID string or an object with a name.
fn name_or_lookup(
    value: &Option<Value>,
    map: &HashMap<String, String>,
    truncate_uuid: bool,
) -> String {
    match value {
        Some(Value::Object(o)) => o
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        Some(Value::String(uuid)) => match map.get(uuid) {
            Some(name) => name.clone(),
            None if truncate_uuid => uuid.chars().take(8).collect(),
            None => uuid.clone(),
        },
        _ => String::new(),
    }
}

/// Extract an estimate display value from the polymorphic estimate_point field.
fn estimate_display(value: &Option<Value>) -> String {
    match value {
        Some(Value::Object(o)) => o
            .get("value")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                _ => String::new(),
            })
            .unwrap_or_default(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => String::new(),
    }
}

/// Build the JSON view of a work item, matching the Python enriched row.
pub fn work_item_view(
    item: &WorkItem,
    project_identifier: &str,
    lookups: &Lookups,
    base_url: &str,
    workspace: &str,
) -> Value {
    let identifier = if !project_identifier.is_empty() {
        project_identifier.to_string()
    } else {
        item.project_detail
            .as_ref()
            .and_then(|d| d.identifier.clone())
            .unwrap_or_default()
    };
    let seq = item
        .sequence_id
        .as_ref()
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    let sequence_id = if identifier.is_empty() || seq.is_empty() {
        String::new()
    } else {
        format!("{identifier}-{seq}")
    };

    let priority = match item.priority.as_deref() {
        Some(p) if !p.is_empty() && p != "none" => p.to_string(),
        _ => String::new(),
    };

    let label_detail_names = rows(item.labels.as_ref())
        .iter()
        .map(|(id, name)| match name {
            Some(n) => n.clone(),
            None => lookups
                .label_map
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.clone()),
        })
        .collect::<Vec<_>>();
    let label_names = label_detail_names.join(", ");

    let project_id = item.project.clone().unwrap_or_default();
    let web_url = if !project_id.is_empty() && !item.id.is_empty() && !workspace.is_empty() {
        format!(
            "{}/projects/{}/issues/{}/",
            web_base(base_url, workspace),
            project_id,
            item.id
        )
    } else {
        String::new()
    };

    let assignee_names: Vec<String> = rows(item.assignees.as_ref())
        .iter()
        .map(|(id, name)| match name {
            Some(n) => n.clone(),
            None => lookups
                .member_map
                .get(id)
                .cloned()
                .unwrap_or_else(|| id.chars().take(8).collect()),
        })
        .collect();
    let assignee_names = assignee_names.join(", ");

    json!({
        "id": item.id,
        "sequence_id": sequence_id,
        "name": item.name.clone().unwrap_or_default(),
        "description_html": item.description_html.clone().unwrap_or_default(),
        "description_stripped": strip_html_tags(item.description_html.as_deref().unwrap_or("")).trim(),
        "priority": priority,
        "state": raw_value(item.state.as_ref()),
        "state_detail_name": name_or_lookup(&item.state, &lookups.state_map, false),
        "assignee_names": assignee_names,
        "label_names": label_names,
        "label_detail_names": label_detail_names,
        "estimate_display": estimate_display(&item.estimate_point.clone().or_else(|| item.point.clone())),
        "parent": item.parent.clone().unwrap_or_default(),
        "project": project_id,
        "project_identifier": identifier,
        "start_date": item.start_date.clone().unwrap_or_default(),
        "target_date": item.target_date.clone().unwrap_or_default(),
        "created_at": item.created_at.clone().unwrap_or_default(),
        "updated_at": item.updated_at.clone().unwrap_or_default(),
        "web_url": web_url,
    })
}

/// `{base_url}/{workspace}` with the base URL's trailing slash stripped.
fn web_base(base_url: &str, workspace: &str) -> String {
    format!("{}/{}", base_url.trim_end_matches('/'), workspace)
}

/// Keep a polymorphic state value (UUID string or object) as-is for JSON.
fn raw_value(value: Option<&Value>) -> Value {
    value.cloned().unwrap_or(Value::Null)
}

/// JSON view of a comment, mirroring the Python `wi show` comment entries.
pub fn comment_json(comment: &Comment, member_map: &HashMap<String, String>) -> Value {
    let actor = comment.actor.clone().unwrap_or_default();
    let actor_name = member_map
        .get(&actor)
        .cloned()
        .unwrap_or_else(|| actor.chars().take(8).collect());
    json!({
        "id": comment.id,
        "created_at": comment.created_at.clone().unwrap_or_default(),
        "updated_at": comment.updated_at.clone().unwrap_or_default(),
        "comment_html": comment.comment_html.clone().unwrap_or_default(),
        "comment_stripped": comment.comment_stripped.clone().unwrap_or_default(),
        "actor": actor,
        "actor_name": actor_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use planebotcli_types::WorkItem;

    fn member(name: &str) -> planebotcli_types::Member {
        planebotcli_types::Member {
            id: "u".into(),
            display_name: Some(name.into()),
            ..Default::default()
        }
    }

    #[test]
    fn member_full_name_prefers_joined_first_last() {
        let m = planebotcli_types::Member {
            id: "u1".into(),
            first_name: Some("Jicai".into()),
            last_name: Some("Liu".into()),
            display_name: Some("Other".into()),
            ..Default::default()
        };
        assert_eq!(m.full_name(), "Jicai Liu");
        assert_eq!(member("Kimicode").full_name(), "Kimicode");
    }

    #[test]
    fn view_resolves_names_and_composes_sequence_id() {
        let mut lookups = Lookups::default();
        lookups.state_map.insert("s1".into(), "In Progress".into());
        lookups.label_map.insert("l1".into(), "feat".into());
        lookups.member_map.insert("m1".into(), "Jicai Liu".into());

        let item = WorkItem {
            id: "i1".into(),
            name: Some("Task".into()),
            sequence_id: Some(Value::from(30)),
            description_html: Some("<p>Hello</p>".into()),
            state: Some(Value::String("s1".into())),
            labels: Some(Value::Array(vec![Value::String("l1".into())])),
            assignees: Some(Value::Array(vec![Value::String("m1".into())])),
            project: Some("p1".into()),
            ..Default::default()
        };

        let view = work_item_view(&item, "PLANECLI", &lookups, "http://x/", "ws");
        assert_eq!(view["sequence_id"], "PLANECLI-30");
        assert_eq!(view["state_detail_name"], "In Progress");
        assert_eq!(view["label_names"], "feat");
        assert_eq!(view["assignee_names"], "Jicai Liu");
        assert_eq!(view["description_stripped"], "Hello");
        assert_eq!(view["web_url"], "http://x/ws/projects/p1/issues/i1/");
    }

    #[test]
    fn view_uses_object_names_when_expanded() {
        let item = WorkItem {
            id: "i1".into(),
            state: Some(Value::Object(serde_json::Map::from_iter([
                ("name".into(), Value::String("Todo".into())),
                ("id".into(), Value::String("s9".into())),
            ]))),
            ..Default::default()
        };
        let view = work_item_view(&item, "", &Lookups::default(), "http://x", "ws");
        assert_eq!(view["state_detail_name"], "Todo");
    }

    #[test]
    fn view_clears_unset_priority() {
        let item = WorkItem {
            id: "i".into(),
            priority: Some("none".into()),
            ..Default::default()
        };
        assert_eq!(
            work_item_view(&item, "", &Lookups::default(), "http://x", "ws")["priority"],
            ""
        );
        let item2 = WorkItem {
            id: "i".into(),
            priority: Some("urgent".into()),
            ..Default::default()
        };
        assert_eq!(
            work_item_view(&item2, "", &Lookups::default(), "http://x", "ws")["priority"],
            "urgent"
        );
    }
}

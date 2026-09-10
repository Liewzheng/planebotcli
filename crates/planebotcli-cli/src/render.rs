//! Build the display view for work items, mirroring the Python CLI's enriched
//! output: resolved `*_name` fields via cached state/label/member maps, a
//! composed `sequence_id` (`PROJ-123`), `description_stripped`, and `web_url`.

use std::collections::HashMap;

use planebotcli_html::strip_html_tags;
use planebotcli_types::{Attachment, Comment, IntakeItem, WorkItem};
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

/// `PROJ-123` composed from a raw `sequence_id` value (`Number` or `String`)
/// and the project identifier; empty when either half is missing.
pub fn compose_sequence_id(sequence_id: Option<&Value>, project_identifier: &str) -> String {
    let seq = sequence_id
        .map(|v| match v {
            Value::Number(n) => n.to_string(),
            Value::String(s) => s.clone(),
            _ => String::new(),
        })
        .unwrap_or_default();
    if project_identifier.is_empty() || seq.is_empty() {
        String::new()
    } else {
        format!("{project_identifier}-{seq}")
    }
}

/// The `sub_issues` entries of a work item view: the project's work items
/// whose `parent` is `parent_id`, each as `{id, sequence_id, name,
/// state_detail_name}`.
pub fn sub_issue_summaries(
    items: &[WorkItem],
    parent_id: &str,
    project_identifier: &str,
    lookups: &Lookups,
) -> Vec<Value> {
    items
        .iter()
        .filter(|item| item.parent.as_deref() == Some(parent_id))
        .map(|item| {
            json!({
                "id": item.id,
                "sequence_id": compose_sequence_id(
                    item.sequence_id.as_ref(),
                    project_identifier,
                ),
                "name": item.name.clone().unwrap_or_default(),
                "state_detail_name": name_or_lookup(&item.state, &lookups.state_map, false),
            })
        })
        .collect()
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
    let sequence_id = compose_sequence_id(item.sequence_id.as_ref(), &identifier);
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

/// Intake status codes → labels (mirrors the Python `INTAKE_STATUS_LABELS`).
/// Unknown codes render as their raw number; a missing status as an empty
/// string.
pub fn intake_status_label(status: Option<i64>) -> String {
    match status {
        None => String::new(),
        Some(-2) => "pending".to_string(),
        Some(-1) => "rejected".to_string(),
        Some(0) => "snoozed".to_string(),
        Some(1) => "accepted".to_string(),
        Some(2) => "duplicate".to_string(),
        Some(other) => other.to_string(),
    }
}

/// Enriched JSON view of an intake row, mirroring the Python `_enrich_intake`:
/// `name`/`priority` are flattened from the nested `issue_detail`, `issue_id`
/// is the work-item UUID (`issue`, falling back to the nested detail id), and
/// the numeric `status` becomes its label.
pub fn intake_view(item: &IntakeItem) -> Value {
    let detail = item.issue_detail.clone().unwrap_or(Value::Null);
    let pick = |key: &str| -> String {
        match detail.get(key) {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => String::new(),
        }
    };
    let name = pick("name");
    let mut priority = pick("priority");
    if priority.is_empty() {
        priority = "none".to_string();
    }
    let issue_id = match item.issue.as_deref() {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => pick("id"),
    };
    json!({
        "id": item.id,
        "issue": item.issue.clone().unwrap_or_default(),
        "issue_detail": detail,
        "project": item.project.clone().unwrap_or_default(),
        "workspace": item.workspace.clone().unwrap_or_default(),
        "status": intake_status_label(item.status),
        "created_at": item.created_at.clone().unwrap_or_default(),
        "updated_at": item.updated_at.clone().unwrap_or_default(),
        "name": name,
        "priority": priority,
        "issue_id": issue_id,
    })
}

/// Enriched JSON view of an attachment, mirroring the Python `_enrich_attachment`:
/// `name`/`type`/`size` are flattened from the nested `attributes` object so
/// table and JSON consumers see one consistent shape. The top-level `size`
/// wins when both are present; the raw `attributes` stay in the output.
pub fn attachment_json(attachment: &Attachment) -> Value {
    let attributes = attachment.attributes.clone().unwrap_or(Value::Null);
    json!({
        "id": attachment.id,
        "asset_id": attachment.asset_id.clone().unwrap_or_default(),
        "attributes": attributes,
        "name": attachment.name(),
        "type": attachment.content_type(),
        "size": attachment.size_bytes().unwrap_or(Value::Null),
        "is_uploaded": attachment.is_uploaded.unwrap_or(false),
        "is_deleted": attachment.is_deleted.unwrap_or(false),
        "created_at": attachment.created_at.clone().unwrap_or_default(),
        "updated_at": attachment.updated_at.clone().unwrap_or_default(),
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

    #[test]
    fn attachment_view_flattens_attributes_and_keeps_them() {
        let att = Attachment {
            id: "a1".into(),
            attributes: Some(json!({"name": "spec.pdf", "type": "application/pdf", "size": 123})),
            is_uploaded: Some(true),
            created_at: Some("2026-09-08T04:54:06.446Z".into()),
            ..Default::default()
        };
        let view = attachment_json(&att);
        assert_eq!(view["name"], "spec.pdf");
        assert_eq!(view["type"], "application/pdf");
        assert_eq!(view["size"].as_u64(), Some(123));
        assert_eq!(view["is_uploaded"], true);
        assert_eq!(view["is_deleted"], false);
        assert_eq!(view["attributes"]["name"], "spec.pdf");
        assert_eq!(view["created_at"], "2026-09-08T04:54:06.446Z");
    }

    #[test]
    fn attachment_view_top_level_size_wins_and_missing_attrs_are_empty() {
        // Top-level `size` beats `attributes.size` (Python flatten semantics).
        let att = Attachment {
            id: "a2".into(),
            asset_id: Some("a2".into()),
            attributes: Some(json!({"size": 999})),
            size: Some(json!(42)),
            ..Default::default()
        };
        let view = attachment_json(&att);
        assert_eq!(view["size"].as_u64(), Some(42));
        assert_eq!(view["asset_id"], "a2");

        let bare = Attachment {
            id: "a3".into(),
            ..Default::default()
        };
        let view = attachment_json(&bare);
        assert_eq!(view["name"], "");
        assert_eq!(view["type"], "");
        assert_eq!(view["size"], Value::Null);
        assert_eq!(view["is_uploaded"], false);
    }

    #[test]
    fn sequence_id_composes_number_and_string_forms() {
        assert_eq!(
            compose_sequence_id(Some(&Value::from(30)), "PLANECLI"),
            "PLANECLI-30"
        );
        assert_eq!(
            compose_sequence_id(Some(&Value::String("30".into())), "PLANECLI"),
            "PLANECLI-30"
        );
        // Either half missing yields nothing rather than a partial label.
        assert_eq!(compose_sequence_id(Some(&Value::from(30)), ""), "");
        assert_eq!(compose_sequence_id(None, "PLANECLI"), "");
    }

    #[test]
    fn sub_issue_summaries_select_only_children() {
        let mut lookups = Lookups::default();
        lookups.state_map.insert("s1".into(), "In Progress".into());
        let items = vec![
            WorkItem {
                id: "child-1".into(),
                name: Some("Child".into()),
                sequence_id: Some(Value::from(7)),
                parent: Some("parent-1".into()),
                state: Some(Value::String("s1".into())),
                ..Default::default()
            },
            WorkItem {
                id: "other".into(),
                name: Some("Unrelated".into()),
                parent: Some("parent-2".into()),
                ..Default::default()
            },
            WorkItem {
                id: "top".into(),
                name: Some("Top level".into()),
                ..Default::default()
            },
        ];
        let summaries = sub_issue_summaries(&items, "parent-1", "PLANECLI", &lookups);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0]["id"], "child-1");
        assert_eq!(summaries[0]["sequence_id"], "PLANECLI-7");
        assert_eq!(summaries[0]["name"], "Child");
        assert_eq!(summaries[0]["state_detail_name"], "In Progress");
        // No children → empty array, never null.
        assert!(sub_issue_summaries(&items, "nobody", "PLANECLI", &lookups).is_empty());
    }
}

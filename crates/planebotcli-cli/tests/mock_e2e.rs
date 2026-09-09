//! End-to-end command-layer tests: run the real `planebotcli` binary against a
//! mockito mock server (no real instance). Pins the `--json` output contract
//! (sequence ids, resolved names, web_url) and exit codes.
//!
//! The binary is located via cargo's `CARGO_BIN_EXE_planebotcli`. Proxies are
//! stripped from the child env so local mockito traffic is not hijacked.

use std::process::Command;

use mockito::Server;

const BIN: &str = env!("CARGO_BIN_EXE_planebotcli");

fn run(args: &[&str], base_url: &str) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env("PLANE_BASE_URL", base_url)
        .env("PLANE_API_KEY", "test-key")
        .env("PLANE_WORKSPACE", "ws")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .output()
        .expect("failed to spawn planebotcli")
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

const MEMBERS: &str =
    r#"[{"id":"u1","display_name":"Bot","first_name":"B","last_name":"","email":"b@x"}]"#;
const PROJECTS: &str = r#"{"results":[{"id":"p1","name":"Demo","identifier":"DEMO","created_at":"2026-01-01T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;
const STATES: &str = r##"{"results":[{"id":"s1","name":"Todo","group":"unstarted","color":"#fff","sequence":1}],"next_cursor":null,"next_page_results":false}"##;
const LABELS: &str = r##"{"results":[{"id":"l1","name":"feat","color":"#3B82F6"}],"next_cursor":null,"next_page_results":false}"##;
const ITEM: &str = r#"{"id":"wi1","name":"A task","sequence_id":1,"description_html":"<p>hi</p>","priority":"high","state":"s1","labels":["l1"],"assignees":["u1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;
const ITEMS: &str = r#"{"results":[{"id":"wi1","name":"A task","sequence_id":1,"description_html":"<p>hi</p>","priority":"high","state":"s1","labels":["l1"],"assignees":["u1"],"project":"p1","created_at":"2026-01-02T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}],"next_cursor":null,"next_page_results":false}"#;

#[test]
fn wi_ls_json_contract() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/v1/workspaces/ws/members/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(MEMBERS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    for (path, body) in [
        ("/api/v1/workspaces/ws/projects/p1/states/", STATES),
        ("/api/v1/workspaces/ws/projects/p1/labels/", LABELS),
        ("/api/v1/workspaces/ws/projects/p1/work-items/", ITEMS),
    ] {
        server
            .mock("GET", path)
            .match_query(mockito::Matcher::Any)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
    }

    let out = run(
        &["wi", "ls", "-p", "Demo", "--json", "--no-cache"],
        &server.url(),
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout_of(&out)).expect("valid JSON");
    let row = &parsed[0];
    assert_eq!(row["sequence_id"], "DEMO-1");
    assert_eq!(row["name"], "A task");
    assert_eq!(row["state_detail_name"], "Todo");
    assert_eq!(row["label_names"], "feat");
    assert_eq!(row["assignee_names"], "B"); // member full_name = first+last
    assert!(
        row["web_url"]
            .as_str()
            .unwrap_or("")
            .contains("/ws/projects/p1/issues/wi1/")
    );
}

#[test]
fn invalid_state_exits_5_with_available_list() {
    let mut server = Server::new();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(PROJECTS)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/states/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(STATES)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id":"p1","name":"Demo","identifier":"DEMO"}"#)
        .create();
    server
        .mock("GET", "/api/v1/workspaces/ws/projects/p1/labels/")
        .match_query(mockito::Matcher::Any)
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(LABELS)
        .create();
    // create_work_item must not be reached
    server
        .mock("POST", "/api/v1/workspaces/ws/projects/p1/work-items/")
        .with_status(201)
        .with_body(ITEM)
        .expect(0)
        .create();

    let out = run(
        &[
            "wi",
            "create",
            "x",
            "-p",
            "Demo",
            "--state",
            "NoSuch",
            "--json",
            "--no-cache",
        ],
        &server.url(),
    );
    assert_eq!(out.status.code(), Some(5));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("State 'NoSuch' not found"), "{err}");
    assert!(err.contains("Available: Todo"), "{err}");
}

use std::{path::Path, process::Command};

#[test]
fn agentenv_cli_can_search_install_list_info_and_verify_from_hub_http_registry() {
    let agentenv_bin = match std::env::var("AGENTENV_BIN") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping compatibility test: AGENTENV_BIN is not set");
            return;
        }
    };
    let hub_url =
        std::env::var("HUB_E2E_URL").unwrap_or_else(|_| "http://127.0.0.1:7777".to_owned());
    let home = tempfile::tempdir().unwrap();

    let search = run_agentenv(
        &agentenv_bin,
        home.path(),
        &[
            "skills",
            "search",
            "review",
            "--registry",
            &hub_url,
            "--json",
        ],
    );
    assert_success(&search);
    let search_json: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(search_json["skills"][0]["name"], "code-review");

    let add = run_agentenv(
        &agentenv_bin,
        home.path(),
        &[
            "skills",
            "add",
            "code-review@1.2.0",
            "--registry",
            &hub_url,
            "--allow-unsigned",
            "--json",
        ],
    );
    assert_success(&add);
    let add_json: serde_json::Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(add_json["name"], "code-review");
    assert_eq!(add_json["version"], "1.2.0");

    let list = run_agentenv(&agentenv_bin, home.path(), &["skills", "list", "--json"]);
    assert_success(&list);
    let list_json: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(list_json["skills"][0]["name"], "code-review");

    let info = run_agentenv(
        &agentenv_bin,
        home.path(),
        &["skills", "info", "code-review", "--json"],
    );
    assert_success(&info);
    let info_json: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    assert_eq!(info_json["name"], "code-review");

    let verify = run_agentenv(
        &agentenv_bin,
        home.path(),
        &["skills", "verify", "code-review"],
    );
    assert_success(&verify);
    assert!(
        String::from_utf8_lossy(&verify.stdout).contains("code-review"),
        "stdout: {}",
        String::from_utf8_lossy(&verify.stdout)
    );
}

fn run_agentenv(bin: &str, home: &Path, args: &[&str]) -> std::process::Output {
    Command::new(bin)
        .args(args)
        .env("HOME", home)
        .env("AGENTENV_SKILLS_ALLOW_LOOPBACK_REGISTRIES", "1")
        .output()
        .unwrap()
}

fn assert_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

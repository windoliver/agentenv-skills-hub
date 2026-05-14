use std::process::Command;

#[test]
fn agentenv_can_install_from_hub_http_registry() {
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

    let output = Command::new(agentenv_bin)
        .arg("skills")
        .arg("add")
        .arg("code-review@1.2.0")
        .arg("--registry")
        .arg(hub_url)
        .arg("--allow-unsigned")
        .arg("--json")
        .env("HOME", home.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

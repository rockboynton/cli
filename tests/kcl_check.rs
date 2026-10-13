//! Real-process tests for the token-free offline checker.

use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn run(project: &Path, arguments: &[&str], stdin: Option<&str>) -> Output {
    let config = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_zoo"))
        .args(["kcl", "check"])
        .args(arguments)
        .current_dir(project)
        .env("ZOO_CONFIG_DIR", config.path())
        .env_remove("ZOO_API_TOKEN")
        .env_remove("KITTYCAD_API_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("ZOO_NO_UPDATE_NOTIFIER")
        .env("ZOO_HOST", "http://127.0.0.1:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{error}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

#[test]
fn clean_directory_and_default_input_work_without_authentication() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("main.kcl"),
        "@settings(kclVersion = 3.0)\nanswer = 7\n",
    )
    .unwrap();
    for args in [vec!["--json"], vec![".", "--json"]] {
        let output = run(project.path(), &args, None);
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let report = json(&output);
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["ok"], true);
        assert_eq!(report["diagnostics"], serde_json::json!([]));
    }
}

#[test]
fn warnings_can_be_denied_and_have_locations() {
    let project = tempfile::tempdir().unwrap();
    // Deliberately test the deprecated API under KCL 2.
    std::fs::write(
        project.path().join("main.kcl"),
        "@settings(kclVersion = 2.0)\np = startSketchOn(XY)\n",
    )
    .unwrap();
    let output = run(project.path(), &["--json"], None);
    assert!(output.status.success());
    let report = json(&output);
    let warning = &report["diagnostics"][0];
    assert_eq!(warning["severity"], "warning");
    assert_eq!(warning["code"], "deprecated");
    assert_eq!(warning["locations"][0]["start"]["line"], 2);
    assert_eq!(warning["locations"][0]["start"]["column"], 5);
    let denied = run(project.path(), &["--json", "--deny-warnings"], None);
    assert!(!denied.status.success());
    assert_eq!(json(&denied)["ok"], false);
    assert_eq!(json(&denied)["diagnostics"], report["diagnostics"]);
}

#[test]
fn imported_type_errors_preserve_the_imported_file_location() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("sub")).unwrap();
    std::fs::write(
        project.path().join("main.kcl"),
        "@settings(kclVersion = 3.0)\nimport shape from \"sub/main.kcl\"\n",
    )
    .unwrap();
    std::fs::write(project.path().join("sub/main.kcl"), "@settings(kclVersion = 3.0)\nexport shape = sketch(on = XY) {\n  bottom = line(start = [0, 0], end = [4, 0])\n  right = line(start = bottom.end, end = [4, 3])\n}\n").unwrap();
    let output = run(project.path(), &["--json"], None);
    assert!(!output.status.success());
    let report = json(&output);
    let error = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["message"].as_str().unwrap().contains("Point2d"))
        .unwrap();
    assert_eq!(error["severity"], "error");
    assert!(error["message"].as_str().unwrap().contains("Segment"));
    assert!(error["locations"].as_array().unwrap().iter().any(|location| {
        Path::new(location["file"].as_str().unwrap()).ends_with(Path::new("sub").join("main.kcl"))
            && location["start"]["line"] == 4
    }));
}

#[test]
fn syntax_and_input_errors_have_json_and_failing_statuses() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("main.kcl"), "broken = (\n").unwrap();
    for args in [vec!["--json"], vec!["missing.kcl", "--json"]] {
        let output = run(project.path(), &args, None);
        assert!(!output.status.success());
        assert_eq!(json(&output)["ok"], false);
        assert_eq!(json(&output)["diagnostics"][0]["severity"], "error");
    }
}

#[test]
fn invalid_parent_settings_fail_for_nested_entry_points() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join("sub")).unwrap();
    std::fs::write(
        project.path().join("sub/main.kcl"),
        "@settings(kclVersion = 3.0)\nanswer = 7\n",
    )
    .unwrap();
    std::fs::write(project.path().join("project.toml"), "[invalid toml").unwrap();
    let output = run(project.path(), &["sub", "--json"], None);
    assert!(!output.status.success());
    assert!(
        json(&output)["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("project.toml")
    );
}

#[test]
fn stdin_checks_resolve_imports_from_the_working_directory() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join("value.kcl"),
        "@settings(kclVersion = 3.0)\nexport answer = 7\n",
    )
    .unwrap();
    let output = run(
        project.path(),
        &["-", "--json"],
        Some("@settings(kclVersion = 3.0)\nimport answer from \"value.kcl\"\n"),
    );
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let error = run(project.path(), &["-", "--json"], Some("broken = (\n"));
    assert!(!error.status.success());
    assert_eq!(json(&error)["diagnostics"][0]["locations"][0]["file"], "-");
}

#[test]
fn human_output_reports_the_error_at_its_location() {
    let project = tempfile::tempdir().unwrap();
    let output = run(project.path(), &["-"], Some("broken = (\n"));
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("-:1:11: error:"), "{stderr}");
    assert!(stderr.contains("Unexpected end of file"), "{stderr}");
}

#[test]
fn unicode_positions_use_byte_offsets_and_character_columns() {
    let project = tempfile::tempdir().unwrap();
    let source = "@settings(kclVersion = 3.0)\r\np = \"caf\u{e9}\" + 1\r\n";
    let output = run(project.path(), &["-", "--json"], Some(source));
    assert!(!output.status.success());
    let report = json(&output);
    let location = &report["diagnostics"][0]["locations"][0];
    assert_eq!(location["start"]["byte"], source.find('\"').unwrap());
    assert_eq!(location["end"]["byte"], source.find(" + 1").unwrap());
    assert_eq!(location["start"]["line"], 2);
    assert_eq!(location["start"]["column"], 5);
    assert_eq!(location["end"]["column"], 11);
}

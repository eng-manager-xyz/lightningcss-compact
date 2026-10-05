#![cfg(feature = "cli")]

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lightningcss-compact"))
        .args(args)
        .output()
        .expect("CLI executes")
}
fn path(path: &Path) -> &str {
    path.to_str().expect("temporary path is UTF-8")
}
fn fixture(file: &Path) {
    fs::write(file, include_str!("../examples/project.json")).unwrap();
}

#[test]
fn generated_output_is_repeatable_and_preserves_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("project.json");
    fixture(&input);
    let original = fs::read(&input).unwrap();
    let output = directory.path().join("generated");
    let args = [
        "build",
        "--project",
        path(&input),
        "--out-dir",
        path(&output),
    ];
    let first = run(&args);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let result = fs::read(output.join("result.json")).unwrap();
    assert!(output.join("css/cards.css.map").is_file());
    assert!(run(&args).status.success());
    assert_eq!(result, fs::read(output.join("result.json")).unwrap());
    assert_eq!(original, fs::read(&input).unwrap());
}

#[test]
fn independent_processes_preserve_rules_after_adjacent_merge_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("project.json");
    let value = serde_json::json!({
        "stylesheets": [{"id": "fixture.css", "source": ".left{color:red;height:100px}.right{color:red}.right{height:100px}.tail{color:blue}@--lightningcss-compact-internal-boundary;"}],
        "bindings": [{"id": "document", "kind": "html", "value": "<div class=\"left\">left</div><div class=\"right\">right</div><div class=\"tail\">tail</div>"}],
        "complete_usage": true,
        "managed_classes": ["left", "right", "tail"],
        "load_groups": [["fixture.css"]]
    });
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    for mode in ["baseline", "naming", "compact"] {
        let output = directory.path().join(mode);
        let args = [
            "build",
            "--mode",
            mode,
            "--project",
            path(&input),
            "--out-dir",
            path(&output),
        ];
        assert!(run(&args).status.success());
        let first = fs::read(output.join("result.json")).unwrap();
        let result: serde_json::Value = serde_json::from_slice(&first).unwrap();
        let css = result["stylesheets"]["fixture.css"].as_str().unwrap();
        assert!(css.contains("color:#00f"), "later rule was lost: {css}");
        assert_eq!(
            css.matches("@--lightningcss-compact-internal-boundary;")
                .count(),
            1,
            "authored unknown at-rule must survive; temporary barriers must not escape"
        );
        for _ in 0..16 {
            assert!(run(&args).status.success());
            assert_eq!(
                first,
                fs::read(output.join("result.json")).unwrap(),
                "independent {mode} process changed output"
            );
        }
    }
}

#[test]
fn eval_refuses_to_overwrite_its_project_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("comparison.json");
    fixture(&input);
    let original = fs::read(&input).unwrap();
    let result = run(&[
        "eval",
        "--project",
        path(&input),
        "--out-dir",
        path(directory.path()),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("overwrite"));
    assert_eq!(original, fs::read(&input).unwrap());
}

#[test]
fn traversal_identity_cannot_escape_output_directory() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("project.json");
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../examples/project.json")).unwrap();
    value["stylesheets"][0]["id"] = "../../escape.css".into();
    value["load_groups"][0][0] = "../../escape.css".into();
    fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
    let output = directory.path().join("generated");
    let result = run(&[
        "build",
        "--project",
        path(&input),
        "--out-dir",
        path(&output),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("unsafe stylesheet"));
    assert!(!directory.path().join("escape.css").exists());
    assert!(!output.join("result.json").exists());
}

#[test]
fn case_and_unicode_equivalent_output_identities_are_rejected_portably() {
    for (first, second) in [
        ("Cards.css", "cards.css"),
        ("caf\u{e9}.css", "cafe\u{301}.css"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("project.json");
        let mut value: serde_json::Value =
            serde_json::from_str(include_str!("../examples/project.json")).unwrap();
        value["stylesheets"][0]["id"] = first.into();
        value["stylesheets"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":second,"source":"body{margin:0}"}));
        value["load_groups"] = serde_json::json!([[first, second]]);
        fs::write(&input, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = directory.path().join("generated");
        let result = run(&[
            "build",
            "--project",
            path(&input),
            "--out-dir",
            path(&output),
        ]);
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("collide"));
        assert!(!output.join("result.json").exists());
    }
}

#[test]
fn redundant_path_segments_are_rejected_before_any_artifact_is_written() {
    for alias in ["a//b.css", "a/./b.css"] {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("project.json");
        let value = serde_json::json!({
            "stylesheets": [
                {"id": "a/b.css", "source": ".first{color:red}"},
                {"id": alias, "source": ".second{color:blue}"}
            ],
            "load_groups": [["a/b.css", alias]]
        });
        let original = serde_json::to_vec(&value).unwrap();
        fs::write(&input, &original).unwrap();
        let output = directory.path().join("generated");
        let result = run(&[
            "build",
            "--project",
            path(&input),
            "--out-dir",
            path(&output),
        ]);
        assert!(!result.status.success(), "accepted output alias {alias}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("unsafe stylesheet"));
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    }
}

#[test]
fn planned_file_directory_conflicts_are_rejected_before_any_artifact_is_written() {
    for (first, second) in [
        ("a.css", "a.css/b.css"),
        ("A.css", "a.css/b.css"),
        ("caf\u{e9}.css", "cafe\u{301}.css/b.css"),
        ("a.css", "a.css.map/b.css"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("project.json");
        let value = serde_json::json!({
            "stylesheets": [
                {"id": first, "source": ".first{color:red}"},
                {"id": second, "source": ".second{color:blue}"}
            ],
            "load_groups": [[first, second]]
        });
        let original = serde_json::to_vec(&value).unwrap();
        fs::write(&input, &original).unwrap();
        let output = directory.path().join("generated");
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.txt"), "existing output").unwrap();
        let result = run(&[
            "build",
            "--project",
            path(&input),
            "--out-dir",
            path(&output),
        ]);
        assert!(
            !result.status.success(),
            "accepted file/directory conflict {first}, {second}"
        );
        assert!(String::from_utf8_lossy(&result.stderr).contains("planned file"));
        assert_eq!(fs::read(&input).unwrap(), original);
        assert_eq!(fs::read_dir(&output).unwrap().count(), 1);
        assert_eq!(
            fs::read_to_string(output.join("keep.txt")).unwrap(),
            "existing output"
        );
    }
}

#[test]
fn a_hard_link_cannot_alias_output_to_the_project_input() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("project.json");
    fixture(&input);
    let output = directory.path().join("generated");
    fs::create_dir(&output).unwrap();
    fs::hard_link(&input, output.join("result.json")).unwrap();
    let original = fs::read(&input).unwrap();
    let result = run(&[
        "build",
        "--project",
        path(&input),
        "--out-dir",
        path(&output),
    ]);
    assert!(!result.status.success());
    assert_eq!(original, fs::read(&input).unwrap());
    assert!(!output.join("manifest.json").exists());
}

#[cfg(unix)]
#[test]
fn symlink_destinations_are_rejected_before_artifacts_are_written() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("project.json");
    fixture(&input);
    let output = directory.path().join("generated");
    let external = directory.path().join("authored");
    fs::create_dir(&output).unwrap();
    fs::create_dir(&external).unwrap();
    std::os::unix::fs::symlink(&external, output.join("css")).unwrap();
    let result = run(&[
        "build",
        "--project",
        path(&input),
        "--out-dir",
        path(&output),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("symlink"));
    assert!(!external.join("cards.css").exists());
    assert!(!output.join("manifest.json").exists());
}

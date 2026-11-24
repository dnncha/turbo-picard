#[cfg(test)]
mod mcp_session {
    use assert_cmd::Command;
    use serde_json::{Value, json};
    use std::path::Path;

    fn session_with_env(messages: &[Value], cwd: &Path, extra_env: &[(&str, &str)]) -> Vec<Value> {
        let input = messages
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let mut command = Command::cargo_bin("turbo-picard").expect("binary exists");
        command
            .arg("mcp")
            .current_dir(cwd)
            .env_remove("TURBO_PICARD_REQUIRE_NATIVE")
            .write_stdin(format!("{input}\n"));
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let output = command.output().expect("mcp server runs");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf-8 output")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each line is JSON-RPC"))
            .collect()
    }

    fn session(messages: &[Value], cwd: &Path) -> Vec<Value> {
        session_with_env(messages, cwd, &[])
    }

    fn call(id: u64, name: &str, arguments: &Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
    }

    #[test]
    fn mcp_session_lists_tools_explains_and_runs_a_guarded_command() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/markduplicates/basic/input.sam");
        std::fs::copy(&fixture, dir.path().join("input.sam")).expect("fixture copied");

        let responses = session(
            &[
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}),
                json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
                json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
                call(
                    3,
                    "explain",
                    &json!({"command": "SortSam", "args": ["I=input.sam", "O=sorted.sam"]}),
                ),
                call(
                    4,
                    "run",
                    &json!({"command": "SortSam", "args": ["I=input.sam", "O=sorted.sam", "SORT_ORDER=queryname"]}),
                ),
                call(
                    5,
                    "run",
                    &json!({"command": "SortSam", "args": ["I=input.sam", "O=sorted.sam", "SORT_ORDER=queryname"]}),
                ),
                call(6, "doctor", &json!({})),
                call(7, "capabilities", &json!({"command": "MarkDuplicates"})),
                call(
                    8,
                    "run",
                    &json!({
                        "command": "SortSam",
                        "args": ["-I", "input.sam", "--output", "sorted_gatk.sam", "--sort-order", "queryname"]
                    }),
                ),
            ],
            dir.path(),
        );

        assert_eq!(responses.len(), 8, "the notification gets no response");
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
        let tools = responses[1]["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 6);

        let explain = &responses[2]["result"];
        assert_eq!(explain["isError"], false);
        assert_eq!(explain["structuredContent"]["command"], "SortSam");
        assert!(explain["structuredContent"]["fallback_ready"].is_boolean());

        let run = &responses[3]["result"];
        assert_eq!(run["isError"], false, "{run}");
        assert_eq!(run["structuredContent"]["exit_code"], 0);
        assert_eq!(
            run["structuredContent"]["declared_outputs"],
            json!(["sorted.sam"])
        );
        assert!(dir.path().join("sorted.sam").is_file());

        let rerun = &responses[4]["result"];
        assert_eq!(rerun["isError"], true);
        assert!(
            rerun["content"][0]["text"]
                .as_str()
                .expect("text")
                .contains("refusing to overwrite")
        );

        assert!(responses[5]["result"]["structuredContent"]["fallback_ready"].is_boolean());
        assert_eq!(
            responses[6]["result"]["structuredContent"]["commands"][0]["name"],
            "MarkDuplicates"
        );
        let gatk_run = &responses[7]["result"];
        assert_eq!(gatk_run["isError"], false, "{gatk_run}");
        assert_eq!(gatk_run["structuredContent"]["exit_code"], 0);
        assert_eq!(
            gatk_run["structuredContent"]["declared_outputs"],
            json!(["sorted_gatk.sam"])
        );
        assert!(dir.path().join("sorted_gatk.sam").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn mcp_verify_reports_pass_and_catches_alignment_mismatch_without_writing_workspace() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir exists");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/markduplicates/basic/input.sam");
        std::fs::copy(&fixture, dir.path().join("input.sam")).expect("fixture copied");

        let turbo_bin = assert_cmd::cargo::cargo_bin!("turbo-picard");
        let matching_picard = dir.path().join("mock-picard-match.sh");
        std::fs::write(
            &matching_picard,
            format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", turbo_bin.display()),
        )
        .expect("matching mock written");
        std::fs::set_permissions(&matching_picard, std::fs::Permissions::from_mode(0o755))
            .expect("chmod ok");

        let pass_responses = session_with_env(
            &[call(
                1,
                "verify",
                &json!({
                    "command": "SortSam",
                    "args": ["-I", "input.sam", "--output", "should_not_exist.sam", "--sort-order", "queryname", "--quiet"]
                }),
            )],
            dir.path(),
            &[(
                "TURBO_PICARD_FALLBACK_COMMAND",
                matching_picard.to_str().expect("utf8 path"),
            )],
        );
        let pass = &pass_responses[0]["result"];
        assert_eq!(pass["isError"], false, "{pass}");
        assert_eq!(pass["structuredContent"]["verdict"], "pass");
        assert_eq!(pass["structuredContent"]["outputs"][0]["kind"], "alignment");
        assert_eq!(pass["structuredContent"]["outputs"][0]["matched"], true);
        assert!(!dir.path().join("should_not_exist.sam").exists());

        let differing_picard = dir.path().join("mock-picard-diff.sh");
        std::fs::write(
            &differing_picard,
            "#!/bin/sh\nout=''\nfor arg in \"$@\"; do\n  case \"$arg\" in OUTPUT=*) out=\"${arg#OUTPUT=}\" ;; esac\ndone\ncp input.sam \"$out\"\n",
        )
        .expect("differing mock written");
        std::fs::set_permissions(&differing_picard, std::fs::Permissions::from_mode(0o755))
            .expect("chmod ok");

        let mismatch_responses = session_with_env(
            &[call(
                1,
                "verify",
                &json!({
                    "command": "SortSam",
                    "args": ["-I", "input.sam", "--output", "never_created.sam", "--sort-order", "queryname"]
                }),
            )],
            dir.path(),
            &[(
                "TURBO_PICARD_FALLBACK_COMMAND",
                differing_picard.to_str().expect("utf8 path"),
            )],
        );
        let mismatch = &mismatch_responses[0]["result"];
        assert_eq!(mismatch["isError"], false, "{mismatch}");
        assert_eq!(mismatch["structuredContent"]["verdict"], "mismatch");
        assert_eq!(
            mismatch["structuredContent"]["outputs"][0]["matched"],
            false
        );
        assert!(!dir.path().join("never_created.sam").exists());
    }
}

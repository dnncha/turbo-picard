#[cfg(test)]
mod mcp_session {
    use assert_cmd::Command;
    use serde_json::{Value, json};
    use std::path::Path;

    fn session(messages: &[Value], cwd: &Path) -> Vec<Value> {
        let input = messages
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let output = Command::cargo_bin("turbo-picard")
            .expect("binary exists")
            .arg("mcp")
            .current_dir(cwd)
            .env_remove("TURBO_PICARD_REQUIRE_NATIVE")
            .write_stdin(format!("{input}\n"))
            .output()
            .expect("mcp server runs");
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
            ],
            dir.path(),
        );

        assert_eq!(responses.len(), 7, "the notification gets no response");
        assert_eq!(responses[0]["result"]["protocolVersion"], "2025-06-18");
        let tools = responses[1]["result"]["tools"]
            .as_array()
            .expect("tools array");
        assert_eq!(tools.len(), 5);

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
    }
}

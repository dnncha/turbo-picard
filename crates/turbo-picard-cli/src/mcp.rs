use crate::command_error::CommandError;
use serde_json::{Map, Value, json};
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
const OUTPUT_TAIL_BYTES: usize = 16 * 1024;
const REPORT_SUBCOMMANDS: [&str; 6] = [
    "capabilities",
    "doctor",
    "explain",
    "trial",
    "mcp",
    "AccelerationStatus",
];

const INSTRUCTIONS: &str = "turbo-picard runs selected Picard commands natively and delegates the rest to upstream Picard when fallback_ready is true. For any Picard-shaped task: call explain with the exact command and KEY=VALUE arguments first. Use run when status is native, or partial-native with options inside native_scope, or when fallback_ready is true. Otherwise install upstream Picard (doctor gives the hint) or call Picard directly. run refuses to overwrite inputs or existing outputs unless overwrite is true.";

pub(crate) fn serve() -> i32 {
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            return 1;
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_message(&line)
            && (writeln!(stdout, "{response}").is_err() || stdout.flush().is_err())
        {
            return 1;
        }
    }
    0
}

fn handle_message(line: &str) -> Option<Value> {
    let Ok(message) = serde_json::from_str::<Value>(line) else {
        return Some(error_response(&Value::Null, -32700, "parse error"));
    };
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = id?;
    let result = match method {
        "initialize" => Ok(initialize_result(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => Ok(call_tool(&params)),
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(&id, code, &message),
    })
}

fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize_result(params: &Value) -> Value {
    let requested = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    let protocol = SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .find(|version| **version == requested)
        .copied()
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "turbo-picard", "version": env!("CARGO_PKG_VERSION") },
        "instructions": INSTRUCTIONS,
    })
}

fn command_args_schema(description: &str) -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "Picard command name, for example MarkDuplicates." },
            "args": {
                "type": "array",
                "items": { "type": "string" },
                "description": description,
            },
        },
        "required": ["command"],
        "additionalProperties": false,
    })
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "capabilities",
            "title": "List turbo-picard command coverage",
            "description": "Returns the capabilities --json contract: every Picard command with native, partial-native or fallback-only status, native and fallback scope, and saved benchmark evidence. Pass command to get one entry without the benchmark table.",
            "inputSchema": {
                "type": "object",
                "properties": { "command": { "type": "string" } },
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "explain",
            "title": "Explain the execution path for one Picard call",
            "description": "Call first for any Picard-shaped task. Returns explain --json for the exact command and KEY=VALUE arguments: status, native_scope, fallback_scope, execution_path, fallback_ready and declared outputs. Read-only; it does not open inputs or check option support.",
            "inputSchema": command_args_schema("Picard KEY=VALUE arguments, one per item."),
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "trial",
            "title": "Print a side-by-side Picard trial contract",
            "description": "Returns trial --json: picard_argv and turbo_argv arrays, declared outputs and what to compare when evaluating turbo-picard beside upstream Picard. Read-only.",
            "inputSchema": command_args_schema("Picard KEY=VALUE arguments, one per item."),
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "doctor",
            "title": "Report the local turbo-picard install",
            "description": "Returns version, thread policy and fallback state as JSON, including fallback_ready and an install hint when upstream Picard is missing.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "run",
            "title": "Run a Picard command with turbo-picard",
            "description": "Runs turbo-picard <command> <args> without a shell. Native work runs natively; anything outside native scope delegates to upstream Picard when fallback_ready is true. Refuses when an output argument names an input or an existing file unless overwrite is true. Set require_native to fail instead of delegating.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Picard command name, for example SortSam." },
                    "args": { "type": "array", "items": { "type": "string" }, "description": "Picard KEY=VALUE arguments, one per item." },
                    "cwd": { "type": "string", "description": "Working directory for relative paths. Defaults to the server's working directory." },
                    "require_native": { "type": "boolean", "description": "Fail instead of delegating to upstream Picard." },
                    "overwrite": { "type": "boolean", "description": "Allow outputs that already exist." },
                },
                "required": ["command", "args"],
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false },
        },
    ])
}

fn call_tool(params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params
        .get("arguments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let outcome = match name {
        "capabilities" => capabilities_tool(&arguments),
        "explain" => report_tool("explain", &arguments),
        "trial" => report_tool("trial", &arguments),
        "doctor" => doctor_tool(),
        "run" => run_tool(&arguments).map_err(|error| error.to_string()),
        _ => Err(format!("unknown tool: {name}")),
    };
    match outcome {
        Ok((structured, is_error)) => json!({
            "content": [{ "type": "text", "text": structured.to_string() }],
            "structuredContent": structured,
            "isError": is_error,
        }),
        Err(message) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true,
        }),
    }
}

fn string_field(arguments: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn bool_field(arguments: &Map<String, Value>, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{key} must be a boolean")),
    }
}

fn args_field(arguments: &Map<String, Value>) -> Result<Vec<String>, String> {
    match arguments.get("args") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| "args must be an array of strings".to_string())
            })
            .collect(),
        Some(_) => Err("args must be an array of strings".to_string()),
    }
}

fn required_command(arguments: &Map<String, Value>) -> Result<String, String> {
    let command = string_field(arguments, "command")?
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| "command is required".to_string())?;
    if command.starts_with('-') {
        return Err(format!("command must be a Picard command name: {command}"));
    }
    Ok(command)
}

struct Captured {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

fn run_self(args: &[String], cwd: Option<&Path>, require_native: bool) -> Result<Captured, String> {
    let exe = env::current_exe().map_err(|error| format!("cannot locate turbo-picard: {error}"))?;
    let mut command = Command::new(exe);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    if require_native {
        command.env("TURBO_PICARD_REQUIRE_NATIVE", "1");
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to start turbo-picard: {error}"))?;
    Ok(Captured {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn parse_json_report(captured: &Captured) -> Result<(Value, bool), String> {
    if captured.exit_code != 0 {
        return Err(captured.stderr.trim().to_string());
    }
    serde_json::from_str(&captured.stdout)
        .map(|value| (value, false))
        .map_err(|error| format!("turbo-picard returned invalid JSON: {error}"))
}

fn capabilities_tool(arguments: &Map<String, Value>) -> Result<(Value, bool), String> {
    let mut args = vec!["capabilities".to_string(), "--json".to_string()];
    if let Some(command) = string_field(arguments, "command")? {
        args.push("--command".to_string());
        args.push(command);
    }
    parse_json_report(&run_self(&args, None, false)?)
}

fn report_tool(report: &str, arguments: &Map<String, Value>) -> Result<(Value, bool), String> {
    let mut args = vec![
        report.to_string(),
        "--json".to_string(),
        required_command(arguments)?,
    ];
    args.extend(args_field(arguments)?);
    parse_json_report(&run_self(&args, None, false)?)
}

fn doctor_tool() -> Result<(Value, bool), String> {
    let captured = run_self(&["doctor".to_string()], None, false)?;
    if captured.exit_code != 0 {
        return Err(captured.stderr.trim().to_string());
    }
    Ok((doctor_json(&captured.stdout), false))
}

fn doctor_json(text: &str) -> Value {
    let mut report = Map::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = match value {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => Value::String(other.to_string()),
        };
        report.insert(key.to_string(), value);
    }
    Value::Object(report)
}

fn run_tool(arguments: &Map<String, Value>) -> Result<(Value, bool), CommandError> {
    let command = required_command(arguments)?;
    if REPORT_SUBCOMMANDS.contains(&command.as_str()) {
        return Err(format!("{command} is a report; call its own tool instead of run").into());
    }
    if super::command_matrix_entry(&command).is_none()
        && !super::is_picard_reference_command(&command)
    {
        return Err(CommandError::unsupported_command(&command));
    }
    let args = args_field(arguments)?;
    let cwd = match string_field(arguments, "cwd")? {
        Some(cwd) => PathBuf::from(cwd),
        None => {
            env::current_dir().map_err(|error| format!("cannot read working directory: {error}"))?
        }
    };
    if !cwd.is_dir() {
        return Err(format!("cwd is not a directory: {}", cwd.display()).into());
    }
    let overwrite = bool_field(arguments, "overwrite")?;
    let require_native = bool_field(arguments, "require_native")?;
    let declared = guard_outputs(&command, &args, &cwd, overwrite)?;
    let mut invocation = vec![command.clone()];
    invocation.extend(args);
    let captured = run_self(&invocation, Some(&cwd), require_native)?;
    let readiness = super::fallback_readiness();
    let status = super::command_matrix_entry(&command)
        .map_or_else(|| "fallback-only".to_string(), |entry| entry.status);
    let report = json!({
        "command": command,
        "argv": std::iter::once("turbo-picard".to_string()).chain(invocation.iter().skip(1).cloned()).collect::<Vec<_>>(),
        "cwd": cwd.display().to_string(),
        "status": status,
        "require_native": require_native,
        "fallback_ready": readiness.ready && !require_native,
        "exit_code": captured.exit_code,
        "declared_outputs": declared,
        "stdout_tail": tail(&captured.stdout),
        "stderr_tail": tail(&captured.stderr),
    });
    Ok((report, captured.exit_code != 0))
}

fn guard_outputs(
    command: &str,
    args: &[String],
    cwd: &Path,
    overwrite: bool,
) -> Result<Vec<String>, String> {
    let declared = crate::agent_contract::declared_outputs(command, args)?;
    let inputs = argument_values(args, &["I", "INPUT"]);
    let mut outputs = Vec::new();
    for declared_output in &declared {
        let Some((_, value)) = declared_output.split_once('=') else {
            continue;
        };
        if value == "/dev/stdout" || value == "-" {
            continue;
        }
        let path = cwd.join(value);
        if inputs
            .iter()
            .any(|input| same_path(&cwd.join(input), &path))
        {
            return Err(format!("refusing to run: output {value} is also an input"));
        }
        if path.exists() && !overwrite {
            return Err(format!(
                "refusing to overwrite existing output {value}; choose a new path or pass overwrite=true"
            ));
        }
        outputs.push(value.to_string());
    }
    Ok(outputs)
}

fn argument_values(args: &[String], keys: &[&str]) -> Vec<String> {
    let mut values = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        let flag = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-'));
        if let Some(flag) = flag {
            if let Some((key, value)) = flag.split_once('=') {
                if keys.contains(&key) {
                    values.push(value.to_string());
                }
            } else if keys.contains(&flag) {
                if let Some(value) = args.get(index + 1) {
                    values.push(value.clone());
                }
                index += 1;
            }
        } else if let Some((key, value)) = arg.split_once('=')
            && keys.contains(&key)
        {
            values.push(value.to_string());
        }
        index += 1;
    }
    values
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn tail(text: &str) -> String {
    if text.len() <= OUTPUT_TAIL_BYTES {
        return text.to_string();
    }
    let mut start = text.len() - OUTPUT_TAIL_BYTES;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    text[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn notifications_get_no_response_and_unknown_methods_error() {
        assert!(
            handle_message(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none()
        );
        let response =
            handle_message(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#).unwrap();
        assert_eq!(response["error"]["code"], -32601);
        let response = handle_message("not json").unwrap();
        assert_eq!(response["error"]["code"], -32700);
    }

    #[test]
    fn initialize_negotiates_a_supported_protocol() {
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26"}}"#,
        )
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(response["result"]["serverInfo"]["name"], "turbo-picard");
        let response = handle_message(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        )
        .unwrap();
        assert_eq!(
            response["result"]["protocolVersion"],
            SUPPORTED_PROTOCOL_VERSIONS[0]
        );
    }

    #[test]
    fn tools_list_names_every_tool() {
        let names = tool_definitions()
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(names, ["capabilities", "explain", "trial", "doctor", "run"]);
    }

    #[test]
    fn doctor_text_becomes_typed_json() {
        let report = doctor_json("fallback_ready=false\nfallback_command=not-found\nnoise\n");
        assert_eq!(report["fallback_ready"], false);
        assert_eq!(report["fallback_command"], "not-found");
    }

    #[test]
    fn run_guard_rejects_outputs_that_overwrite_inputs_or_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("in.bam"), b"x").unwrap();
        let error = guard_outputs(
            "SortSam",
            &strings(&["I=in.bam", "O=in.bam"]),
            dir.path(),
            true,
        )
        .unwrap_err();
        assert!(error.contains("also an input"), "{error}");
        std::fs::write(dir.path().join("exists.bam"), b"x").unwrap();
        let error = guard_outputs(
            "SortSam",
            &strings(&["I=in.bam", "O=exists.bam"]),
            dir.path(),
            false,
        )
        .unwrap_err();
        assert!(error.contains("overwrite"), "{error}");
        assert_eq!(
            guard_outputs(
                "SortSam",
                &strings(&["--INPUT", "in.bam", "O=exists.bam"]),
                dir.path(),
                true
            )
            .unwrap(),
            ["exists.bam"]
        );
        assert_eq!(
            guard_outputs(
                "SortSam",
                &strings(&["I=in.bam", "O=new.bam"]),
                dir.path(),
                false
            )
            .unwrap(),
            ["new.bam"]
        );
    }

    #[test]
    fn run_rejects_reports_and_unknown_commands() {
        let mut arguments = Map::new();
        arguments.insert("command".to_string(), json!("explain"));
        arguments.insert("args".to_string(), json!([]));
        assert!(matches!(
            run_tool(&arguments),
            Err(CommandError::Failed(message)) if message.contains("report")
        ));
        arguments.insert("command".to_string(), json!("NotAPicardTool"));
        assert!(matches!(
            run_tool(&arguments),
            Err(CommandError::Unsupported(_))
        ));
    }

    #[test]
    fn argument_values_reads_every_supported_syntax() {
        assert_eq!(
            argument_values(
                &strings(&[
                    "I=a.bam",
                    "--INPUT",
                    "b.bam",
                    "--I=c.bam",
                    "-I",
                    "d.bam",
                    "O=x"
                ]),
                &["I", "INPUT"]
            ),
            ["a.bam", "b.bam", "c.bam", "d.bam"]
        );
    }

    #[test]
    fn tail_keeps_the_end_on_a_char_boundary() {
        let text = format!("{}é{}", "a".repeat(10), "b".repeat(OUTPUT_TAIL_BYTES - 1));
        let kept = tail(&text);
        assert!(kept.len() <= OUTPUT_TAIL_BYTES);
        assert!(kept.ends_with('b'));
    }
}

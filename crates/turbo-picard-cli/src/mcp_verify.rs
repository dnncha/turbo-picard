use crate::agent_contract;
use crate::command_error::CommandError;
use crate::mcp::{
    Captured, REPORT_SUBCOMMANDS, args_field, required_command, run_self, string_field, tail,
};
use flate2::read::MultiGzDecoder;
use rust_htslib::bam::{self, Read as BamRead};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::Read as IoRead;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use turbo_picard_core::hts_io;
use turbo_picard_core::picard_args::{ParsedPicardArg, parse_picard_args_for_command};

pub(crate) fn verify_tool(arguments: &Map<String, Value>) -> Result<(Value, bool), CommandError> {
    let command = required_command(arguments)?;
    if REPORT_SUBCOMMANDS.contains(&command.as_str()) {
        return Err(format!("{command} is a report; call its own tool instead of verify").into());
    }
    if crate::command_matrix_entry(&command).is_none()
        && !crate::is_picard_reference_command(&command)
    {
        return Err(CommandError::unsupported_command(&command));
    }
    let args = args_field(arguments)?;
    let parsed = parse_picard_args_for_command(&command, &args)
        .map_err(|error| CommandError::Failed(error.to_string()))?;
    let cwd = match string_field(arguments, "cwd")? {
        Some(cwd) => PathBuf::from(cwd),
        None => {
            env::current_dir().map_err(|error| format!("cannot read working directory: {error}"))?
        }
    };
    if !cwd.is_dir() {
        return Err(format!("cwd is not a directory: {}", cwd.display()).into());
    }
    let Some(upstream) = crate::resolve_upstream_picard_for_verification()
        .filter(|cmd| crate::fallback_program_available(cmd))
    else {
        return Ok((
            json!({
                "command": command,
                "verdict": "unavailable",
                "fallback_ready": false,
                "fallback_install_hint": crate::fallback_install_hint(),
            }),
            true,
        ));
    };

    let tempdir = tempfile::tempdir()
        .map_err(|error| format!("failed to create verification directory: {error}"))?;
    let turbo_dir = tempdir.path().join("turbo");
    let picard_dir = tempdir.path().join("picard");
    fs::create_dir_all(&turbo_dir)
        .map_err(|error| format!("failed to create turbo temp dir: {error}"))?;
    fs::create_dir_all(&picard_dir)
        .map_err(|error| format!("failed to create picard temp dir: {error}"))?;

    let (turbo_args, picard_args) =
        build_isolated_invocations(&command, &parsed, &turbo_dir, &picard_dir)?;

    let turbo_started = Instant::now();
    let turbo_captured = run_self(&turbo_args, Some(&cwd), true)?;
    let turbo_elapsed = turbo_started.elapsed();

    let picard_started = Instant::now();
    let picard_captured = run_upstream_captured(&upstream, &picard_args, &cwd)?;
    let picard_elapsed = picard_started.elapsed();

    let turbo_ms = u64::try_from(turbo_elapsed.as_millis()).unwrap_or(u64::MAX);
    let picard_ms = u64::try_from(picard_elapsed.as_millis()).unwrap_or(u64::MAX);
    let speedup = if turbo_elapsed.as_secs_f64() > 0.0 {
        Some((picard_elapsed.as_secs_f64() / turbo_elapsed.as_secs_f64() * 100.0).round() / 100.0)
    } else {
        None
    };

    if turbo_captured.exit_code != 0 || picard_captured.exit_code != 0 {
        let verdict = match (
            turbo_captured.exit_code != 0,
            picard_captured.exit_code != 0,
        ) {
            (true, true) => "both_failed",
            (true, false) => "turbo_failed",
            (false, true) => "picard_failed",
            (false, false) => unreachable!(),
        };
        let report = json!({
            "command": command,
            "verdict": verdict,
            "require_native": true,
            "turbo": {
                "exit_code": turbo_captured.exit_code,
                "wall_ms": turbo_ms,
                "stderr_tail": tail(&turbo_captured.stderr),
            },
            "picard": {
                "exit_code": picard_captured.exit_code,
                "wall_ms": picard_ms,
                "stderr_tail": tail(&picard_captured.stderr),
            },
            "speedup": speedup,
            "outputs": [],
        });
        return Ok((report, true));
    }

    let reference = resolved_reference(&parsed, &cwd);
    let outputs = compare_output_directories(
        &turbo_dir,
        &picard_dir,
        reference.as_deref(),
        &turbo_captured,
        &picard_captured,
    )?;
    let all_matched = outputs.iter().all(|entry| entry.matched);
    let verdict = if all_matched { "pass" } else { "mismatch" };

    let report = json!({
        "command": command,
        "verdict": verdict,
        "require_native": true,
        "turbo": {
            "exit_code": turbo_captured.exit_code,
            "wall_ms": turbo_ms,
            "stderr_tail": tail(&turbo_captured.stderr),
        },
        "picard": {
            "exit_code": picard_captured.exit_code,
            "wall_ms": picard_ms,
            "stderr_tail": tail(&picard_captured.stderr),
        },
        "speedup": speedup,
        "outputs": outputs.into_iter().map(OutputComparison::into_json).collect::<Vec<_>>(),
    });
    Ok((report, false))
}

fn build_isolated_invocations(
    command: &str,
    parsed: &[ParsedPicardArg],
    turbo_dir: &Path,
    picard_dir: &Path,
) -> Result<(Vec<String>, Vec<String>), String> {
    let mut turbo_args = vec![command.to_string()];
    let mut picard_args = vec![command.to_string()];
    let mut output_index = 0usize;

    for arg in parsed {
        if agent_contract::is_output_argument(command, &arg.canonical_key) {
            if arg.value == "-" || arg.value == "/dev/stdout" {
                return Err("verify requires file outputs, not stdout".to_string());
            }
            let rel = safe_output_relpath(&arg.value, output_index)?;
            output_index += 1;
            let turbo_path = turbo_dir.join(&rel);
            let picard_path = picard_dir.join(&rel);
            if let Some(parent) = turbo_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to prepare turbo output dir: {error}"))?;
            }
            if let Some(parent) = picard_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("failed to prepare picard output dir: {error}"))?;
            }
            turbo_args.push(format!("{}={}", arg.canonical_key, turbo_path.display()));
            picard_args.push(format!("{}={}", arg.canonical_key, picard_path.display()));
        } else {
            let formatted = format!("{}={}", arg.canonical_key, arg.value);
            turbo_args.push(formatted.clone());
            picard_args.push(formatted);
        }
    }

    Ok((turbo_args, picard_args))
}

fn safe_output_relpath(raw: &str, index: usize) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    let mut clean = PathBuf::new();
    let mut simple_relative = true;
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                simple_relative = false;
                break;
            }
        }
    }
    if simple_relative && !clean.as_os_str().is_empty() {
        return Ok(clean);
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("output path has no filename: {raw}"))?;
    Ok(PathBuf::from(format!("output_{index}")).join(file_name))
}

fn run_upstream_captured(upstream: &str, args: &[String], cwd: &Path) -> Result<Captured, String> {
    let mut parts = crate::split_fallback_command(upstream)?;
    let program = parts.remove(0);
    let mut command = Command::new(program);
    command
        .args(parts)
        .args(args)
        .current_dir(cwd)
        .env_remove("TURBO_PICARD_FALLBACK_COMMAND")
        .env(
            "JDK_JAVA_OPTIONS",
            crate::jvm_options_with_logs_on_stderr(env::var("JDK_JAVA_OPTIONS").ok().as_deref()),
        );
    let output = command
        .output()
        .map_err(|error| format!("failed to start upstream Picard: {error}"))?;
    Ok(Captured {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn resolved_reference(parsed: &[ParsedPicardArg], cwd: &Path) -> Option<String> {
    parsed
        .iter()
        .find(|arg| arg.canonical_key == "REFERENCE_SEQUENCE")
        .map(|arg| {
            let path = Path::new(&arg.value);
            if path.is_absolute() {
                arg.value.clone()
            } else {
                cwd.join(path).display().to_string()
            }
        })
}

struct OutputComparison {
    path: String,
    kind: &'static str,
    matched: bool,
    detail: String,
    records_compared: Option<u64>,
}

impl OutputComparison {
    fn into_json(self) -> Value {
        let mut object = Map::new();
        object.insert("path".to_string(), json!(self.path));
        object.insert("kind".to_string(), json!(self.kind));
        object.insert("matched".to_string(), json!(self.matched));
        object.insert("detail".to_string(), json!(self.detail));
        if let Some(records) = self.records_compared {
            object.insert("records_compared".to_string(), json!(records));
        }
        Value::Object(object)
    }
}

fn compare_output_directories(
    turbo_dir: &Path,
    picard_dir: &Path,
    reference: Option<&str>,
    turbo_captured: &Captured,
    picard_captured: &Captured,
) -> Result<Vec<OutputComparison>, String> {
    let mut turbo_files = BTreeSet::new();
    let mut picard_files = BTreeSet::new();
    collect_files(turbo_dir, turbo_dir, &mut turbo_files)?;
    collect_files(picard_dir, picard_dir, &mut picard_files)?;

    let all_files = turbo_files
        .union(&picard_files)
        .cloned()
        .collect::<Vec<_>>();
    if all_files.is_empty() {
        let matched = turbo_captured.stdout == picard_captured.stdout;
        return Ok(vec![OutputComparison {
            path: "stdout".to_string(),
            kind: "stdout",
            matched,
            detail: if matched {
                "exact stdout match".to_string()
            } else {
                "stdout differs".to_string()
            },
            records_compared: None,
        }]);
    }

    let mut results = Vec::with_capacity(all_files.len());
    for rel in all_files {
        let rel_display = rel.display().to_string();
        let turbo_path = turbo_dir.join(&rel);
        let picard_path = picard_dir.join(&rel);
        if !turbo_files.contains(&rel) {
            results.push(OutputComparison {
                path: rel_display,
                kind: "missing",
                matched: false,
                detail: "produced by Picard but missing in turbo-picard".to_string(),
                records_compared: None,
            });
            continue;
        }
        if !picard_files.contains(&rel) {
            results.push(OutputComparison {
                path: rel_display,
                kind: "missing",
                matched: false,
                detail: "produced by turbo-picard but missing in Picard".to_string(),
                records_compared: None,
            });
            continue;
        }
        results.push(compare_output_file(
            &turbo_path,
            &picard_path,
            &rel_display,
            reference,
            turbo_dir,
            picard_dir,
        )?);
    }
    Ok(results)
}

fn collect_files(root: &Path, current: &Path, out: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else if path.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_path_buf();
            out.insert(rel);
        }
    }
    Ok(())
}

fn compare_output_file(
    turbo_path: &Path,
    picard_path: &Path,
    rel_display: &str,
    reference: Option<&str>,
    turbo_dir: &Path,
    picard_dir: &Path,
) -> Result<OutputComparison, String> {
    let ext = hts_io::path_extension_lower(rel_display);
    match ext.as_deref() {
        Some("sam" | "bam" | "cram") => {
            return compare_alignment(turbo_path, picard_path, rel_display, reference);
        }
        Some("pdf") => return compare_pdf(turbo_path, picard_path, rel_display),
        Some("bai") => {
            return compare_bai(turbo_path, picard_path, rel_display, turbo_dir, picard_dir);
        }
        Some("gz") => {
            let is_vcf = Path::new(rel_display)
                .file_stem()
                .and_then(|stem| Path::new(stem).extension())
                .is_some_and(|inner| inner.eq_ignore_ascii_case("vcf"));
            return compare_gzip(turbo_path, picard_path, rel_display, is_vcf);
        }
        _ => {}
    }

    let turbo_bytes = fs::read(turbo_path).map_err(|error| error.to_string())?;
    let picard_bytes = fs::read(picard_path).map_err(|error| error.to_string())?;
    if let (Ok(turbo_text), Ok(picard_text)) = (
        std::str::from_utf8(&turbo_bytes),
        std::str::from_utf8(&picard_bytes),
    ) {
        if picard_text.contains("## METRICS CLASS") || turbo_text.contains("## METRICS CLASS") {
            return Ok(compare_picard_metrics(turbo_text, picard_text, rel_display));
        }
        if ext.as_deref() == Some("vcf") {
            return Ok(compare_vcf_text(turbo_text, picard_text, rel_display));
        }
    }

    let matched = turbo_bytes == picard_bytes;
    Ok(OutputComparison {
        path: rel_display.to_string(),
        kind: "bytes",
        matched,
        detail: if matched {
            "exact byte match".to_string()
        } else {
            format!(
                "bytes differ (turbo={} bytes, picard={} bytes)",
                turbo_bytes.len(),
                picard_bytes.len()
            )
        },
        records_compared: None,
    })
}

fn compare_alignment(
    turbo_path: &Path,
    picard_path: &Path,
    rel_display: &str,
    reference: Option<&str>,
) -> Result<OutputComparison, String> {
    let mut turbo_reader =
        hts_io::open_reader(turbo_path, reference).map_err(|error| error.to_string())?;
    let mut picard_reader =
        hts_io::open_reader(picard_path, reference).map_err(|error| error.to_string())?;

    if let Some(header_diff) = compare_sam_headers(turbo_reader.header(), picard_reader.header()) {
        return Ok(OutputComparison {
            path: rel_display.to_string(),
            kind: "alignment",
            matched: false,
            detail: header_diff,
            records_compared: Some(0),
        });
    }

    let turbo_header = turbo_reader.header().clone();
    let picard_header = picard_reader.header().clone();
    let mut turbo_rec = bam::Record::new();
    let mut picard_rec = bam::Record::new();
    let mut compared = 0u64;

    loop {
        let turbo_next = turbo_reader.read(&mut turbo_rec);
        let picard_next = picard_reader.read(&mut picard_rec);
        match (turbo_next, picard_next) {
            (None, None) => {
                return Ok(OutputComparison {
                    path: rel_display.to_string(),
                    kind: "alignment",
                    matched: true,
                    detail: "exact header, record, flag, and tag parity".to_string(),
                    records_compared: Some(compared),
                });
            }
            (Some(Ok(())), Some(Ok(()))) => {
                if let Some(diff) =
                    compare_bam_records(&turbo_rec, &turbo_header, &picard_rec, &picard_header)
                {
                    return Ok(OutputComparison {
                        path: rel_display.to_string(),
                        kind: "alignment",
                        matched: false,
                        detail: format!("record {compared}: {diff}"),
                        records_compared: Some(compared),
                    });
                }
                compared += 1;
            }
            (Some(Err(error)), _) => {
                return Err(format!("failed reading turbo alignment: {error}"));
            }
            (_, Some(Err(error))) => {
                return Err(format!("failed reading Picard alignment: {error}"));
            }
            (Some(Ok(())), None) => {
                return Ok(OutputComparison {
                    path: rel_display.to_string(),
                    kind: "alignment",
                    matched: false,
                    detail: format!("turbo-picard has extra records after {compared}"),
                    records_compared: Some(compared),
                });
            }
            (None, Some(Ok(()))) => {
                return Ok(OutputComparison {
                    path: rel_display.to_string(),
                    kind: "alignment",
                    matched: false,
                    detail: format!("Picard has extra records after {compared}"),
                    records_compared: Some(compared),
                });
            }
        }
    }
}

fn compare_sam_headers(turbo: &bam::HeaderView, picard: &bam::HeaderView) -> Option<String> {
    let turbo_text = String::from_utf8_lossy(turbo.as_bytes());
    let picard_text = String::from_utf8_lossy(picard.as_bytes());
    let turbo_so = header_so(&turbo_text);
    let picard_so = header_so(&picard_text);
    if turbo_so != picard_so {
        return Some(format!(
            "header sort order differs: turbo={turbo_so:?}, picard={picard_so:?}"
        ));
    }
    let turbo_sq = header_sq(&turbo_text);
    let picard_sq = header_sq(&picard_text);
    if turbo_sq != picard_sq {
        return Some("header @SQ contigs differ".to_string());
    }
    let turbo_rg = header_rg(&turbo_text);
    let picard_rg = header_rg(&picard_text);
    if turbo_rg != picard_rg {
        return Some("header @RG lines differ".to_string());
    }
    None
}

fn header_so(header: &str) -> Option<String> {
    header
        .lines()
        .find(|line| line.starts_with("@HD\t"))
        .and_then(|line| {
            line.split('\t')
                .find_map(|field| field.strip_prefix("SO:").map(str::to_string))
        })
}

fn header_sq(header: &str) -> Vec<(String, String)> {
    header
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
        .map(|line| {
            let mut sn = String::new();
            let mut ln = String::new();
            for field in line.split('\t').skip(1) {
                if let Some(value) = field.strip_prefix("SN:") {
                    sn = value.to_string();
                } else if let Some(value) = field.strip_prefix("LN:") {
                    ln = value.to_string();
                }
            }
            (sn, ln)
        })
        .collect()
}

fn header_rg(header: &str) -> BTreeSet<String> {
    header
        .lines()
        .filter(|line| line.starts_with("@RG\t"))
        .map(|line| {
            let mut fields = line.split('\t').skip(1).collect::<Vec<_>>();
            fields.sort_unstable();
            fields.join("\t")
        })
        .collect()
}

fn contig_name(header: &bam::HeaderView, tid: i32) -> Option<String> {
    u32::try_from(tid)
        .ok()
        .map(|tid| String::from_utf8_lossy(header.tid2name(tid)).into_owned())
}

fn compare_bam_records(
    turbo: &bam::Record,
    turbo_header: &bam::HeaderView,
    picard: &bam::Record,
    picard_header: &bam::HeaderView,
) -> Option<String> {
    if turbo.qname() != picard.qname() {
        return Some(format!(
            "qname differs: turbo={}, picard={}",
            String::from_utf8_lossy(turbo.qname()),
            String::from_utf8_lossy(picard.qname())
        ));
    }
    let qname = String::from_utf8_lossy(turbo.qname());
    if turbo.flags() != picard.flags() {
        return Some(format!(
            "{qname}: flags differ (turbo={}, picard={})",
            turbo.flags(),
            picard.flags()
        ));
    }
    if contig_name(turbo_header, turbo.tid()) != contig_name(picard_header, picard.tid())
        || turbo.pos() != picard.pos()
        || turbo.mapq() != picard.mapq()
        || turbo.cigar() != picard.cigar()
        || contig_name(turbo_header, turbo.mtid()) != contig_name(picard_header, picard.mtid())
        || turbo.mpos() != picard.mpos()
        || turbo.insert_size() != picard.insert_size()
        || turbo.seq().as_bytes() != picard.seq().as_bytes()
        || turbo.qual() != picard.qual()
    {
        return Some(format!("{qname}: alignment fields differ"));
    }
    let turbo_aux = canonical_aux(turbo);
    let picard_aux = canonical_aux(picard);
    if turbo_aux != picard_aux {
        return Some(format!(
            "{qname}: aux tags differ (turbo={turbo_aux:?}, picard={picard_aux:?})"
        ));
    }
    None
}

fn canonical_aux(record: &bam::Record) -> BTreeMap<String, String> {
    let mut tags = BTreeMap::new();
    for (tag, value) in record.aux_iter().flatten() {
        tags.insert(
            String::from_utf8_lossy(tag).into_owned(),
            format!("{value:?}"),
        );
    }
    tags
}

fn compare_picard_metrics(turbo: &str, picard: &str, rel_display: &str) -> OutputComparison {
    let turbo_lines = normalized_metrics_lines(turbo);
    let picard_lines = normalized_metrics_lines(picard);
    if turbo_lines == picard_lines {
        return OutputComparison {
            path: rel_display.to_string(),
            kind: "picard_metrics",
            matched: true,
            detail: "exact metrics and histogram parity".to_string(),
            records_compared: Some(u64::try_from(turbo_lines.len()).unwrap_or(u64::MAX)),
        };
    }
    let detail = turbo_lines
        .iter()
        .zip(&picard_lines)
        .enumerate()
        .find_map(|(line_no, (left, right))| {
            (left != right).then(|| {
                format!(
                    "line {} differs: turbo={left:?}, picard={right:?}",
                    line_no + 1
                )
            })
        })
        .unwrap_or_else(|| {
            format!(
                "metrics line count differs: turbo={}, picard={}",
                turbo_lines.len(),
                picard_lines.len()
            )
        });
    OutputComparison {
        path: rel_display.to_string(),
        kind: "picard_metrics",
        matched: false,
        detail,
        records_compared: None,
    }
}

fn normalized_metrics_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| {
            if line.starts_with('#') {
                line.starts_with("## METRICS CLASS") || line.starts_with("## HISTOGRAM")
            } else {
                !line.trim().is_empty()
            }
        })
        .map(str::to_string)
        .collect()
}

fn compare_vcf_text(turbo: &str, picard: &str, rel_display: &str) -> OutputComparison {
    let normalize = |text: &str| {
        text.lines()
            .map(|line| line.trim_end_matches('\r'))
            .filter(|line| {
                !line.starts_with("##fileDate=")
                    && !line.starts_with("##commandline=")
                    && !line.starts_with("##Picard")
            })
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    let turbo_lines = normalize(turbo);
    let picard_lines = normalize(picard);
    let matched = turbo_lines == picard_lines;
    OutputComparison {
        path: rel_display.to_string(),
        kind: "vcf",
        matched,
        detail: if matched {
            "exact VCF parity".to_string()
        } else {
            "VCF records or headers differ".to_string()
        },
        records_compared: None,
    }
}

fn compare_gzip(
    turbo_path: &Path,
    picard_path: &Path,
    rel_display: &str,
    is_vcf: bool,
) -> Result<OutputComparison, String> {
    let turbo_bytes = read_gzip(turbo_path)?;
    let picard_bytes = read_gzip(picard_path)?;
    if is_vcf
        && let (Ok(turbo_text), Ok(picard_text)) = (
            std::str::from_utf8(&turbo_bytes),
            std::str::from_utf8(&picard_bytes),
        )
    {
        return Ok(compare_vcf_text(turbo_text, picard_text, rel_display));
    }
    let matched = turbo_bytes == picard_bytes;
    Ok(OutputComparison {
        path: rel_display.to_string(),
        kind: "gzip",
        matched,
        detail: if matched {
            "exact decompressed byte match".to_string()
        } else {
            "decompressed bytes differ".to_string()
        },
        records_compared: None,
    })
}

fn read_gzip(path: &Path) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut decoder = MultiGzDecoder::new(file);
    let mut bytes = Vec::new();
    decoder
        .read_to_end(&mut bytes)
        .map_err(|error| format!("failed to decompress {}: {error}", path.display()))?;
    Ok(bytes)
}

fn compare_pdf(
    turbo_path: &Path,
    picard_path: &Path,
    rel_display: &str,
) -> Result<OutputComparison, String> {
    let turbo_bytes = fs::read(turbo_path).map_err(|error| error.to_string())?;
    let picard_bytes = fs::read(picard_path).map_err(|error| error.to_string())?;
    let both_pdf = turbo_bytes.starts_with(b"%PDF-") && picard_bytes.starts_with(b"%PDF-");
    Ok(OutputComparison {
        path: rel_display.to_string(),
        kind: "pdf_chart_summary",
        matched: both_pdf,
        detail: if both_pdf {
            "both outputs are valid PDFs (chart PDFs are lightweight summaries, not pixel-identical to Picard R plots)".to_string()
        } else {
            "invalid PDF header".to_string()
        },
        records_compared: None,
    })
}

fn compare_bai(
    turbo_path: &Path,
    picard_path: &Path,
    rel_display: &str,
    turbo_dir: &Path,
    picard_dir: &Path,
) -> Result<OutputComparison, String> {
    let turbo_bytes = fs::read(turbo_path).map_err(|error| error.to_string())?;
    let picard_bytes = fs::read(picard_path).map_err(|error| error.to_string())?;
    if turbo_bytes == picard_bytes {
        return Ok(OutputComparison {
            path: rel_display.to_string(),
            kind: "bai_index",
            matched: true,
            detail: "exact BAI byte match".to_string(),
            records_compared: None,
        });
    }
    if let (Some(turbo_bam), Some(picard_bam)) = (
        sibling_bam(turbo_path, turbo_dir),
        sibling_bam(picard_path, picard_dir),
    ) {
        let turbo_counts = indexed_contig_counts(&turbo_bam, turbo_path)?;
        let picard_counts = indexed_contig_counts(&picard_bam, picard_path)?;
        let matched = turbo_counts == picard_counts;
        return Ok(OutputComparison {
            path: rel_display.to_string(),
            kind: "bai_index",
            matched,
            detail: if matched {
                "indexed per-contig query counts match".to_string()
            } else {
                "indexed per-contig query counts differ".to_string()
            },
            records_compared: None,
        });
    }
    Ok(OutputComparison {
        path: rel_display.to_string(),
        kind: "bai_index",
        matched: false,
        detail: "BAI bytes differ and no sibling BAM is present to verify indexed queries"
            .to_string(),
        records_compared: None,
    })
}

fn sibling_bam(bai_path: &Path, root: &Path) -> Option<PathBuf> {
    let file_name = bai_path.file_name()?.to_str()?;
    let parent = bai_path.parent().unwrap_or(root);
    let candidate = if let Some(stripped) = file_name.strip_suffix(".bam.bai") {
        parent.join(format!("{stripped}.bam"))
    } else {
        bai_path.with_extension("bam")
    };
    candidate.is_file().then_some(candidate)
}

fn indexed_contig_counts(bam_path: &Path, bai_path: &Path) -> Result<Vec<(String, u64)>, String> {
    let mut reader = bam::IndexedReader::from_path_and_index(bam_path, bai_path)
        .map_err(|error| format!("failed opening indexed BAM {}: {error}", bam_path.display()))?;
    let header = reader.header().clone();
    let mut counts = Vec::new();
    for tid in 0..header.target_count() {
        reader
            .fetch(tid)
            .map_err(|error| format!("index fetch failed for tid {tid}: {error}"))?;
        let mut count = 0u64;
        let mut record = bam::Record::new();
        while let Some(result) = reader.read(&mut record) {
            result.map_err(|error| error.to_string())?;
            count += 1;
        }
        counts.push((
            String::from_utf8_lossy(header.tid2name(tid)).into_owned(),
            count,
        ));
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picard_metrics_comparison_ignores_header_comments_and_catches_value_diffs() {
        let left = "# Picard\n# Started on: Mon\n## METRICS CLASS\tpicard.sam.DuplicationMetrics\nLIBRARY\tREAD_PAIRS\nlib1\t100\n\n## HISTOGRAM\tjava.lang.Double\nBIN\tVALUE\n1\t1.0\n";
        let right = "# Turbo Picard\n## METRICS CLASS\tpicard.sam.DuplicationMetrics\nLIBRARY\tREAD_PAIRS\nlib1\t100\n\n## HISTOGRAM\tjava.lang.Double\nBIN\tVALUE\n1\t1.0\n";
        let same = compare_picard_metrics(left, right, "metrics.txt");
        assert!(same.matched, "{}", same.detail);

        let altered = right.replace("lib1\t100", "lib1\t101");
        let diff = compare_picard_metrics(left, &altered, "metrics.txt");
        assert!(!diff.matched);
        assert!(diff.detail.contains("100"), "{}", diff.detail);
    }

    #[test]
    fn safe_output_relpath_confines_absolute_and_parent_traversal_paths() {
        assert_eq!(
            safe_output_relpath("out/marked.bam", 0).unwrap(),
            PathBuf::from("out/marked.bam")
        );
        assert_eq!(
            safe_output_relpath("/tmp/marked.bam", 1).unwrap(),
            PathBuf::from("output_1/marked.bam")
        );
        assert_eq!(
            safe_output_relpath("../escape/marked.bam", 2).unwrap(),
            PathBuf::from("output_2/marked.bam")
        );
    }
}

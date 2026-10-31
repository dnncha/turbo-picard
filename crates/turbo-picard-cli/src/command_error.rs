use crate::cmm_pipeline::CmmPipelineError;
use crate::hs_metrics::HsMetricsError;
use crate::sam_structure::SamStructureError;
use crate::vcf_headers::VcfHeadersError;
use thiserror::Error;
use turbo_picard_core::hts_io::HtsIoError;
use turbo_picard_core::markdup_config::MarkDuplicatesConfigError;
use turbo_picard_core::picard_args::CommandArgError;
use turbo_picard_core::sort_error::SortError;
use turbo_picard_markdup::MarkDuplicatesError;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Failed(String),
}

impl CommandError {
    pub(crate) fn unsupported_command(command: &str) -> Self {
        CommandError::Unsupported(format!("unsupported Picard command: {command}"))
    }
}

pub(crate) fn unsupported(message: impl Into<String>) -> CommandError {
    CommandError::Unsupported(message.into())
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::Failed(message.to_string())
    }
}

impl From<HsMetricsError> for CommandError {
    fn from(error: HsMetricsError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<CmmPipelineError> for CommandError {
    fn from(error: CmmPipelineError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<SamStructureError> for CommandError {
    fn from(error: SamStructureError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<VcfHeadersError> for CommandError {
    fn from(error: VcfHeadersError) -> Self {
        match error {
            VcfHeadersError::Command(inner) => inner,
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<SortError> for CommandError {
    fn from(error: SortError) -> Self {
        Self::Failed(error.to_string())
    }
}

impl From<HtsIoError> for CommandError {
    fn from(error: HtsIoError) -> Self {
        match error {
            HtsIoError::UnsupportedOutputFormat { .. } => Self::Unsupported(error.to_string()),
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<MarkDuplicatesError> for CommandError {
    fn from(error: MarkDuplicatesError) -> Self {
        match error {
            MarkDuplicatesError::UnsupportedInputFormat(_) => Self::Unsupported(error.to_string()),
            MarkDuplicatesError::HtsIo(inner) => inner.into(),
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<MarkDuplicatesConfigError> for CommandError {
    fn from(error: MarkDuplicatesConfigError) -> Self {
        match error {
            MarkDuplicatesConfigError::UnsupportedOption(_) => Self::Unsupported(error.to_string()),
            other => Self::Failed(other.to_string()),
        }
    }
}

impl From<CommandArgError> for CommandError {
    fn from(error: CommandArgError) -> Self {
        match error {
            CommandArgError::Unknown { .. } | CommandArgError::InvalidValue { .. } => {
                Self::Unsupported(error.to_string())
            }
            other => Self::Failed(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    const SOURCES: [(&str, &str); 10] = [
        ("agent_contract.rs", include_str!("agent_contract.rs")),
        ("cmm_pipeline.rs", include_str!("cmm_pipeline.rs")),
        ("command_error.rs", include_str!("command_error.rs")),
        ("hs_metrics.rs", include_str!("hs_metrics.rs")),
        ("lib.rs", include_str!("lib.rs")),
        ("main.rs", include_str!("main.rs")),
        ("mcp.rs", include_str!("mcp.rs")),
        ("sam_sort.rs", include_str!("sam_sort.rs")),
        ("sam_structure.rs", include_str!("sam_structure.rs")),
        ("vcf_headers.rs", include_str!("vcf_headers.rs")),
    ];

    const DELEGATION_MARKERS: [&str; 2] = ["not implemented yet", "should use upstream Picard"];

    fn production_code(source: &str) -> &str {
        source
            .split("\n#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or(source)
    }

    fn triggers_delegation(literal: &str) -> bool {
        literal.starts_with("unsupported ")
            || DELEGATION_MARKERS
                .iter()
                .any(|marker| literal.contains(marker))
    }

    fn is_identifier_char(character: char) -> bool {
        character.is_alphanumeric() || character == '_'
    }

    fn callee_before(chars: &[char], open_paren: usize) -> String {
        let mut start = open_paren;
        while start > 0 && (is_identifier_char(chars[start - 1]) || ":!".contains(chars[start - 1]))
        {
            start -= 1;
        }
        chars[start..open_paren].iter().collect()
    }

    fn constructs_unsupported(callee: &str) -> bool {
        callee == "unsupported"
            || callee.ends_with("::unsupported")
            || callee.ends_with("CommandError::Unsupported")
    }

    fn raw_string_hashes(chars: &[char], index: usize) -> Option<usize> {
        let previous = index.checked_sub(1).map(|position| chars[position]);
        let before_previous = index.checked_sub(2).map(|position| chars[position]);
        let starts_token = match previous {
            None => true,
            Some('b') => before_previous.is_none_or(|character| !is_identifier_char(character)),
            Some(character) => !is_identifier_char(character),
        };
        if !starts_token {
            return None;
        }
        let hashes = chars[index + 1..]
            .iter()
            .take_while(|character| **character == '#')
            .count();
        (chars.get(index + 1 + hashes) == Some(&'"')).then_some(hashes)
    }

    fn end_of_raw_string(chars: &[char], opening_quote: usize, hashes: usize) -> usize {
        let mut index = opening_quote + 1;
        while index < chars.len() {
            if chars[index] == '"'
                && chars[index + 1..]
                    .iter()
                    .take(hashes)
                    .filter(|character| **character == '#')
                    .count()
                    == hashes
            {
                return index;
            }
            index += 1;
        }
        chars.len()
    }

    fn end_of_string(chars: &[char], opening_quote: usize) -> usize {
        let mut index = opening_quote + 1;
        while index < chars.len() && chars[index] != '"' {
            index += if chars[index] == '\\' { 2 } else { 1 };
        }
        index
    }

    fn unguarded_literals(file: &str, source: &str) -> Vec<String> {
        let chars: Vec<char> = production_code(source).chars().collect();
        let mut callees: Vec<String> = Vec::new();
        let mut violations = Vec::new();
        let mut index = 0;
        while index < chars.len() {
            let character = chars[index];
            let next = chars.get(index + 1).copied();
            if character == '/' && next == Some('/') {
                while index < chars.len() && chars[index] != '\n' {
                    index += 1;
                }
            } else if character == '/' && next == Some('*') {
                while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                    index += 1;
                }
                index += 2;
            } else if character == '"'
                || (character == 'r' && raw_string_hashes(&chars, index).is_some())
            {
                let (content_start, end) = match raw_string_hashes(&chars, index) {
                    Some(hashes) if character == 'r' => {
                        let opening_quote = index + 1 + hashes;
                        (
                            opening_quote + 1,
                            end_of_raw_string(&chars, opening_quote, hashes),
                        )
                    }
                    _ => (index + 1, end_of_string(&chars, index)),
                };
                let literal: String = chars[content_start..end.min(chars.len())].iter().collect();
                let guarded = callees.iter().any(|callee| constructs_unsupported(callee));
                if triggers_delegation(&literal) && !guarded {
                    let line = chars[..index].iter().filter(|c| **c == '\n').count() + 1;
                    violations.push(format!("{file}:{line}: {literal}"));
                }
                index = end + 1;
            } else if character == '\'' {
                if next == Some('\\') {
                    index += 3;
                    while index < chars.len() && chars[index] != '\'' {
                        index += 1;
                    }
                    index += 1;
                } else if chars.get(index + 2) == Some(&'\'') {
                    index += 3;
                } else {
                    index += 1;
                }
            } else {
                if character == '(' {
                    callees.push(callee_before(&chars, index));
                } else if character == ')' {
                    callees.pop();
                }
                index += 1;
            }
        }
        violations
    }

    #[test]
    fn every_delegation_message_is_built_as_an_unsupported_command_error() {
        let on_disk = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rs"))
                    .count()
            })
            .unwrap_or_default();
        assert_eq!(
            on_disk,
            SOURCES.len(),
            "src/*.rs changed; list every file in SOURCES"
        );

        let violations: Vec<String> = SOURCES
            .iter()
            .flat_map(|(file, source)| unguarded_literals(file, source))
            .collect();
        assert!(
            violations.is_empty(),
            "delegation messages must be built with CommandError::Unsupported or unsupported(...):\n{}",
            violations.join("\n")
        );
    }
}

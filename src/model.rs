use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleSet(u8);

impl RuleSet {
    pub const IMG_ALT: u8 = 1 << 0;
    pub const HTML_LANG: u8 = 1 << 1;
    pub const BUTTON_TYPE: u8 = 1 << 2;
    pub const IFRAME_TITLE: u8 = 1 << 3;
    pub const DOCTYPE_HTML: u8 = 1 << 4;
    pub const PARSE_ERROR: u8 = 1 << 5;

    pub const COMMON: Self = Self(Self::IMG_ALT);
    pub const ALL: Self = Self(
        Self::IMG_ALT
            | Self::HTML_LANG
            | Self::BUTTON_TYPE
            | Self::IFRAME_TITLE
            | Self::DOCTYPE_HTML
            | Self::PARSE_ERROR,
    );

    #[inline]
    pub const fn contains(self, rule: u8) -> bool {
        self.0 & rule != 0
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        if value == "all" {
            return Ok(Self::ALL);
        }
        if value == "common" {
            return Ok(Self::COMMON);
        }

        let mut bits = 0;
        for name in value.split(',') {
            bits |= match name {
                "img-alt" => Self::IMG_ALT,
                "html-lang" => Self::HTML_LANG,
                "button-type" => Self::BUTTON_TYPE,
                "iframe-title" => Self::IFRAME_TITLE,
                "doctype-html" => Self::DOCTYPE_HTML,
                "parse-error" => Self::PARSE_ERROR,
                "" => return Err("--rules cannot be empty".into()),
                unknown => return Err(format!("unknown rule: {unknown}")),
            };
        }
        Ok(Self(bits))
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u8,
    pub status: &'static str,
    pub files: Vec<FileReport>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<OperationalError>,
    pub summary: Summary,
}

impl Report {
    pub fn from_parts(files: Vec<FileReport>, errors: Vec<OperationalError>) -> Self {
        let bytes = files.iter().map(|file| file.bytes).sum();
        let findings = files.iter().map(|file| file.diagnostics.len() as u64).sum();
        let truncated = files.iter().any(|file| file.truncated);
        let status = if !errors.is_empty() {
            "error"
        } else if findings != 0 {
            "findings"
        } else {
            "clean"
        };

        Self {
            schema_version: 1,
            status,
            summary: Summary {
                files: files.len() as u64,
                bytes,
                findings,
                operational_errors: errors.len() as u64,
                truncated,
            },
            files,
            errors,
        }
    }

    pub fn operational(message: impl Into<String>) -> Self {
        Self::from_parts(
            Vec::new(),
            vec![OperationalError {
                path: None,
                message: message.into(),
            }],
        )
    }

    pub fn exit_code(&self) -> i32 {
        match self.status {
            "clean" => 0,
            "findings" => 1,
            _ => 2,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct FileReport {
    pub path: String,
    pub bytes: u64,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub rule: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: usize,
    pub column: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct OperationalError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub files: u64,
    pub bytes: u64,
    pub findings: u64,
    pub operational_errors: u64,
    pub truncated: bool,
}

#[derive(Debug)]
pub(crate) struct DiagnosticDraft {
    pub rule: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub help: Option<&'static str>,
}

impl DiagnosticDraft {
    pub fn resolve(self, input_len: usize, line_starts: &[usize]) -> Diagnostic {
        let offset = self.byte_start.min(input_len);
        let line_index = line_starts.partition_point(|start| *start <= offset) - 1;
        let line_start = line_starts[line_index];
        Diagnostic {
            rule: self.rule,
            severity: self.severity,
            message: self.message,
            byte_start: self.byte_start,
            byte_end: self.byte_end,
            line: line_index + 1,
            column: offset - line_start + 1,
            help: self.help,
        }
    }
}

use std::convert::Infallible;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use html5gum::emitters::callback::{CallbackEmitter, CallbackEvent};
use html5gum::{Emitter, Error, Span, State, Tokenizer, naive_next_state};
use rayon::prelude::*;

use crate::cli::Cli;
use crate::input::discover;
use crate::model::{DiagnosticDraft, FileReport, OperationalError, Report, RuleSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TagKind {
    #[default]
    Other,
    Img,
    Html,
    Button,
    Iframe,
}

#[derive(Debug, Default)]
struct OpenTag {
    kind: TagKind,
    start: usize,
    has_alt: bool,
    has_lang: bool,
    has_type: bool,
    has_title: bool,
}

struct Scanner {
    rules: RuleSet,
    max_diagnostics: usize,
    diagnostics: Vec<DiagnosticDraft>,
    truncated: bool,
    current: OpenTag,
    saw_html_doctype: bool,
}

struct CommonEmitter<'a> {
    scanner: &'a mut Scanner,
    position: usize,
    tag_start: usize,
    current_is_start: bool,
    current_tag_name: Vec<u8>,
    last_start_tag: Vec<u8>,
    current_attribute_name: Vec<u8>,
    has_alt: bool,
}

impl<'a> CommonEmitter<'a> {
    fn new(scanner: &'a mut Scanner) -> Self {
        Self {
            scanner,
            position: 0,
            tag_start: 0,
            current_is_start: false,
            current_tag_name: Vec::with_capacity(16),
            last_start_tag: Vec::with_capacity(16),
            current_attribute_name: Vec::with_capacity(16),
            has_alt: false,
        }
    }

    #[inline]
    fn flush_attribute(&mut self) {
        self.has_alt |= self.current_attribute_name == b"alt";
        self.current_attribute_name.clear();
    }
}

impl Emitter for CommonEmitter<'_> {
    type Token = Infallible;

    fn set_last_start_tag(&mut self, last_start_tag: Option<&[u8]>) {
        self.last_start_tag.clear();
        self.last_start_tag
            .extend_from_slice(last_start_tag.unwrap_or_default());
    }

    #[inline]
    fn emit_eof(&mut self) {}

    #[inline]
    fn emit_error(&mut self, _error: Error) {}

    #[inline]
    fn should_emit_errors(&mut self) -> bool {
        false
    }

    #[inline]
    fn pop_token(&mut self) -> Option<Self::Token> {
        None
    }

    #[inline]
    fn emit_string(&mut self, _characters: &[u8]) {}

    #[inline]
    fn init_start_tag(&mut self) {
        self.current_is_start = true;
        self.current_tag_name.clear();
        self.current_attribute_name.clear();
        self.has_alt = false;
    }

    #[inline]
    fn init_end_tag(&mut self) {
        self.current_is_start = false;
        self.current_tag_name.clear();
        self.current_attribute_name.clear();
    }

    #[inline]
    fn init_comment(&mut self) {}

    #[inline]
    fn emit_current_tag(&mut self) -> Option<State> {
        self.flush_attribute();
        if self.current_is_start {
            if self.current_tag_name == b"img" && !self.has_alt {
                self.scanner.push(
                    "img-alt",
                    "error",
                    "Image elements must have an alt attribute".into(),
                    self.tag_start,
                    self.position,
                    Some("Add alt text, or use alt=\"\" for a decorative image."),
                );
            }
            self.last_start_tag.clear();
            std::mem::swap(&mut self.last_start_tag, &mut self.current_tag_name);
            naive_next_state(&self.last_start_tag)
        } else {
            self.last_start_tag.clear();
            None
        }
    }

    #[inline]
    fn emit_current_comment(&mut self) {}

    #[inline]
    fn emit_current_doctype(&mut self) {}

    #[inline]
    fn set_self_closing(&mut self) {}

    #[inline]
    fn set_force_quirks(&mut self) {}

    #[inline]
    fn push_tag_name(&mut self, value: &[u8]) {
        self.current_tag_name.extend_from_slice(value);
    }

    #[inline]
    fn push_comment(&mut self, _value: &[u8]) {}

    #[inline]
    fn push_doctype_name(&mut self, _value: &[u8]) {}

    #[inline]
    fn init_doctype(&mut self) {}

    #[inline]
    fn init_attribute(&mut self) {
        self.flush_attribute();
    }

    #[inline]
    fn push_attribute_name(&mut self, value: &[u8]) {
        self.current_attribute_name.extend_from_slice(value);
    }

    #[inline]
    fn push_attribute_value(&mut self, _value: &[u8]) {}

    #[inline]
    fn set_doctype_public_identifier(&mut self, _value: &[u8]) {}

    #[inline]
    fn set_doctype_system_identifier(&mut self, _value: &[u8]) {}

    #[inline]
    fn push_doctype_public_identifier(&mut self, _value: &[u8]) {}

    #[inline]
    fn push_doctype_system_identifier(&mut self, _value: &[u8]) {}

    #[inline]
    fn start_open_tag(&mut self) {
        self.tag_start = self.position.saturating_sub(1);
    }

    #[inline]
    fn current_is_appropriate_end_tag_token(&mut self) -> bool {
        !self.current_is_start && self.current_tag_name == self.last_start_tag
    }

    #[inline]
    fn move_position(&mut self, offset: isize) {
        self.position = self.position.saturating_add_signed(offset);
    }
}

impl Scanner {
    fn new(rules: RuleSet, max_diagnostics: usize) -> Self {
        Self {
            rules,
            max_diagnostics,
            diagnostics: Vec::with_capacity(max_diagnostics.min(32)),
            truncated: false,
            current: OpenTag::default(),
            saw_html_doctype: false,
        }
    }

    #[inline]
    fn event(&mut self, event: CallbackEvent<'_>, span: Span<usize>) {
        match event {
            CallbackEvent::OpenStartTag { name } => {
                self.current = OpenTag {
                    kind: match name {
                        b"img" => TagKind::Img,
                        b"html" => TagKind::Html,
                        b"button" => TagKind::Button,
                        b"iframe" => TagKind::Iframe,
                        _ => TagKind::Other,
                    },
                    start: span.start,
                    ..OpenTag::default()
                };
            }
            CallbackEvent::AttributeName { name } => match name {
                b"alt" => self.current.has_alt = true,
                b"lang" => self.current.has_lang = true,
                b"type" => self.current.has_type = true,
                b"title" => self.current.has_title = true,
                _ => {}
            },
            CallbackEvent::CloseStartTag { .. } => self.close_tag(span.end),
            CallbackEvent::Doctype { name, .. } => {
                self.saw_html_doctype = name.eq_ignore_ascii_case(b"html");
            }
            CallbackEvent::Error(error) if self.rules.contains(RuleSet::PARSE_ERROR) => {
                self.push(
                    "parse-error",
                    "error",
                    format!("HTML tokenizer error: {error:?}"),
                    span.start,
                    span.end,
                    None,
                );
            }
            _ => {}
        }
    }

    #[inline]
    fn close_tag(&mut self, end: usize) {
        match self.current.kind {
            TagKind::Img if self.rules.contains(RuleSet::IMG_ALT) && !self.current.has_alt => {
                self.push(
                    "img-alt",
                    "error",
                    "Image elements must have an alt attribute".into(),
                    self.current.start,
                    end,
                    Some("Add alt text, or use alt=\"\" for a decorative image."),
                );
            }
            TagKind::Html if self.rules.contains(RuleSet::HTML_LANG) && !self.current.has_lang => {
                self.push(
                    "html-lang",
                    "error",
                    "The html element must have a lang attribute".into(),
                    self.current.start,
                    end,
                    Some("Set lang to the document's primary language."),
                );
            }
            TagKind::Button
                if self.rules.contains(RuleSet::BUTTON_TYPE) && !self.current.has_type =>
            {
                self.push(
                    "button-type",
                    "error",
                    "Button elements must have an explicit type attribute".into(),
                    self.current.start,
                    end,
                    Some("Use type=\"button\", type=\"submit\", or type=\"reset\"."),
                );
            }
            TagKind::Iframe
                if self.rules.contains(RuleSet::IFRAME_TITLE) && !self.current.has_title =>
            {
                self.push(
                    "iframe-title",
                    "error",
                    "Iframe elements must have a title attribute".into(),
                    self.current.start,
                    end,
                    Some("Add a concise title describing the embedded content."),
                );
            }
            _ => {}
        }
    }

    fn finish(&mut self) {
        if self.rules.contains(RuleSet::DOCTYPE_HTML) && !self.saw_html_doctype {
            self.push(
                "doctype-html",
                "error",
                "Document must begin with an HTML doctype".into(),
                0,
                0,
                Some("Add <!doctype html> before the html element."),
            );
        }
    }

    #[inline(never)]
    fn push(
        &mut self,
        rule: &'static str,
        severity: &'static str,
        message: String,
        byte_start: usize,
        byte_end: usize,
        help: Option<&'static str>,
    ) {
        if self.diagnostics.len() < self.max_diagnostics {
            self.diagnostics.push(DiagnosticDraft {
                rule,
                severity,
                message,
                byte_start,
                byte_end,
                help,
            });
        } else {
            self.truncated = true;
        }
    }
}

pub fn lint_bytes(
    path: impl Into<String>,
    input: &[u8],
    rules: RuleSet,
    max_diagnostics: usize,
) -> FileReport {
    let mut scanner = Scanner::new(rules, max_diagnostics);
    if rules == RuleSet::COMMON {
        let emitter = CommonEmitter::new(&mut scanner);
        let _ = Tokenizer::new_with_emitter(input, emitter).finish();
    } else {
        let mut emitter = CallbackEmitter::new(
            |event: CallbackEvent<'_>, span: Span<usize>| -> Option<Infallible> {
                scanner.event(event, span);
                None
            },
        );
        emitter.naively_switch_states(true);
        // A byte slice preserves byte offsets even when the input is not valid UTF-8.
        let _ = Tokenizer::new_with_emitter(input, emitter).finish();
    }
    scanner.finish();
    scanner
        .diagnostics
        .sort_unstable_by_key(|diagnostic| diagnostic.byte_start);
    let mut line_starts = vec![0];
    if !scanner.diagnostics.is_empty() {
        line_starts.extend(
            input
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        );
    }
    let diagnostics = scanner
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.resolve(input.len(), &line_starts))
        .collect();

    FileReport {
        path: path.into(),
        bytes: input.len() as u64,
        diagnostics,
        truncated: scanner.truncated,
    }
}

pub fn run(cli: &Cli) -> Report {
    let (paths, discovery_errors) = discover(&cli.paths);
    let mut errors: Vec<OperationalError> = discovery_errors
        .into_iter()
        .map(|(path, message)| OperationalError {
            path: Some(display_path(&path)),
            message,
        })
        .collect();

    if paths.is_empty() {
        if errors.is_empty() {
            errors.push(OperationalError {
                path: None,
                message: "no HTML files found".into(),
            });
        }
        return Report::from_parts(Vec::new(), errors);
    }

    let process = |path: &PathBuf| process_path(path, cli);
    let results = if paths.len() == 1 || cli.threads == Some(1) {
        paths.iter().map(process).collect()
    } else if let Some(threads) = cli.threads {
        match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
            Ok(pool) => pool.install(|| paths.par_iter().map(process).collect()),
            Err(error) => {
                errors.push(OperationalError {
                    path: None,
                    message: format!("could not create worker pool: {error}"),
                });
                Vec::new()
            }
        }
    } else {
        paths.par_iter().map(process).collect()
    };

    let mut files = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(file) => files.push(file),
            Err(error) => errors.push(error),
        }
    }
    files.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Report::from_parts(files, errors)
}

fn process_path(path: &Path, cli: &Cli) -> Result<FileReport, OperationalError> {
    if path.as_os_str() == "-" {
        let mut input = Vec::new();
        io::stdin()
            .read_to_end(&mut input)
            .map_err(|error| OperationalError {
                path: Some(cli.stdin_filename.clone()),
                message: error.to_string(),
            })?;
        return Ok(lint_bytes(
            cli.stdin_filename.clone(),
            &input,
            cli.rules,
            cli.max_diagnostics,
        ));
    }

    let input = fs::read(path).map_err(|error| OperationalError {
        path: Some(display_path(path)),
        message: error.to_string(),
    })?;
    Ok(lint_bytes(
        display_path(path),
        &input,
        cli.rules,
        cli.max_diagnostics,
    ))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_exact_missing_alt_span() {
        let html = b"<!doctype html>\n<html lang=en><img src=x></html>";
        let report = lint_bytes("test.html", html, RuleSet::COMMON, 100);
        assert_eq!(report.diagnostics.len(), 1);
        let diagnostic = &report.diagnostics[0];
        assert_eq!(diagnostic.rule, "img-alt");
        assert_eq!(
            &html[diagnostic.byte_start..diagnostic.byte_end],
            b"<img src=x>"
        );
        assert_eq!((diagnostic.line, diagnostic.column), (2, 15));
    }

    #[test]
    fn clean_common_profile_has_no_findings() {
        let html = b"<!doctype html><html lang=en><img src=x alt='x'></html>";
        let report = lint_bytes("test.html", html, RuleSet::COMMON, 100);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn common_profile_respects_script_data_state() {
        let html = br#"<!doctype html><html lang=en><script>const template = "<img src=x>";</script><img src=x alt=x></html>"#;
        let report = lint_bytes("test.html", html, RuleSet::COMMON, 100);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn common_profile_normalizes_ascii_case() {
        let html = b"<!doctype html><HTML lang=en><IMG src=x ALT=x></HTML>";
        let report = lint_bytes("test.html", html, RuleSet::COMMON, 100);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn all_rules_include_tokenizer_errors() {
        let html = b"<!doctype html><html lang=en>\0</html>";
        let report = lint_bytes("test.html", html, RuleSet::ALL, 100);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule == "parse-error")
        );
    }

    #[test]
    fn all_rules_find_missing_attributes_and_doctype() {
        let html = b"<html><button>Save</button><iframe></iframe><img>";
        let report = lint_bytes("test.html", html, RuleSet::ALL, 100);
        let rules: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule)
            .collect();
        assert!(rules.contains(&"doctype-html"));
        assert!(rules.contains(&"html-lang"));
        assert!(rules.contains(&"button-type"));
        assert!(rules.contains(&"iframe-title"));
        assert!(rules.contains(&"img-alt"));
    }

    #[test]
    fn diagnostic_cap_is_explicit() {
        let report = lint_bytes("test.html", b"<img><img><img>", RuleSet::COMMON, 2);
        assert_eq!(report.diagnostics.len(), 2);
        assert!(report.truncated);
    }
}

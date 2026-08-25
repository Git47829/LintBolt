use std::io::{self, Write};
use std::process::ExitCode;

use agent_html_lint::cli::HELP;
use agent_html_lint::{Cli, CliAction, Report, run};
use serde::Serialize;

#[derive(Serialize)]
struct Information<'a> {
    schema_version: u8,
    status: &'static str,
    kind: &'static str,
    value: &'a str,
}

fn main() -> ExitCode {
    let (report, explicit_exit) = match Cli::parse(std::env::args()) {
        Ok(CliAction::Run(cli)) => {
            let report = run(&cli);
            let exit = report.exit_code();
            (Output::Report(report), exit)
        }
        Ok(CliAction::Help) => (
            Output::Information(Information {
                schema_version: 1,
                status: "ok",
                kind: "usage",
                value: HELP,
            }),
            0,
        ),
        Ok(CliAction::Version) => (
            Output::Information(Information {
                schema_version: 1,
                status: "ok",
                kind: "version",
                value: env!("CARGO_PKG_VERSION"),
            }),
            0,
        ),
        Err(error) => (Output::Report(Report::operational(error)), 2),
    };

    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    if serde_json::to_writer(&mut writer, &report).is_err() || writer.write_all(b"\n").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::from(explicit_exit as u8)
}

#[derive(Serialize)]
#[serde(untagged)]
enum Output<'a> {
    Report(Report),
    Information(Information<'a>),
}

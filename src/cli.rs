use std::path::PathBuf;

use crate::model::RuleSet;

#[derive(Debug)]
pub struct Cli {
    pub paths: Vec<PathBuf>,
    pub stdin_filename: String,
    pub threads: Option<usize>,
    pub max_diagnostics: usize,
    pub rules: RuleSet,
}

#[derive(Debug)]
pub enum CliAction {
    Run(Cli),
    Help,
    Version,
}

impl Cli {
    pub fn parse<I>(args: I) -> Result<CliAction, String>
    where
        I: IntoIterator<Item = String>,
    {
        let mut args = args.into_iter();
        let _program = args.next();
        let mut paths = Vec::new();
        let mut stdin_filename = "<stdin>".to_owned();
        let mut threads = None;
        let mut max_diagnostics = 1_000;
        let mut rules = RuleSet::ALL;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => return Ok(CliAction::Help),
                "--version" | "-V" => return Ok(CliAction::Version),
                "--stdin-filename" => {
                    stdin_filename = next_value(&mut args, "--stdin-filename")?;
                }
                "--threads" | "-j" => {
                    let value = next_value(&mut args, "--threads")?;
                    let parsed = value
                        .parse::<usize>()
                        .map_err(|_| "--threads requires a positive integer".to_owned())?;
                    if parsed == 0 {
                        return Err("--threads requires a positive integer".into());
                    }
                    threads = Some(parsed);
                }
                "--max-diagnostics" => {
                    let value = next_value(&mut args, "--max-diagnostics")?;
                    max_diagnostics = value.parse::<usize>().map_err(|_| {
                        "--max-diagnostics requires a non-negative integer".to_owned()
                    })?;
                }
                "--rules" => {
                    rules = RuleSet::parse(&next_value(&mut args, "--rules")?)?;
                }
                "--" => {
                    paths.extend(args.map(PathBuf::from));
                    break;
                }
                "-" => paths.push(PathBuf::from("-")),
                value if value.starts_with('-') => {
                    return Err(format!("unknown option: {value}"));
                }
                value => paths.push(PathBuf::from(value)),
            }
        }

        if paths.is_empty() {
            paths.push(PathBuf::from("-"));
        }
        if paths.iter().filter(|path| path.as_os_str() == "-").count() > 1 {
            return Err("stdin may be specified only once".into());
        }

        Ok(CliAction::Run(Self {
            paths,
            stdin_filename,
            threads,
            max_diagnostics,
            rules,
        }))
    }
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_stdin() {
        let CliAction::Run(cli) = Cli::parse(["html-lint".into()]).unwrap() else {
            panic!("expected run action")
        };
        assert_eq!(cli.paths, [PathBuf::from("-")]);
    }

    #[test]
    fn parses_agent_options() {
        let args = [
            "html-lint",
            "--rules",
            "common",
            "--threads",
            "3",
            "page.html",
        ]
        .map(str::to_owned);
        let CliAction::Run(cli) = Cli::parse(args).unwrap() else {
            panic!("expected run action")
        };
        assert_eq!(cli.threads, Some(3));
        assert_eq!(cli.rules, RuleSet::COMMON);
    }
}

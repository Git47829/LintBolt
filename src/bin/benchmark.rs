use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct Config {
    tools: Vec<Tool>,
    fixtures: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
struct Tool {
    name: String,
    version: String,
    command: Vec<String>,
    accepted_exit_codes: Vec<i32>,
    #[serde(default)]
    enabled: bool,
    qualified_findings: [usize; 3],
    #[serde(default)]
    max_benchmark_bytes: Option<u64>,
    #[serde(default)]
    exclusion_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    path: String,
    warmups: usize,
    runs: usize,
    expected_findings: usize,
}

#[derive(Debug, Serialize)]
struct Evidence {
    schema_version: u8,
    generated_unix_ms: u128,
    machine: BTreeMap<String, String>,
    tools: Vec<ToolEvidence>,
    results: Vec<ResultRow>,
}

#[derive(Debug, Serialize)]
struct ToolEvidence {
    name: String,
    version: String,
    qualified_findings: [usize; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    max_benchmark_bytes: Option<u64>,
}

#[derive(Debug, Serialize)]
struct ResultRow {
    tool: String,
    fixture: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    exclusion_reason: Option<String>,
    bytes: u64,
    expected_findings: usize,
    runs: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    median_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    p95_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    min_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_ms: Option<f64>,
    samples_ms: Vec<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().collect();
    let config_path = arguments
        .get(1)
        .cloned()
        .unwrap_or_else(|| "benchmarks/tools.json".into());
    let smoke = arguments.iter().any(|argument| argument == "--smoke");
    let config: Config = serde_json::from_slice(&fs::read(&config_path)?)?;
    let mut results = Vec::new();
    let enabled: Vec<_> = config.tools.iter().filter(|tool| tool.enabled).collect();

    for (fixture_index, fixture) in config.fixtures.iter().enumerate() {
        let bytes = fs::metadata(&fixture.path)?.len();
        for tool in enabled.iter().filter(|tool| {
            tool.max_benchmark_bytes
                .is_some_and(|maximum| bytes > maximum)
        }) {
            results.push(ResultRow {
                tool: tool.name.clone(),
                fixture: fixture.name.clone(),
                status: "excluded",
                exclusion_reason: tool.exclusion_reason.clone(),
                bytes,
                expected_findings: fixture.expected_findings,
                runs: 0,
                median_ms: None,
                p95_ms: None,
                min_ms: None,
                max_ms: None,
                samples_ms: Vec::new(),
            });
        }
        let active: Vec<_> = enabled
            .iter()
            .copied()
            .filter(|tool| {
                tool.max_benchmark_bytes
                    .is_none_or(|maximum| bytes <= maximum)
            })
            .collect();
        let warmups = if smoke { 0 } else { fixture.warmups };
        let runs = if smoke { 1 } else { fixture.runs };
        let mut samples = vec![Vec::with_capacity(runs); active.len()];

        for round in 0..warmups {
            for tool_index in shuffled_indices(
                active.len(),
                0x9e37_79b9 ^ fixture_index as u64 ^ round as u64,
            ) {
                run_once(active[tool_index], fixture)?;
            }
        }
        for round in 0..runs {
            eprintln!(
                "benchmarking {}: round {}/{}",
                fixture.name,
                round + 1,
                runs
            );
            for tool_index in shuffled_indices(
                active.len(),
                0xd1b5_4a32_d192_ed03 ^ fixture_index as u64 ^ round as u64,
            ) {
                samples[tool_index].push(run_once(active[tool_index], fixture)?);
            }
        }

        for (tool, samples) in active.iter().zip(samples) {
            let mut sorted = samples.clone();
            sorted.sort_unstable_by(Duration::cmp);
            results.push(ResultRow {
                tool: tool.name.clone(),
                fixture: fixture.name.clone(),
                status: "measured",
                exclusion_reason: None,
                bytes,
                expected_findings: fixture.expected_findings,
                runs,
                median_ms: Some(millis(percentile(&sorted, 0.50))),
                p95_ms: Some(millis(percentile(&sorted, 0.95))),
                min_ms: Some(millis(sorted[0])),
                max_ms: Some(millis(*sorted.last().expect("non-empty samples"))),
                samples_ms: samples.into_iter().map(millis).collect(),
            });
        }
    }

    let evidence = Evidence {
        schema_version: 1,
        generated_unix_ms: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis(),
        machine: machine_metadata(),
        tools: enabled
            .iter()
            .map(|tool| ToolEvidence {
                name: tool.name.clone(),
                version: tool.version.clone(),
                qualified_findings: tool.qualified_findings,
                max_benchmark_bytes: tool.max_benchmark_bytes,
            })
            .collect(),
        results,
    };
    fs::create_dir_all("benchmarks/results")?;
    let output = serde_json::to_vec_pretty(&evidence)?;
    fs::write("benchmarks/results/latest.json", &output)?;
    println!("{}", String::from_utf8(output)?);
    Ok(())
}

fn shuffled_indices(length: usize, mut state: u64) -> Vec<usize> {
    let mut values: Vec<_> = (0..length).collect();
    for index in (1..values.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        values.swap(index, state as usize % (index + 1));
    }
    values
}

fn run_once(tool: &Tool, fixture: &Fixture) -> Result<Duration, Box<dyn std::error::Error>> {
    let (program, arguments) = tool.command.split_first().ok_or("empty tool command")?;
    let arguments: Vec<_> = arguments
        .iter()
        .map(|argument| argument.replace("{file}", &fixture.path))
        .collect();
    let started = Instant::now();
    let status = Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    let elapsed = started.elapsed();
    let code = status.code().unwrap_or(-1);
    if !tool.accepted_exit_codes.contains(&code) {
        return Err(format!(
            "{} failed on {} with exit code {}",
            tool.name, fixture.name, code
        )
        .into());
    }
    Ok(elapsed)
}

fn percentile(sorted: &[Duration], percentile: f64) -> Duration {
    assert!(!sorted.is_empty());
    let rank = ((sorted.len() - 1) as f64 * percentile).ceil() as usize;
    sorted[rank]
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn machine_metadata() -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("os".into(), std::env::consts::OS.into());
    metadata.insert("arch".into(), std::env::consts::ARCH.into());
    metadata.insert(
        "available_parallelism".into(),
        std::thread::available_parallelism()
            .map(|count| count.get().to_string())
            .unwrap_or_else(|_| "unknown".into()),
    );
    if let Some(output) = command_text("rustc", &["--version"]) {
        metadata.insert("rustc".into(), output);
    }
    if Path::new("Cargo.lock").exists() {
        metadata.insert("cargo_lock".into(), "present".into());
    }
    metadata
}

fn command_text(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

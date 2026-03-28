use crate::error::Result;
use crate::skill::Skill;
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillTestCase {
    pub name: String,
    pub input: String,
    #[serde(default)]
    pub expected_contains: Vec<String>,
    #[serde(default)]
    pub expected_not_contains: Vec<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillTestSuite {
    pub skill_name: String,
    pub cases: Vec<SkillTestCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestResult {
    pub case_name: String,
    pub passed: bool,
    pub actual_output: String,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalResult {
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResult>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub pass_rate: f64,
    pub avg_duration_ms: u64,
    pub min_duration_ms: u64,
    pub max_duration_ms: u64,
    pub run_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillAction {
    Eval(String),
    Improve(String),
    Benchmark { name: String, runs: usize },
}

pub fn parse_skill_action(input: &str) -> Option<SkillAction> {
    let trimmed = input.trim();
    if let Some(name) = trimmed.strip_prefix("/skill eval ") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(SkillAction::Eval(name.to_string()));
        }
    }

    if let Some(name) = trimmed.strip_prefix("/skill improve ") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(SkillAction::Improve(name.to_string()));
        }
    }

    if let Some(rest) = trimmed.strip_prefix("/skill benchmark ") {
        let mut parts = rest.split_whitespace();
        let name = parts.next()?.to_string();
        let runs = parts
            .next()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(3);
        return Some(SkillAction::Benchmark {
            name,
            runs: runs.max(1),
        });
    }

    None
}

pub fn eval_skill<F>(skill: &Skill, cases: &[SkillTestCase], executor: F) -> EvalResult
where
    F: Fn(&Skill, &str, u64) -> Result<String>,
{
    let start = Instant::now();
    let results = cases
        .iter()
        .map(|case| {
            let output = match executor(skill, &case.input, case.timeout_secs) {
                Ok(output) => output,
                Err(error) => {
                    return TestResult {
                        case_name: case.name.clone(),
                        passed: false,
                        actual_output: String::new(),
                        failure_reason: Some(error.to_string()),
                    };
                }
            };

            let normalized_output = output.to_lowercase();
            for expected in &case.expected_contains {
                if !normalized_output.contains(&expected.to_lowercase()) {
                    return TestResult {
                        case_name: case.name.clone(),
                        passed: false,
                        actual_output: output,
                        failure_reason: Some(format!(
                            "expected_contains missing: {expected}"
                        )),
                    };
                }
            }

            for forbidden in &case.expected_not_contains {
                if normalized_output.contains(&forbidden.to_lowercase()) {
                    return TestResult {
                        case_name: case.name.clone(),
                        passed: false,
                        actual_output: output,
                        failure_reason: Some(format!(
                            "expected_not_contains matched: {forbidden}"
                        )),
                    };
                }
            }

            TestResult {
                case_name: case.name.clone(),
                passed: true,
                actual_output: output,
                failure_reason: None,
            }
        })
        .collect::<Vec<_>>();

    let passed = results.iter().filter(|result| result.passed).count();
    let failed = results.len().saturating_sub(passed);

    EvalResult {
        passed,
        failed,
        results,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

pub fn benchmark_skill<F>(
    skill: &Skill,
    cases: &[SkillTestCase],
    runs: usize,
    executor: F,
) -> BenchmarkResult
where
    F: Fn(&Skill, &str, u64) -> Result<String> + Copy,
{
    let run_count = runs.max(1);
    let mut durations = Vec::with_capacity(run_count);
    let mut passed_runs = 0usize;

    for _ in 0..run_count {
        let result = eval_skill(skill, cases, executor);
        if result.failed == 0 {
            passed_runs += 1;
        }
        durations.push(result.duration_ms);
    }

    let min_duration_ms = *durations.iter().min().unwrap_or(&0);
    let max_duration_ms = *durations.iter().max().unwrap_or(&0);
    let avg_duration_ms = durations.iter().sum::<u64>() / run_count as u64;

    BenchmarkResult {
        pass_rate: passed_runs as f64 / run_count as f64,
        avg_duration_ms,
        min_duration_ms,
        max_duration_ms,
        run_count,
    }
}

fn default_timeout_secs() -> u64 {
    30
}

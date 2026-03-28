use crate::events::{EventSeverity, SystemEvent};
use crate::mode::ExecMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Ignore,
    Log,
    AutoFix { action: String },
    Report { message: String },
    Escalate { context: String, question: String },
}

pub fn decide(event: &SystemEvent, severity: &EventSeverity, exec_mode: &ExecMode) -> Decision {
    match severity {
        EventSeverity::Info => Decision::Log,
        EventSeverity::Warning => match exec_mode {
            ExecMode::Trust => {
                if let Some(action) = is_auto_fixable(event) {
                    Decision::AutoFix { action }
                } else {
                    Decision::Report {
                        message: format!("Warning detected: {}", describe_event(event)),
                    }
                }
            }
            ExecMode::Auto => Decision::Report {
                message: format!("Warning detected: {}", describe_event(event)),
            },
            ExecMode::Safe => Decision::Report {
                message: format!(
                    "Warning detected and confirmation is required: {}",
                    describe_event(event)
                ),
            },
        },
        EventSeverity::Critical => Decision::Escalate {
            context: describe_event(event),
            question: "What is the safest next action?".to_string(),
        },
    }
}

pub fn is_auto_fixable(event: &SystemEvent) -> Option<String> {
    match event {
        SystemEvent::TestFailed { error, .. } => {
            let normalized = error.to_lowercase();
            if normalized.contains("unused import") || normalized.contains("missing semicolon") {
                return Some("cargo fix --allow-dirty".to_string());
            }
            if normalized.contains("formatting") {
                return Some("cargo fmt".to_string());
            }
            None
        }
        SystemEvent::FileChanged { path, .. } if path.ends_with(".rs") => {
            Some("cargo fmt".to_string())
        }
        _ => None,
    }
}

pub fn describe_event(event: &SystemEvent) -> String {
    match event {
        SystemEvent::FileChanged { path, change_type } => {
            format!("File changed: {path} ({change_type:?})")
        }
        SystemEvent::TestFailed { test_name, error } => {
            format!("Test failed: {test_name} ({error})")
        }
        SystemEvent::TestPassed { test_name } => format!("Test passed: {test_name}"),
        SystemEvent::HighMemoryUsage { percent } => {
            format!("High memory usage detected: {percent:.2}%")
        }
        SystemEvent::HighDiskUsage { percent } => {
            format!("High disk usage detected: {percent:.2}%")
        }
        SystemEvent::GitConflict { branch } => format!("Git conflict detected on {branch}"),
        SystemEvent::CronTrigger { schedule_name } => {
            format!("Cron trigger fired: {schedule_name}")
        }
        SystemEvent::SkillFailed { skill_name, error } => {
            format!("Skill failed: {skill_name} ({error})")
        }
        SystemEvent::LongIdle { minutes } => format!("User has been idle for {minutes} minutes"),
    }
}

#[cfg(test)]
mod tests {
    use super::{Decision, decide, is_auto_fixable};
    use crate::events::{EventSeverity, SystemEvent};
    use crate::mode::ExecMode;

    #[test]
    fn decision_matrix_matches_modes_and_severity() {
        let warning = SystemEvent::TestFailed {
            test_name: "unit".to_string(),
            error: "unused import".to_string(),
        };
        let critical = SystemEvent::GitConflict {
            branch: "main".to_string(),
        };

        assert_eq!(
            decide(&warning, &EventSeverity::Info, &ExecMode::Safe),
            Decision::Log
        );
        assert!(matches!(
            decide(&warning, &EventSeverity::Warning, &ExecMode::Trust),
            Decision::AutoFix { .. }
        ));
        assert!(matches!(
            decide(&warning, &EventSeverity::Warning, &ExecMode::Auto),
            Decision::Report { .. }
        ));
        assert!(matches!(
            decide(&critical, &EventSeverity::Critical, &ExecMode::Safe),
            Decision::Escalate { .. }
        ));
    }

    #[test]
    fn auto_fixable_returns_known_fix_commands() {
        let simple_failure = SystemEvent::TestFailed {
            test_name: "unit".to_string(),
            error: "unused import in src/main.rs".to_string(),
        };
        let complex_failure = SystemEvent::TestFailed {
            test_name: "unit".to_string(),
            error: "type mismatch across multiple trait bounds".to_string(),
        };

        assert_eq!(
            is_auto_fixable(&simple_failure),
            Some("cargo fix --allow-dirty".to_string())
        );
        assert_eq!(is_auto_fixable(&complex_failure), None);
    }
}

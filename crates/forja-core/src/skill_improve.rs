use crate::skill::Skill;
use crate::skill_eval::EvalResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    pub target: SuggestionTarget,
    pub description: String,
    pub priority: SuggestionPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionTarget {
    Prompt,
    Script,
    Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionPriority {
    High,
    Medium,
    Low,
}

pub fn suggest_improvements(_skill: &Skill, eval_result: &EvalResult) -> Vec<Suggestion> {
    let mut suggestions = Vec::new();

    if eval_result.failed == 0 {
        return suggestions;
    }

    if eval_result
        .results
        .iter()
        .filter_map(|result| result.failure_reason.as_deref())
        .any(|reason| reason.contains("timeout"))
    {
        suggestions.push(Suggestion {
            target: SuggestionTarget::Config,
            description: "Optimize script performance or increase timeout.".to_string(),
            priority: SuggestionPriority::High,
        });
    }

    if eval_result
        .results
        .iter()
        .filter_map(|result| result.failure_reason.as_deref())
        .any(|reason| reason.contains("expected_contains"))
    {
        suggestions.push(Suggestion {
            target: SuggestionTarget::Script,
            description: "Script output is missing expected keywords, check script logic.".to_string(),
            priority: SuggestionPriority::Medium,
        });
    }

    if eval_result
        .results
        .iter()
        .filter_map(|result| result.failure_reason.as_deref())
        .any(|reason| reason.contains("expected_not_contains"))
    {
        suggestions.push(Suggestion {
            target: SuggestionTarget::Script,
            description: "Script is producing error output, add error handling.".to_string(),
            priority: SuggestionPriority::High,
        });
    }

    suggestions
}

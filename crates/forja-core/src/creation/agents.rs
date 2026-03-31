use super::DebateAgent;

pub fn default_debate_agents() -> Vec<DebateAgent> {
    vec![
        DebateAgent {
            id: "architect".to_string(),
            role: "Architect".to_string(),
            framework: "Break every proposal into no more than 3 components. Reject if complex.".to_string(),
            budget: 5_000,
        },
        DebateAgent {
            id: "critic".to_string(),
            role: "Critique".to_string(),
            framework: "Find falsifiable flaws in every claim. Estimate failure probability.".to_string(),
            budget: 5_000,
        },
        DebateAgent {
            id: "builder".to_string(),
            role: "Build".to_string(),
            framework: "If it cannot be implemented within 48 hours, propose alternative.".to_string(),
            budget: 5_000,
        },
        DebateAgent {
            id: "researcher".to_string(),
            role: "Research".to_string(),
            framework: "Ignore claims without sources. Judge only from data.".to_string(),
            budget: 5_000,
        },
        DebateAgent {
            id: "synthesizer".to_string(),
            role: "Synthesis".to_string(),
            framework: "Summarize in 3 sentences. Convert to executable task list.".to_string(),
            budget: 5_000,
        },
    ]
}

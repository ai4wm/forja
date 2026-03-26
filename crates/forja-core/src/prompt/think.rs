use crate::mode::ThinkLevel;

pub fn think_prompt(level: ThinkLevel) -> &'static str {
    match level {
        ThinkLevel::Min => "Be concise. Answer in 1-3 sentences maximum. Skip all explanations unless explicitly asked. Answer directly and immediately.",
        ThinkLevel::Mid => "",
        ThinkLevel::Max => "Think extremely thoroughly before responding.\n1. Restate the problem in your own words to confirm understanding.\n2. List all assumptions and verify each one.\n3. Generate at least 3 different approaches.\n4. For each approach, analyze pros, cons, risks, and edge cases.\n5. Select the best approach and explain why.\n6. Implement step by step with detailed reasoning.\n7. After completing, review your own answer for errors or missed cases.\n8. Provide a confidence level (1-10) and list remaining uncertainties.\nThoroughness is more important than brevity.",
    }
}

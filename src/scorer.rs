use crate::db::{Message, Session};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Scored result for a single session across 7 dimensions.
/// Max per dimension: security=15, effectivity=15, solidity=10,
/// efficiency=15, planning_quality=15, recovery_ability=15, hallucination_rate=15.
/// Total max = 100.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    pub session_id: String,
    pub total_score: i64,
    pub security: i64,
    pub effectivity: i64,
    pub solidity: i64,
    pub efficiency: i64,
    pub planning_quality: i64,
    pub recovery_ability: i64,
    pub hallucination_rate: i64,
    pub grade: String,
    pub scored_at: String,
}

/// Returns true if the session has enough substance to score.
/// Trivial sessions (single commands, empty exchanges) return false → N/A.
fn is_scorable(messages: &[Message]) -> bool {
    let substantial_user = messages
        .iter()
        .filter(|m| m.role == "user" && m.content.trim().len() > 15)
        .count();
    let substantial_assistant = messages
        .iter()
        .filter(|m| m.role == "assistant" && m.content.trim().len() > 50)
        .count();
    substantial_user >= 2 && substantial_assistant >= 2
}

/// Score a session using heuristic rules. Returns None for trivial/empty sessions.
pub fn score_session(session: &Session, messages: &[Message]) -> Option<Score> {
    if !is_scorable(messages) {
        return None;
    }

    let security = score_security(messages);
    let effectivity = score_effectivity(messages);
    let solidity = score_solidity(messages);
    let efficiency = score_efficiency(session, messages);
    let planning_quality = score_planning_quality(messages);
    let recovery_ability = score_recovery_ability(messages);
    let hallucination_rate = score_hallucination_rate(messages);

    let total = security
        + effectivity
        + solidity
        + efficiency
        + planning_quality
        + recovery_ability
        + hallucination_rate;
    let grade = grade_for(total).to_string();

    Some(Score {
        session_id: session.id.clone(),
        total_score: total,
        security,
        effectivity,
        solidity,
        efficiency,
        planning_quality,
        recovery_ability,
        hallucination_rate,
        grade,
        scored_at: Utc::now().to_rfc3339(),
    })
}

fn grade_for(score: i64) -> &'static str {
    match score {
        85..=100 => "S",
        75..=84  => "A",
        65..=74  => "B",
        55..=64  => "C",
        45..=54  => "D",
        _        => "F",
    }
}

/// Security (0–15): start at 15, deduct 3 per dangerous pattern found.
fn score_security(messages: &[Message]) -> i64 {
    static DANGEROUS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = DANGEROUS.get_or_init(|| {
        [
            r"rm\s+-[rf]{1,2}\s+[/~]",
            r"sudo\s+rm\s+-[rf]",
            r"chmod\s+[0-7]*7[0-7]*7",
            r"(?:curl|wget)[^\n|]*\|\s*(?:bash|sh|zsh)",
            r"(?i)DROP\s+(?:TABLE|DATABASE)",
            r"(?i)TRUNCATE\s+TABLE",
        ]
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
    });

    let mut score = 15i64;
    for msg in messages {
        for re in patterns {
            if re.is_match(&msg.content) {
                score -= 3;
            }
        }
    }
    score.max(0)
}

/// Effectivity (0–15): baseline 8; +bonus for completion signals; –penalty for failure loops.
fn score_effectivity(messages: &[Message]) -> i64 {
    static FAILURE: OnceLock<Regex> = OnceLock::new();
    static SUCCESS: OnceLock<Regex> = OnceLock::new();

    let fail_re = FAILURE.get_or_init(|| {
        Regex::new(
            r"(?i)doesn'?t work|not working|still (?:not|fails?|broken)|compile error|build fail(?:ed)?|try again|that'?s not right|still broken|failed again|wrong (?:output|result|answer)",
        )
        .unwrap()
    });
    let success_re = SUCCESS.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:done|fixed|working now|completed?|success(?:ful(?:ly)?)?|all tests pass(?:ed)?|build succeed|✓|✅|solved|deployed)\b",
        )
        .unwrap()
    });

    let mut score = 8i64;

    // Penalise failure signals from the user (each occurrence –2, capped at –8).
    let user_failures: usize = messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| fail_re.find_iter(&m.content).count())
        .sum();
    score -= (user_failures as i64 * 2).min(8);

    // Reward completion signals in the last two assistant messages (+3 each).
    let bonus: i64 = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .rev()
        .take(2)
        .map(|m| if success_re.is_match(&m.content) { 3 } else { 0 })
        .sum();
    score += bonus;

    score.max(0).min(15)
}

/// Solidity (0–10): test execution > test reference > code blocks > non-coding neutral.
fn score_solidity(messages: &[Message]) -> i64 {
    static TEST_EXEC: OnceLock<Regex> = OnceLock::new();
    static TEST_REF: OnceLock<Regex> = OnceLock::new();
    static CODE_SIGNAL: OnceLock<Regex> = OnceLock::new();

    let exec_re = TEST_EXEC.get_or_init(|| {
        Regex::new(r"(?i)cargo\s+test|npm\s+test|yarn\s+test|pytest|go\s+test|jest|mocha|rspec|dotnet\s+test")
            .unwrap()
    });
    let ref_re = TEST_REF.get_or_init(|| {
        Regex::new(r"(?i)\btest(?:s|ing)?\b|\bassert(?:_eq|_ne|ions?)?\b|\.test\.|_test\b|\.spec\.|#\[test\]|@Test\b")
            .unwrap()
    });
    // Code blocks written or file-edit tools used → coding session
    let code_re = CODE_SIGNAL.get_or_init(|| {
        Regex::new(r"```[\s\S]{30,}```|\[tool: (?:Write|Edit|NotebookEdit)\]").unwrap()
    });

    let all: String = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    if exec_re.is_match(&all) {
        10
    } else if ref_re.is_match(&all) {
        7
    } else if code_re.is_match(&all) {
        4
    } else {
        // Non-coding session (shell commands, git, etc.): neutral score.
        5
    }
}

/// Efficiency (0–15): baseline 10; penalise correction loops and token bloat.
fn score_efficiency(session: &Session, messages: &[Message]) -> i64 {
    static CORRECTION: OnceLock<Regex> = OnceLock::new();
    let corr_re = CORRECTION.get_or_init(|| {
        Regex::new(
            r"(?i)doesn'?t work|not working|still (?:not|fails?|broken)|try again|wrong (?:output|result)|that'?s not right|failed again",
        )
        .unwrap()
    });

    if session.message_count == 0 {
        return 0;
    }

    let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == "user").collect();
    let correction_loops: i64 = user_msgs
        .iter()
        .filter(|m| corr_re.is_match(&m.content))
        .count() as i64;

    let mut score = 10i64;

    // Penalise correction loops (wasted effort).
    score -= (correction_loops * 3).min(8);

    // Token efficiency signal.
    if session.token_count > 0 {
        let avg_tpm = session.token_count / session.message_count;
        if avg_tpm > 5000 {
            score -= 2;
        } else if avg_tpm < 500 {
            score += 1;
        }
    }

    // Reward short, clean sessions.
    if session.message_count <= 6 && correction_loops == 0 {
        score += 3;
    }

    score.max(0).min(15)
}

/// Planning Quality (0–15): requires structured, explicit planning language.
/// Baseline 5; "let me" alone does NOT qualify.
fn score_planning_quality(messages: &[Message]) -> i64 {
    static STRONG: OnceLock<Regex> = OnceLock::new();
    static MODERATE: OnceLock<Regex> = OnceLock::new();

    // Numbered steps, explicit "plan:" / "approach:" headers.
    let strong_re = STRONG.get_or_init(|| {
        Regex::new(
            r"(?i)(?:step [123][:.\)]|\b[123]\.\s+\w|here'?s (?:the )?plan:|^plan:|^approach:|^strategy:|\bI'?ll:\s*\n)",
        )
        .unwrap()
    });
    // Sequential connectors spanning the plan ("first … then … finally").
    let moderate_re = MODERATE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:first[,\s]+I'?(?:ll|will).{5,}(?:then|next)|then[,\s]+I'?(?:ll|will)|after (?:that|which)[,\s]+I'?(?:ll|will)|finally[,\s]+I'?(?:ll|will))",
        )
        .unwrap()
    });

    let strong_hits: usize = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .take(2)
        .map(|m| strong_re.find_iter(&m.content).count())
        .sum();

    let moderate_hits: usize = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .take(2)
        .map(|m| moderate_re.find_iter(&m.content).count())
        .sum();

    if strong_hits >= 2 {
        15
    } else if strong_hits == 1 {
        12
    } else if moderate_hits >= 2 {
        9
    } else if moderate_hits == 1 {
        7
    } else {
        5
    }
}

/// Recovery Ability (0–15): measures how well errors are addressed.
/// No errors → neutral 10 (not 15). Good recovery → up to 15.
fn score_recovery_ability(messages: &[Message]) -> i64 {
    static ERRORS: OnceLock<Regex> = OnceLock::new();
    static RECOVERY: OnceLock<Regex> = OnceLock::new();

    let err_re = ERRORS.get_or_init(|| {
        Regex::new(
            r"(?i)\berror\b|\bexception\b|\bfailed\b|\bpanic\b|traceback|undefined|null pointer|segfault|\bcrash\b",
        )
        .unwrap()
    });
    let rec_re = RECOVERY.get_or_init(|| {
        Regex::new(
            r"(?i)\bfixed\b|\bresolved\b|now works|should work|\bcorrected\b|let me try|instead[,\s]|alternatively|the (?:issue|problem|root cause) was|I (?:found|identified) the (?:issue|bug|problem)",
        )
        .unwrap()
    });

    let errors: usize = messages
        .iter()
        .map(|m| err_re.find_iter(&m.content).count())
        .sum();

    if errors == 0 {
        // No errors encountered — neutral, not perfect (could be trivial or clean).
        return 10;
    }

    let recoveries: usize = messages
        .iter()
        .map(|m| rec_re.find_iter(&m.content).count())
        .sum();

    let ratio = recoveries as f64 / errors as f64;

    if ratio >= 1.5 {
        15
    } else if ratio >= 1.0 {
        12
    } else if ratio >= 0.5 {
        8
    } else if ratio >= 0.25 {
        5
    } else {
        2
    }
}

/// Interaction Quality / Hallucination Rate (0–15): baseline 10.
/// Satisfaction signals push up; correction and confusion signals push down.
fn score_hallucination_rate(messages: &[Message]) -> i64 {
    static CORRECTION: OnceLock<Regex> = OnceLock::new();
    static SATISFACTION: OnceLock<Regex> = OnceLock::new();
    static CONFUSION: OnceLock<Regex> = OnceLock::new();

    let corr_re = CORRECTION.get_or_init(|| {
        Regex::new(
            r"(?i)(?:actually[,\s]+that'?s (?:wrong|incorrect)|you'?re wrong|you made (?:a|an) (?:mistake|error)|that'?s not (?:right|correct)|no[,\s]+that'?s wrong|you just said|but you said)",
        )
        .unwrap()
    });
    let sat_re = SATISFACTION.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:perfect|great|exactly(?: right)?|that(?:'?s| is) (?:correct|right|(?:now )?working|good)|works?[!.]|good job|well done|thank(?:s| you)(?:[!,]| that))\b",
        )
        .unwrap()
    });
    let conf_re = CONFUSION.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:I meant|that'?s not what I|no[,\s]+I (?:want|need|meant)|what I (?:said|meant) was|you misunderstood)\b",
        )
        .unwrap()
    });

    let user_msgs = messages.iter().filter(|m| m.role == "user");

    let mut corrections = 0i64;
    let mut satisfactions = 0i64;
    let mut confusions = 0i64;

    for msg in user_msgs {
        corrections += corr_re.find_iter(&msg.content).count() as i64;
        satisfactions += sat_re.find_iter(&msg.content).count() as i64;
        confusions += conf_re.find_iter(&msg.content).count() as i64;
    }

    let mut score = 10i64;
    score -= (corrections * 3).min(10);
    score -= confusions.min(3);
    score += (satisfactions * 2).min(5);

    score.max(0).min(15)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Message, Session};

    fn make_session(token_count: i64, message_count: i64) -> Session {
        Session {
            id: "test".into(),
            title: None,
            model: None,
            created_at: "2024-01-01T00:00:00Z".into(),
            updated_at: "2024-01-01T01:00:00Z".into(),
            message_count,
            token_count,
            cwd: None,
            git_branch: None,
            version: None,
            source: "claude".into(),
        }
    }

    fn msg(role: &str, content: &str) -> Message {
        Message {
            id: "msg-id".into(),
            session_id: "test".into(),
            role: role.into(),
            content: content.into(),
            created_at: "2024-01-01T00:00:00Z".into(),
            token_count: 0,
            parent_id: None,
            model: None,
        }
    }

    // ── Viability gate ────────────────────────────────────────────────────────

    #[test]
    fn trivial_session_is_not_scorable() {
        let session = make_session(100, 2);
        let msgs = vec![
            msg("user", "exit"),
            msg("assistant", "Goodbye."),
        ];
        assert!(score_session(&session, &msgs).is_none());
    }

    #[test]
    fn substantial_session_is_scorable() {
        let session = make_session(5000, 4);
        let msgs = vec![
            msg("user", "Please add authentication to the login route"),
            msg("assistant", "I'll add JWT authentication. First I'll update the middleware then the route handlers."),
            msg("user", "Also add refresh token support please"),
            msg("assistant", "Done. I've added refresh token logic in auth.rs and updated the route to return both tokens."),
        ];
        assert!(score_session(&session, &msgs).is_some());
    }

    // ── Security ─────────────────────────────────────────────────────────────

    #[test]
    fn security_deducts_for_dangerous_commands() {
        let msgs = vec![msg("assistant", "run: rm -rf /tmp/data")];
        assert!(score_security(&msgs) < 15);
    }

    #[test]
    fn security_full_for_safe_content() {
        let msgs = vec![msg("assistant", "Use cargo build to compile the project.")];
        assert_eq!(score_security(&msgs), 15);
    }

    // ── Effectivity ───────────────────────────────────────────────────────────

    #[test]
    fn effectivity_baseline_without_signals() {
        let msgs = vec![
            msg("user", "Please update the config file"),
            msg("assistant", "I've updated the config file with the new settings."),
        ];
        // No failure or success keywords → baseline 8
        assert_eq!(score_effectivity(&msgs), 8);
    }

    #[test]
    fn effectivity_rises_with_completion_signal() {
        let msgs = vec![
            msg("user", "Fix the authentication bug"),
            msg("assistant", "I've investigated the issue and fixed the JWT verification logic."),
            msg("user", "Great, test it"),
            msg("assistant", "All tests pass and the auth is working now. Fixed!"),
        ];
        assert!(score_effectivity(&msgs) > 8);
    }

    #[test]
    fn effectivity_drops_with_failure_loops() {
        let msgs = vec![
            msg("user", "add a login form"),
            msg("assistant", "Here's the form."),
            msg("user", "doesn't work, still broken"),
            msg("assistant", "Let me try again."),
            msg("user", "still not working, wrong output"),
            msg("assistant", "Fixing now."),
        ];
        assert!(score_effectivity(&msgs) < 8);
    }

    // ── Solidity ──────────────────────────────────────────────────────────────

    #[test]
    fn solidity_max_for_test_execution() {
        let msgs = vec![msg("assistant", "Run `cargo test` to verify all tests pass.")];
        assert_eq!(score_solidity(&msgs), 10);
    }

    #[test]
    fn solidity_mid_for_test_reference() {
        let msgs = vec![msg("assistant", "I added a #[test] function for this edge case.")];
        assert_eq!(score_solidity(&msgs), 7);
    }

    #[test]
    fn solidity_neutral_for_non_coding_session() {
        let msgs = vec![
            msg("user", "checkout the dev branch and pull the latest"),
            msg("assistant", "Done. Switched to dev and pulled the latest changes."),
        ];
        assert_eq!(score_solidity(&msgs), 5);
    }

    // ── Efficiency ───────────────────────────────────────────────────────────

    #[test]
    fn efficiency_zero_for_empty_session() {
        let session = make_session(0, 0);
        let msgs: Vec<Message> = vec![];
        assert_eq!(score_efficiency(&session, &msgs), 0);
    }

    #[test]
    fn efficiency_bonus_for_short_clean_session() {
        let session = make_session(1000, 4);
        let msgs = vec![
            msg("user", "Please refactor the parse function"),
            msg("assistant", "Refactored. The function is now cleaner and more readable."),
            msg("user", "Looks good, thanks"),
            msg("assistant", "Great!"),
        ];
        assert!(score_efficiency(&session, &msgs) > 10);
    }

    #[test]
    fn efficiency_drops_with_correction_loops() {
        let session = make_session(10000, 10);
        let msgs = vec![
            msg("user", "build the project"),
            msg("assistant", "Built."),
            msg("user", "doesn't work, still broken"),
            msg("assistant", "Fixing."),
            msg("user", "still not working"),
            msg("assistant", "Trying again."),
        ];
        assert!(score_efficiency(&session, &msgs) < 10);
    }

    // ── Planning ─────────────────────────────────────────────────────────────

    #[test]
    fn planning_high_for_numbered_steps() {
        let msgs = vec![msg(
            "assistant",
            "Here's my plan:\n1. Update the schema\n2. Migrate existing data\n3. Update the API endpoints",
        )];
        assert!(score_planning_quality(&msgs) >= 12);
    }

    #[test]
    fn planning_baseline_for_no_structure() {
        let msgs = vec![msg("assistant", "Let me look at the code and help you.")];
        assert_eq!(score_planning_quality(&msgs), 5);
    }

    #[test]
    fn planning_mid_for_sequential_connectors() {
        let msgs = vec![msg(
            "assistant",
            "First I'll update the auth middleware, then I'll fix the token refresh endpoint.",
        )];
        assert!(score_planning_quality(&msgs) >= 7);
    }

    // ── Recovery ─────────────────────────────────────────────────────────────

    #[test]
    fn recovery_neutral_when_no_errors() {
        let msgs = vec![
            msg("user", "Add a config file parser"),
            msg("assistant", "I've added the TOML parser and connected it to the config struct."),
        ];
        assert_eq!(score_recovery_ability(&msgs), 10);
    }

    #[test]
    fn recovery_high_when_errors_are_addressed() {
        let msgs = vec![
            msg("assistant", "error: undefined variable `x`"),
            msg("assistant", "I found the issue — fixed the undefined variable. Should work now."),
            msg("assistant", "The problem was a missing import. Resolved and corrected."),
        ];
        assert!(score_recovery_ability(&msgs) >= 12);
    }

    // ── Hallucination / Interaction Quality ──────────────────────────────────

    #[test]
    fn interaction_baseline_without_signals() {
        let msgs = vec![
            msg("user", "Can you update the README?"),
            msg("assistant", "Updated the README with the new API docs."),
        ];
        assert_eq!(score_hallucination_rate(&msgs), 10);
    }

    #[test]
    fn interaction_rises_with_satisfaction() {
        let msgs = vec![
            msg("user", "Add dark mode support"),
            msg("assistant", "Added dark mode with a CSS variable approach."),
            msg("user", "Perfect, that works great!"),
            msg("assistant", "Glad it works!"),
        ];
        assert!(score_hallucination_rate(&msgs) > 10);
    }

    #[test]
    fn interaction_drops_with_corrections() {
        let msgs = vec![
            msg("user", "Refactor auth"),
            msg("assistant", "Done."),
            msg("user", "Actually that's wrong, you're wrong about the token expiry"),
            msg("assistant", "Correcting."),
        ];
        assert!(score_hallucination_rate(&msgs) < 10);
    }

    // ── Grade thresholds ─────────────────────────────────────────────────────

    #[test]
    fn grade_mapping() {
        assert_eq!(grade_for(90), "S");
        assert_eq!(grade_for(80), "A");
        assert_eq!(grade_for(70), "B");
        assert_eq!(grade_for(60), "C");
        assert_eq!(grade_for(50), "D");
        assert_eq!(grade_for(40), "F");
    }

    // ── Total integrity ──────────────────────────────────────────────────────

    #[test]
    fn score_session_total_equals_sum_of_dims() {
        let session = make_session(5000, 6);
        let msgs = vec![
            msg("user", "Please add input validation to the registration form"),
            msg("assistant", "I'll add validation in three steps:\n1. Add schema validation\n2. Update the form handler\n3. Add error messages to the UI"),
            msg("user", "Also validate email format"),
            msg("assistant", "Added email regex validation. All tests pass — fixed and working."),
            msg("user", "Perfect, that works!"),
            msg("assistant", "Great! The form now validates all fields correctly."),
        ];
        let score = score_session(&session, &msgs).expect("should be scorable");
        assert_eq!(
            score.total_score,
            score.security
                + score.effectivity
                + score.solidity
                + score.efficiency
                + score.planning_quality
                + score.recovery_ability
                + score.hallucination_rate
        );
        assert!(score.total_score <= 100);
        assert!(score.total_score >= 0);
    }
}

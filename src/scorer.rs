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

/// Score a session using heuristic rules. No external calls.
pub fn score_session(session: &Session, messages: &[Message]) -> Score {
    let security = score_security(messages);
    let effectivity = score_effectivity(messages);
    let solidity = score_solidity(messages);
    let efficiency = score_efficiency(session);
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

    Score {
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
    }
}

fn grade_for(score: i64) -> &'static str {
    match score {
        90..=100 => "S",
        80..=89 => "A",
        70..=79 => "B",
        60..=69 => "C",
        50..=59 => "D",
        _ => "F",
    }
}

/// Security (0–15): deduct 3 per dangerous pattern found in any message.
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

/// Effectivity (0–15): ratio of user messages containing failure/correction signals.
fn score_effectivity(messages: &[Message]) -> i64 {
    static FAILURE: OnceLock<Regex> = OnceLock::new();
    let re = FAILURE.get_or_init(|| {
        Regex::new(r"(?i)(?:doesn'?t work|not working|still (?:not|fails?|broken)|wrong answer|incorrect(?:ly)?|compile error|build fail|try again|that'?s not right|still broken)")
            .unwrap()
    });

    let user_msgs: Vec<_> = messages.iter().filter(|m| m.role == "user").collect();
    if user_msgs.is_empty() {
        return 15;
    }

    let failures = user_msgs.iter().filter(|m| re.is_match(&m.content)).count();
    let ratio = failures as f64 / user_msgs.len() as f64;

    if ratio == 0.0 {
        15
    } else if ratio < 0.10 {
        13
    } else if ratio < 0.20 {
        11
    } else if ratio < 0.30 {
        8
    } else if ratio < 0.40 {
        5
    } else if ratio < 0.50 {
        3
    } else {
        1
    }
}

/// Solidity (0–10): presence of test execution or test-related content.
fn score_solidity(messages: &[Message]) -> i64 {
    static TEST_EXEC: OnceLock<Regex> = OnceLock::new();
    static TEST_REF: OnceLock<Regex> = OnceLock::new();
    let exec_re = TEST_EXEC.get_or_init(|| {
        Regex::new(r"(?i)cargo\s+test|npm\s+test|yarn\s+test|pytest|go\s+test|jest|mocha|rspec")
            .unwrap()
    });
    let ref_re = TEST_REF.get_or_init(|| {
        Regex::new(r"(?i)\btest(?:s|ing)?\b|\bassert\b|\.test\.|_test\b|\.spec\.|#\[test\]|@Test")
            .unwrap()
    });

    let assistant_msgs: Vec<_> = messages.iter().filter(|m| m.role == "assistant").collect();

    if assistant_msgs.iter().any(|m| exec_re.is_match(&m.content)) {
        return 10;
    }
    if assistant_msgs.iter().any(|m| ref_re.is_match(&m.content)) {
        return 6;
    }
    2
}

/// Efficiency (0–15): average tokens per message and total message count.
fn score_efficiency(session: &Session) -> i64 {
    if session.message_count == 0 {
        return 15;
    }
    let avg_tpm = session.token_count / session.message_count;
    let base = if avg_tpm < 500 {
        15
    } else if avg_tpm < 1000 {
        13
    } else if avg_tpm < 2000 {
        11
    } else if avg_tpm < 3000 {
        8
    } else if avg_tpm < 4000 {
        5
    } else {
        3
    };
    // Small penalty for very long sessions (many rounds)
    let msg_penalty = ((session.message_count - 20).max(0) / 10).min(2);
    (base - msg_penalty).max(0)
}

/// Planning Quality (0–15): planning language in the first 3 assistant messages.
fn score_planning_quality(messages: &[Message]) -> i64 {
    static PLAN: OnceLock<Regex> = OnceLock::new();
    let re = PLAN.get_or_init(|| {
        Regex::new(r"(?i)(?:first[,\s]|let me|I'?ll start|my approach|the plan|step \d|here'?s (?:what|my|the)|I'?ll need to|strategy|overview|to begin|start(?:ing)? with)")
            .unwrap()
    });

    let matches: usize = messages
        .iter()
        .filter(|m| m.role == "assistant")
        .take(3)
        .map(|m| re.find_iter(&m.content).count())
        .sum();

    if matches == 0 {
        5
    } else if matches < 3 {
        10
    } else {
        15
    }
}

/// Recovery Ability (0–15): ratio of recovery signals to error signals.
fn score_recovery_ability(messages: &[Message]) -> i64 {
    static ERRORS: OnceLock<Regex> = OnceLock::new();
    static RECOVERY: OnceLock<Regex> = OnceLock::new();
    let err_re = ERRORS.get_or_init(|| {
        Regex::new(r"(?i)\berror\b|\bexception\b|\bfailed\b|\bpanic\b|traceback|undefined|null pointer|segfault|\bcrash\b")
            .unwrap()
    });
    let rec_re = RECOVERY.get_or_init(|| {
        Regex::new(r"(?i)\bfixed\b|\bresolved\b|now works|should work|\bupdated\b|\bcorrected\b|let me try|instead[,\s]|alternatively|the issue was|the problem was")
            .unwrap()
    });

    let errors: usize = messages
        .iter()
        .map(|m| err_re.find_iter(&m.content).count())
        .sum();
    if errors == 0 {
        return 15;
    }
    let recoveries: usize = messages
        .iter()
        .map(|m| rec_re.find_iter(&m.content).count())
        .sum();
    let ratio = recoveries as f64 / errors as f64;
    (ratio * 15.0).round().min(15.0) as i64
}

/// Hallucination Rate (0–15): user correction signals in user messages.
fn score_hallucination_rate(messages: &[Message]) -> i64 {
    static CORRECTION: OnceLock<Regex> = OnceLock::new();
    let re = CORRECTION.get_or_init(|| {
        Regex::new(r"(?i)(?:actually[,\s]|that'?s (?:wrong|incorrect)|you'?re wrong|you made (?:a|an) (?:mistake|error)|that'?s not (?:right|correct)|no[,\s]+that|you just said|but you said)")
            .unwrap()
    });

    let corrections: usize = messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| re.find_iter(&m.content).count())
        .sum();

    match corrections {
        0 => 15,
        1 => 12,
        2 => 9,
        3 => 6,
        4 => 3,
        _ => 1,
    }
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

    #[test]
    fn effectivity_perfect_with_no_failures() {
        let msgs = vec![
            msg("user", "Please add a login form"),
            msg("assistant", "Done, here's the form"),
        ];
        assert_eq!(score_effectivity(&msgs), 15);
    }

    #[test]
    fn effectivity_lower_with_many_failures() {
        let msgs = vec![
            msg("user", "doesn't work"),
            msg("user", "still not working"),
            msg("user", "wrong answer"),
            msg("user", "please fix"),
        ];
        assert!(score_effectivity(&msgs) < 10);
    }

    #[test]
    fn solidity_max_for_test_execution() {
        let msgs = vec![msg("assistant", "Run cargo test to verify all tests pass.")];
        assert_eq!(score_solidity(&msgs), 10);
    }

    #[test]
    fn solidity_mid_for_test_reference() {
        let msgs = vec![msg("assistant", "I added a test function for this case.")];
        assert_eq!(score_solidity(&msgs), 6);
    }

    #[test]
    fn efficiency_high_for_low_token_ratio() {
        let session = make_session(1000, 10); // 100 tpm
        assert_eq!(score_efficiency(&session), 15);
    }

    #[test]
    fn grade_mapping() {
        assert_eq!(grade_for(95), "S");
        assert_eq!(grade_for(85), "A");
        assert_eq!(grade_for(75), "B");
        assert_eq!(grade_for(65), "C");
        assert_eq!(grade_for(55), "D");
        assert_eq!(grade_for(40), "F");
    }

    #[test]
    fn score_session_total_equals_sum_of_dims() {
        let session = make_session(500, 5);
        let msgs = vec![
            msg("user", "Add auth"),
            msg("assistant", "Let me start by adding login. First, I'll create the route."),
        ];
        let score = score_session(&session, &msgs);
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
    }
}

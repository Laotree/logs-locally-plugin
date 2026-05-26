use regex::Regex;
use std::sync::OnceLock;

static RULES: OnceLock<Vec<(Regex, String)>> = OnceLock::new();

fn rules() -> &'static Vec<(Regex, String)> {
    RULES.get_or_init(|| {
        // Each tuple: (pattern, replacement).
        // Replacements may reference capture groups via $1, $2, etc.
        // Order matters: more-specific patterns run before broader ones.
        let raw: &[(&str, &str)] = &[
            // URLs with embedded credentials — run before the email pattern.
            // Matches any scheme (https, postgres, mongodb, ftp, …)
            (
                r"([a-zA-Z][a-zA-Z0-9+\-.]*://)[^:@\s/]+:[^@\s]+@",
                "$1<USER>:<PASSWORD>@",
            ),
            // Anthropic API keys
            (r"sk-ant-[A-Za-z0-9_\-]{20,}", "<API_KEY>"),
            // OpenAI-style keys: sk- followed by ≥32 alphanumeric chars (no dash)
            (r"sk-[A-Za-z0-9]{32,}", "<API_KEY>"),
            // GitHub tokens
            (r"(?:ghp|gho|ghs|ghu|ghr)_[A-Za-z0-9]{36}", "<API_KEY>"),
            // AWS IAM access key IDs
            (r"AKIA[0-9A-Z]{16}", "<API_KEY>"),
            // Bearer tokens in Authorization headers
            (
                r"(?i)Bearer\s+[A-Za-z0-9._\-+/]{20,}",
                "Bearer <API_KEY>",
            ),
            // Env var assignments where the name contains a sensitive keyword.
            // Keeps variable name + "=", replaces the value with <REDACTED>.
            (
                r#"((?:export +)?[A-Za-z_][A-Za-z0-9_]*(?:KEY|SECRET|TOKEN|PASSWORD|PASSWD|CREDENTIAL)[A-Za-z0-9_]* *= *)([^\s'"]{4,})"#,
                "${1}<REDACTED>",
            ),
            // Unix home-directory paths — replace the username segment only
            (
                r#"(/(?:Users|home)/)([^/\s"'\\]+)"#,
                "${1}<USER>",
            ),
            // Windows user-directory paths
            (
                r#"(?i)(C:\\Users\\)([^\\]+)"#,
                "${1}<USER>",
            ),
            // Email addresses (after URL/path patterns to avoid collisions)
            (
                r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
                "<EMAIL>",
            ),
        ];

        raw.iter()
            .map(|(pat, rep)| {
                (
                    Regex::new(pat).unwrap_or_else(|e| panic!("invalid scrub regex {pat:?}: {e}")),
                    rep.to_string(),
                )
            })
            .collect()
    })
}

/// Return a copy of `text` with sensitive patterns replaced by placeholders.
pub fn scrub_sensitive(text: &str) -> String {
    let mut out = text.to_owned();
    for (re, replacement) in rules() {
        let replaced = re.replace_all(&out, replacement.as_str()).into_owned();
        out = replaced;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::scrub_sensitive;

    #[test]
    fn redacts_anthropic_key() {
        let s = scrub_sensitive("key=sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij");
        assert!(!s.contains("sk-ant-api03"), "got: {s}");
        assert!(s.contains("<API_KEY>"), "got: {s}");
    }

    #[test]
    fn redacts_openai_style_key() {
        // Caught either by the sk-... key pattern or the env-var pattern
        let s = scrub_sensitive("OPENAI_KEY=sk-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij");
        assert!(s.contains("<API_KEY>") || s.contains("<REDACTED>"), "got: {s}");
        assert!(!s.contains("sk-ABCDEFGHIJ"), "got: {s}");
    }

    #[test]
    fn redacts_github_token() {
        let s = scrub_sensitive("token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij");
        assert!(!s.contains("ghp_"), "got: {s}");
        assert!(s.contains("<API_KEY>"), "got: {s}");
    }

    #[test]
    fn redacts_aws_access_key() {
        let s = scrub_sensitive("AKIAIOSFODNN7EXAMPLE");
        assert!(s.contains("<API_KEY>"), "got: {s}");
    }

    #[test]
    fn redacts_bearer_token() {
        let s = scrub_sensitive("Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9");
        assert!(!s.contains("eyJ"), "got: {s}");
        assert!(s.contains("<API_KEY>"), "got: {s}");
    }

    #[test]
    fn redacts_env_var_value() {
        let s = scrub_sensitive("export DATABASE_PASSWORD=super_secret_value");
        assert!(!s.contains("super_secret_value"), "got: {s}");
        assert!(s.contains("DATABASE_PASSWORD"), "variable name must be preserved: {s}");
        assert!(s.contains("<REDACTED>"), "got: {s}");
    }

    #[test]
    fn redacts_mac_home_path() {
        let s = scrub_sensitive("working in /Users/alice/Documents/project");
        assert!(!s.contains("/Users/alice"), "got: {s}");
        assert!(s.contains("/Users/<USER>"), "got: {s}");
    }

    #[test]
    fn redacts_linux_home_path() {
        let s = scrub_sensitive("config at /home/bob/.config/foo");
        assert!(!s.contains("/home/bob"), "got: {s}");
        assert!(s.contains("/home/<USER>"), "got: {s}");
    }

    #[test]
    fn redacts_windows_home_path() {
        let s = scrub_sensitive(r"C:\Users\Alice\AppData\Roaming");
        assert!(!s.contains("Alice"), "got: {s}");
        assert!(s.contains("<USER>"), "got: {s}");
    }

    #[test]
    fn redacts_email() {
        let s = scrub_sensitive("contact alice@example.com for help");
        assert!(!s.contains("alice@example.com"), "got: {s}");
        assert!(s.contains("<EMAIL>"), "got: {s}");
    }

    #[test]
    fn redacts_url_with_credentials() {
        let s = scrub_sensitive("postgres://admin:s3cr3t@db.example.com:5432/mydb");
        assert!(!s.contains("s3cr3t"), "got: {s}");
        assert!(s.contains("<PASSWORD>"), "got: {s}");
    }

    #[test]
    fn preserves_innocuous_text() {
        let s = "Hello world, the answer is 42.";
        assert_eq!(scrub_sensitive(s), s);
    }
}

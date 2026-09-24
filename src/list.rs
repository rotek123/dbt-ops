use crate::api::ApiClient;
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossterm::style::Stylize;
use url::Url;

pub fn print_runs(api_client: &ApiClient, limit: u64, job_id: Option<i64>) -> Result<()> {
    let runs = api_client.list_runs(limit, job_id)?;

    println!(
        "{:<8} {:<30} {:<25} {:<10} {:<15} {:<12} {:<10} {:<20}",
        "STATUS", "JOB NAME", "BRANCH", "PR", "JOB ID", "RUN ID", "ELAPSED", "AGE"
    );

    for run in runs {
        let status = match run.status {
            10 => "✓".green(),  // Success
            20 => "X".red(),    // Error
            30 => "⨯".yellow(), // Cancelled
            _ => "⟳".cyan(),    // Running / Queued
        };

        let mut job_name = run
            .job
            .and_then(|j| j.name)
            .unwrap_or_else(|| "unknown".to_string());

        // Truncate if it's too long to prevent breaking the table alignment
        if job_name.len() > 29 {
            job_name = format!("{}..", &job_name[..28]);
        }

        let branch = run.git_branch.unwrap_or_else(|| "unknown".to_string());

        let branch = if branch.len() > 23 {
            format!("{}..", &branch[..21])
        } else {
            branch
        };

        let (pr_display_str, pr_visual_len) =
            match run.trigger.as_ref().and_then(|t| t.github_pull_request_id) {
                Some(pr_id) => {
                    let text = format!("#{}", pr_id);
                    let v_len = text.len();

                    // Only render OSC8 if the URL is trusted
                    if let Some(repo_url) = &api_client.repo_url {
                        if is_trusted_repo_url(repo_url) {
                            (
                                format!(
                                    "\x1b]8;;{}/pull/{}\x1b\\{}\x1b]8;;\x1b\\",
                                    repo_url, pr_id, text
                                ),
                                v_len,
                            )
                        } else {
                            // Fallback to plain text if untrusted
                            (text, v_len)
                        }
                    } else {
                        (text, v_len)
                    }
                }
                None => ("-".to_string(), 1),
            };

        let pr_padded = format!(
            "{}{}",
            pr_display_str,
            " ".repeat(10usize.saturating_sub(pr_visual_len))
        );

        let job_id_str = run
            .job_definition_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());

        let elapsed = run.duration_humanized.unwrap_or_else(|| "-".to_string());

        // Shorten "23 minutes, 56 seconds" -> "23m 56s"
        let elapsed = elapsed
            .replace(" minutes", "m")
            .replace(" minute", "m")
            .replace(" seconds", "s")
            .replace(" second", "s")
            .replace(" hours", "h")
            .replace(" hour", "h")
            .replace(", ", " ");

        let age = match run.created_at {
            Some(created_str) => {
                if let Ok(created_dt) = DateTime::parse_from_rfc3339(&created_str) {
                    let now = Utc::now();
                    let duration = now.signed_duration_since(created_dt);
                    format_age(duration)
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };

        // Note: We use `{}` and manual padding for columns with ANSI/OSC8 escape codes
        println!(
            "{}        {:<30} {:<25} {} {:<15} {:<12} {:<10} {:<20}",
            status, job_name, branch, pr_padded, job_id_str, run.id, elapsed, age
        );
    }

    Ok(())
}

/// Helper func that validates that a repository URL belongs to a trusted
/// domain before we allow it to be rendered as an OSC8 clickable terminal link.
fn is_trusted_repo_url(url_str: &str) -> bool {
    let trusted_domains: [&str; 1] = ["github.com"];

    if let Ok(parsed_url) = Url::parse(url_str)
        && let Some(host) = parsed_url.host_str()
    {
        // Only allow scheme of the URL to be either http or https
        let scheme = parsed_url.scheme();
        if scheme != "http" && scheme != "https" {
            return false;
        }
        return trusted_domains.contains(&host);
    }
    false
}

fn format_age(duration: chrono::Duration) -> String {
    let secs = duration.num_seconds();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        let days = secs / 86400;
        if days == 1 {
            "about 1 day ago".to_string()
        } else {
            format!("about {} days ago", days)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_trusted_repo_url() {
        // Valid domains
        assert!(is_trusted_repo_url("https://github.com/my-org/repo"));
        assert!(is_trusted_repo_url("http://github.com/foo/bar"));

        // Malicious or un-allowed domains
        assert!(!is_trusted_repo_url("https://github.com.evil.com/repo")); // Domain spoofing
        assert!(!is_trusted_repo_url("https://evil.github.com/repo")); // Subdomain spoofing
        assert!(!is_trusted_repo_url("https://gitlab.com/repo")); // Not in allowlist
        assert!(!is_trusted_repo_url("ftp://github.com/repo")); // While host is github, you may optionally want to check http/https, though this passes the host check.

        // Invalid URLs
        assert!(!is_trusted_repo_url("not-a-url"));
        assert!(!is_trusted_repo_url(""));
    }

    #[test]
    fn test_format_age() {
        assert_eq!(format_age(chrono::Duration::seconds(45)), "45s ago");
        assert_eq!(format_age(chrono::Duration::seconds(120)), "2m ago");
        assert_eq!(format_age(chrono::Duration::seconds(3600)), "1h ago");
        assert_eq!(format_age(chrono::Duration::seconds(7200)), "2h ago");
        assert_eq!(
            format_age(chrono::Duration::seconds(86400)),
            "about 1 day ago"
        );
        assert_eq!(
            format_age(chrono::Duration::seconds(172800)),
            "about 2 days ago"
        );
    }
}

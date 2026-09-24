use crate::api::{ApiClient, RunResultsArtifact, SourcesArtifact};
use anyhow::Result;
use ratatui::style::{Color, Stylize};
use regex::Regex;

/// Extracts detailed error blocks from raw dbt standard output logs.
/// Looks for `ERROR        Error [...]` or `[ERROR]` and captures subsequent lines until the next log entry.
fn extract_log_errors(logs: &str) -> Vec<String> {
    // Strip ANSI color codes to make regex matching reliable
    let ansi_escape = Regex::new(r"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])").unwrap();

    // Convert escaped JSON newlines to real newlines
    let decoded_logs = logs.replace("\\n", "\n");
    let clean_logs = ansi_escape.replace_all(&decoded_logs, "");

    let mut errors = Vec::new();
    let lines: Vec<&str> = clean_logs.split("\n").collect();

    // Standard dbt log error pattern (e.g., "  ERROR  Error [JinjaError...]")
    let error_pattern =
        Regex::new(r"(?i)(?:\s+(?:ERROR\s+Error\s+\[|INFO\s+Failed\s+\[)|^\[error\]\s+\[)")
            .unwrap();
    let stop_pattern = Regex::new(r"(?i)(?:^\d{2}:\d{2}:\d{2}\s|^={10,}|\[error\]\s+\[)").unwrap();

    let mut i = 0;
    while i < lines.len() {
        if error_pattern.is_match(lines[i]) {
            let mut message_lines = vec![lines[i].trim_end_matches('\r')];
            let mut j = i + 1;

            // Capture continuation lines until next log entry (timestamp) or we hit a max limit to prevent runaways
            while j < lines.len() && message_lines.len() < 30 {
                let next_line = lines[j];
                if stop_pattern.is_match(next_line) {
                    break;
                }
                message_lines.push(next_line.trim_end_matches('\r'));
                j += 1;
            }

            // Remove trailing empty lines that were captured before the boundary
            while let Some(last) = message_lines.last() {
                if last.trim().is_empty() {
                    message_lines.pop();
                } else {
                    break;
                }
            }
            errors.push(message_lines.join("\n"));
            i = j;
        } else {
            i += 1;
        }
    }

    errors
}

pub fn print_run_errors(
    api_client: &ApiClient,
    run_id: i64,
    include_warnings: bool,
    warnings_only: bool,
) -> Result<()> {
    // 1. Fetch the run steps
    let run = api_client.get_run(run_id)?;
    let steps = run.run_steps.unwrap_or_default();

    let mut found_issues = false;

    // 2. Determine which steps to inspect based on status
    for step in steps {
        // Status 20 = Error, Status 10 = Success (but could have warnings)
        let has_error = step.status == 20;
        let is_success = step.status == 10;

        if (warnings_only && !is_success && !has_error)
            || (!warnings_only && !has_error && !include_warnings)
        {
            continue;
        }

        let mut step_has_issues = false;
        let mut error_count = 0;
        let mut warning_count = 0;

        // 3. Try to fetch run_results.json first
        if let Ok(artifact_json) = api_client.get_artifact(run_id, step.index, "run_results.json") {
            if let Ok(artifact) = serde_json::from_str::<RunResultsArtifact>(&artifact_json) {
                // Check if all errors in this artifact are just generic "Compilation Error"
                let mut has_real_json_errors = false;
                for result in &artifact.results {
                    if (result.status == "error" || result.status == "fail")
                        && result.message.as_deref() != Some("Compilation Error")
                        && result.message.as_deref() != Some("Error")
                    {
                        has_real_json_errors = true;
                    }
                }

                // If we found only useless generic errors, fall back to parsing the raw logs immediately
                if has_error && !has_real_json_errors && !warnings_only {
                    let log_errors = extract_log_errors(step.logs.as_deref().unwrap_or(""));

                    if !log_errors.is_empty() {
                        if !step_has_issues {
                            println!("\n▶ Step {}: {}", step.index, step.name.as_str().bold());
                            step_has_issues = true;
                            found_issues = true;
                        }

                        for err_msg in log_errors {
                            error_count += 1;
                            println!(
                                "  {} Compilation or Jinja Failure",
                                "✖ ERROR".fg(Color::Red).bold()
                            );

                            // Iterate line by line because styling a multiline string loses formatting on subsequent lines
                            for line in err_msg.split('\n') {
                                println!("    {}", line.fg(Color::DarkGray));
                            }
                        }
                    }
                }

                // Process standard warnings or remaining real JSON errors
                for result in artifact.results {
                    if (!warnings_only
                        && (result.status == "error" || result.status == "fail")
                        && result.message.as_deref() != Some("Compilation Error")
                        && result.message.as_deref() != Some("Error"))
                        || (include_warnings && result.status == "warn")
                        || (warnings_only && result.status == "warn")
                    {
                        if !step_has_issues {
                            println!("\n▶ Step {}: {}", step.index, step.name.as_str().bold());
                            step_has_issues = true;
                            found_issues = true;
                        }

                        if result.status == "error" || result.status == "fail" {
                            error_count += 1;
                            println!("  {} {}", "✖ ERROR".fg(Color::Red).bold(), result.unique_id);
                            if let Some(msg) = result.message {
                                for line in msg.split('\n') {
                                    println!("    {}", line.fg(Color::DarkGray));
                                }
                            }
                        } else if result.status == "warn" {
                            warning_count += 1;
                            println!(
                                "  {} {}",
                                "⚠ WARN ".fg(Color::Yellow).bold(),
                                result.unique_id
                            );
                            if let Some(msg) = result.message {
                                for line in msg.split('\n') {
                                    println!("    {}", line.fg(Color::DarkGray));
                                }
                            }
                        }
                    }
                }
            }
        }
        // 4. Fallback to sources.json for source freshness checks
        else if let Ok(artifact_json) =
            api_client.get_artifact(run_id, step.index, "sources.json")
        {
            if let Ok(artifact) = serde_json::from_str::<SourcesArtifact>(&artifact_json) {
                for result in artifact.results {
                    if (!warnings_only && (result.status == "error" || result.status == "fail"))
                        || (include_warnings && result.status == "warn")
                        || (warnings_only && result.status == "warn")
                    {
                        if !step_has_issues {
                            println!("\n▶ Step {}: {}", step.index, step.name.as_str().bold());
                            step_has_issues = true;
                            found_issues = true;
                        }

                        if result.status == "error" || result.status == "fail" {
                            error_count += 1;
                            println!("  {} {}", "✖ ERROR".fg(Color::Red).bold(), result.unique_id);
                            if let Some(secs) = result.max_loaded_at_time_ago_in_s {
                                println!(
                                    "    {} {:.0}s since last load",
                                    "Source freshness error:".fg(Color::DarkGray),
                                    secs
                                );
                            }
                        } else if result.status == "warn" {
                            warning_count += 1;
                            println!(
                                "  {} {}",
                                "⚠ WARN ".fg(Color::Yellow).bold(),
                                result.unique_id
                            );
                            if let Some(secs) = result.max_loaded_at_time_ago_in_s {
                                println!(
                                    "    {} {:.0}s since last load",
                                    "Source freshness warning:".fg(Color::DarkGray),
                                    secs
                                );
                            }
                        }
                    }
                }
            }
        } else if has_error && !warnings_only {
            let log_errors = extract_log_errors(step.logs.as_deref().unwrap_or(""));

            if !log_errors.is_empty() {
                if !step_has_issues {
                    println!("\n▶ Step {}: {}", step.index, step.name.as_str().bold());
                    step_has_issues = true;
                    found_issues = true;
                }

                for err_msg in log_errors {
                    error_count += 1;
                    println!(
                        "  {} Compilation, Jinja, Lint, or Unit Test Failure",
                        "✖ ERROR".fg(Color::Red).bold()
                    );

                    for line in err_msg.split('\n') {
                        println!("    {}", line.fg(Color::DarkGray));
                    }
                }
            } else {
                // 5. Total artifact & log failure fallback
                println!("\n▶ Step {}: {}", step.index, step.name.as_str().bold());
                println!(
                    "  {} No structured error artifact generated.",
                    "✖ ERROR".fg(Color::Red).bold()
                );
                println!(
                    "    {}",
                    "Please run `dbt-ops --run-id <ID>` to view the raw execution logs."
                        .fg(Color::DarkGray)
                );
                found_issues = true;
                error_count += 1;
            }
        }

        if step_has_issues {
            println!("  ---");
            if warnings_only {
                println!("  {} warnings", warning_count);
            } else {
                println!("  {} errors | {} warnings", error_count, warning_count);
            }
        }
    }

    if !found_issues {
        if warnings_only {
            println!("\n{}", "✓ No warnings found for this run.".fg(Color::Green));
        } else {
            println!("\n{}", "✓ No errors found for this run.".fg(Color::Green));
        }
    }

    println!();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_log_errors_compilation() {
        let raw_logs = r#"
09:49:43      INFO      Started model     dbt_cloud_pr_123_test_schema.fct_sales
09:49:43   ERROR        Error [JinjaError (dbt1501)]: Error executing materialization macro 'dbt_bigquery.materialization_incremental_bigquery' for model model.my_project.fct_sales: Failed to eval the compiled Jinja expression invalid operation: Compilation Error for model.my_project.fct_sales from models/marts/fct_sales.sql: This model has an enforced contract that failed.
                                Please ensure the name, data_type, and number of columns in your contract match the columns in your model's definition.
                                
                                |               column_name | definition_type | contract_type |     mismatch_reason |
                                | ------------------------- | --------------- | ------------- | ------------------- |
                                | is_first_purchase         | BOOLEAN         |               | missing in contract |
                                
                                (in run/my_project/models/marts/fct_sales.sql)
09:49:43      INFO      Skipped model
"#;

        let errors = extract_log_errors(raw_logs);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("This model has an enforced contract that failed"));
        assert!(errors[0].contains("is_first_purchase"));
    }

    #[test]
    fn test_extract_log_errors_unit_test() {
        let raw_logs = r#"
12:09:43      INFO       Started unit test intermediate.test_int_orders__dedup_keeps_latest (unit)
12:09:43      INFO       Failed [   1.42s] unit test intermediate.test_int_orders__dedup_keeps_latest (unit)
        Test failed test_int_orders__dedup_keeps_latest
        +-------------+-------------+
        | order_id    | total_amt   |
        +-------------+-------------+
        | ord1 -> ∅   | 150 -> ∅    |
        | ord2 -> ∅   | 200 -> ∅    |
        +-------------+-------------+
        2 row(s) differ.
        Expected 2 row(s), got 0 row(s).
12:09:44      INFO      Finished running...
"#;

        let errors = extract_log_errors(raw_logs);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains(
            "Failed [   1.42s] unit test intermediate.test_int_orders__dedup_keeps_latest"
        ));
        assert!(errors[0].contains("ord1 -> ∅"));
        assert!(!errors[0].contains("Finished running"));
    }

    #[test]
    fn test_extract_log_errors_lint() {
        let raw_logs = r#"
    Started linting (1 items)
    Finished [  0.02s] linting (1 items)

=================== Errors and Warnings ====================
[error] [DependencyNotFound (dbt1048)]: Ref 'fct_sales_terms' not found in project. Searched for 'my_project.fct_sales_terms'
    --> models/marts/sales/fct_subscriptions.sql:2:6

==================== Execution Summary =====================
Finished 'lint' with 1 error for target 'ci' [1.4s]
"#;

        let errors = extract_log_errors(raw_logs);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains(
            "[error] [DependencyNotFound (dbt1048)]: Ref 'fct_sales_terms' not found in project."
        ));
        assert!(errors[0].contains("models/marts/sales/fct_subscriptions.sql:2:6"));
        assert!(!errors[0].contains("Execution Summary"));
    }
}

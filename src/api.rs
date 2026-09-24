use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct DbtCloudConfig {
    pub projects: Vec<DbtProject>,
}

#[derive(Debug, Deserialize)]
pub struct DbtProject {
    #[serde(rename = "account-id")]
    pub account_id: Option<String>,
    #[serde(rename = "account-host")]
    pub account_host: Option<String>,
    #[serde(rename = "token-value")]
    pub token_value: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunResponse {
    pub data: Run,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Trigger {
    pub github_pull_request_id: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunsResponse {
    pub data: Vec<Run>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Job {
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Run {
    pub id: i64,
    pub status: i32,
    pub status_humanized: String,
    pub job_definition_id: Option<i64>,
    pub run_steps: Option<Vec<RunStep>>,
    pub duration_humanized: Option<String>,
    pub environment_id: Option<i64>,
    pub git_branch: Option<String>,
    pub created_at: Option<String>,
    pub job: Option<Job>,
    pub trigger: Option<Trigger>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunStep {
    pub index: i32,
    pub name: String,

    pub logs: Option<String>,
    pub status: i32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ArtifactResult {
    pub status: String,
    pub unique_id: String,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunResultsArtifact {
    pub results: Vec<ArtifactResult>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SourceResult {
    pub status: String,
    pub unique_id: String,
    pub max_loaded_at_time_ago_in_s: Option<f64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SourcesArtifact {
    pub results: Vec<SourceResult>,
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    token: String,
    account_id: String,
    host: String,
    pub repo_url: Option<String>,
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME").context("Could not find HOME directory")?;
        let config_path = PathBuf::from(home).join(".dbt/dbt_cloud.yml");
        let config_str =
            fs::read_to_string(config_path).context("Could not read ~/.dbt/dbt_cloud.yml")?;
        let config: DbtCloudConfig =
            serde_yaml::from_str(&config_str).context("Could not parse dbt_cloud.yml")?;

        let project = config
            .projects
            .first()
            .context("No projects found in dbt_cloud.yml")?;
        let token = project.token_value.clone();
        let account_id = project
            .account_id
            .clone()
            .unwrap_or_else(|| "517".to_string());
        let host = project
            .account_host
            .clone()
            .unwrap_or_else(|| "emea.dbt.com".to_string());

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        // Attempt to fetch the repository URL from the projects endpoint
        let mut repo_url = None;
        let projects_url = format!("https://{}/api/v2/accounts/{}/projects/", host, account_id);

        if let Ok(resp) = client
            .get(&projects_url)
            .header("Authorization", format!("Token {}", token))
            .send()
            && let Ok(json) = resp.json::<serde_json::Value>()
        {
            repo_url = json
                .get("data")
                .and_then(|d| d.as_array())
                .and_then(|arr| arr.first())
                .and_then(|proj| proj.get("repository"))
                .and_then(|repo| repo.get("web_url"))
                .and_then(|url| url.as_str())
                .map(|s| s.to_string());
        }

        Ok(Self {
            client,
            token,
            account_id,
            host,
            repo_url,
        })
    }

    fn base_url(&self) -> String {
        format!("https://{}/api/v2/accounts/{}", self.host, self.account_id)
    }

    pub fn get_run(&self, run_id: i64) -> Result<Run> {
        let url = format!(
            "{}/runs/{}/?include_related=[\"run_steps\"]",
            self.base_url(),
            run_id
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()?
            .error_for_status()?
            .json::<RunResponse>()?;
        Ok(resp.data)
    }

    pub fn get_latest_run(&self, job_id: i64) -> Result<Run> {
        let runs = self.list_runs(1, Some(job_id))?;
        let run = runs.into_iter().next().context("No runs found for job")?;
        self.get_run(run.id) // Get again to include run_steps
    }

    pub fn list_runs(&self, limit: u64, job_id: Option<i64>) -> Result<Vec<Run>> {
        let mut url = format!(
            "{}/runs/?order_by=-id&limit={}&include_related=[\"job\",\"trigger\"]",
            self.base_url(),
            limit
        );
        if let Some(id) = job_id {
            url.push_str(&format!("&job_definition_id={}", id));
        }

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()?
            .error_for_status()?
            .json::<RunsResponse>()?;

        Ok(resp.data)
    }

    pub fn get_artifact(&self, run_id: i64, step: i32, path: &str) -> Result<String> {
        let url = format!(
            "{}/runs/{}/artifacts/{}?step={}",
            self.base_url(),
            run_id,
            path,
            step
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()?
            .error_for_status()?
            .text()?;
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_new() {
        let client = ApiClient::new().expect("Failed to initialize ApiClient");
        assert!(!client.token.is_empty(), "Token should not be empty");
    }

    #[test]
    fn test_run_results_deserialization() {
        // Strict unit test to ensure our Rust structs correctly match the dbt JSON schema
        let raw_json = r#"{
            "results": [
                {
                    "status": "error",
                    "unique_id": "model.project.my_model",
                    "message": "BigQuery adapter error"
                },
                {
                    "status": "warn",
                    "unique_id": "model.project.other_model",
                    "message": null
                }
            ]
        }"#;

        let artifact: RunResultsArtifact = serde_json::from_str(raw_json).unwrap();
        assert_eq!(artifact.results.len(), 2);

        let first = &artifact.results[0];
        assert_eq!(first.status, "error");
        assert_eq!(first.unique_id, "model.project.my_model");
        assert_eq!(first.message.as_deref(), Some("BigQuery adapter error"));

        let second = &artifact.results[1];
        assert_eq!(second.status, "warn");
        assert_eq!(second.message, None);
    }

    #[test]
    fn test_sources_deserialization() {
        // Strict unit test for the sources.json freshness schema
        let raw_json = r#"{
            "results": [
                {
                    "status": "error",
                    "unique_id": "source.project.raw_events",
                    "max_loaded_at_time_ago_in_s": 259200.0
                }
            ]
        }"#;

        let artifact: SourcesArtifact = serde_json::from_str(raw_json).unwrap();
        assert_eq!(artifact.results.len(), 1);

        let first = &artifact.results[0];
        assert_eq!(first.status, "error");
        assert_eq!(first.unique_id, "source.project.raw_events");
        assert_eq!(first.max_loaded_at_time_ago_in_s, Some(259200.0));
    }

    #[test]
    fn test_get_run_specific() {
        let client = ApiClient::new().unwrap();

        // This is the specific run ID from your plan
        let run_id = 52843050;

        match client.get_run(run_id) {
            Ok(run) => {
                println!("Successfully fetched run!");
                println!("Run ID: {}", run.id);
                println!("Status: {}", run.status_humanized);
                if let Some(steps) = &run.run_steps {
                    println!("Found {} steps", steps.len());
                } else {
                    println!("Warning: No steps found in response!");
                }
            }
            Err(e) => {
                println!("API Request Failed!");
                println!("Error details: {:#?}", e);
                panic!("Test failed due to API error");
            }
        }
    }

    #[test]
    fn test_list_runs() {
        let client = ApiClient::new().unwrap();

        match client.list_runs(5, None) {
            Ok(runs) => {
                assert!(runs.len() <= 5, "Should return at most 5 runs");
                if let Some(first_run) = runs.first() {
                    println!("First run ID: {}", first_run.id);
                    println!("First run branch: {:?}", first_run.git_branch);
                    // Just ensuring it deserializes correctly
                }
            }
            Err(e) => {
                panic!("API Request Failed: {:#?}", e);
            }
        }
    }
}

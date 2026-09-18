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
pub struct RunsResponse {
    pub data: Vec<Run>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Trigger {
}

#[derive(Debug, Deserialize, Clone)]
pub struct Job {
    pub name: Option<String>
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct RunStep {
    pub name: String,
    #[serde(default)]
    pub logs: String,
    pub status: i32,
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
    token: String,
    account_id: String,
    host: String,
}

impl ApiClient {
    pub fn new() -> Result<Self> {
        let home = std::env::var("HOME").context("Could not find HOME directory")?;
        let config_path = PathBuf::from(home).join(".dbt/dbt_cloud.yml");
        let config_str = fs::read_to_string(config_path).context("Could not read ~/.dbt/dbt_cloud.yml")?;
        let config: DbtCloudConfig = serde_yaml::from_str(&config_str).context("Could not parse dbt_cloud.yml")?;

        let project = config.projects.first().context("No projects found in dbt_cloud.yml")?;
        let token = project.token_value.clone();
        let account_id = project.account_id.clone().unwrap_or_else(|| "517".to_string());
        let host = project.account_host.clone().unwrap_or_else(|| "emea.dbt.com".to_string());

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        Ok(Self {
            client,
            token,
            account_id,
            host,
        })
    }

    fn base_url(&self) -> String {
        format!("https://{}/api/v2/accounts/{}", self.host, self.account_id)
    }

    pub fn get_run(&self, run_id: i64) -> Result<Run> {
        let url = format!("{}/runs/{}/?include_related=[\"run_steps\"]", self.base_url(), run_id);
        let resp = self.client.get(&url)
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
        let mut url = format!("{}/runs/?order_by=-id&limit={}&include_related=[\"job\"]", self.base_url(), limit);
        if let Some(id) = job_id {
            url.push_str(&format!("&job_definition_id={}", id));
        }

        let resp = self.client.get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .send()?
            .error_for_status()?
            .json::<RunsResponse>()?;
        
        Ok(resp.data)
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

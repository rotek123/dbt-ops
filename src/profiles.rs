use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Profile {
    pub target: Option<String>,
    #[serde(default)]
    pub outputs: BTreeMap<String, Output>,
}

#[derive(Debug, Deserialize)]
pub struct Output {
    #[serde(rename = "type")]
    pub adapter: Option<String>,
}

/// Locate the directory containing `profiles.yml`, following dbt's precedence:
/// `--profiles-dir`, `DBT_PROFILES_DIR`, the current directory, then `~/.dbt`.
pub fn resolve_profiles_dir(flag: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = flag {
        return Ok(dir.to_path_buf());
    }
    if let Some(dir) = std::env::var_os("DBT_PROFILES_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let cwd = std::env::current_dir().context("Could not read current directory")?;
    if cwd.join("profiles.yml").is_file() {
        return Ok(cwd);
    }
    let home = std::env::var("HOME").context("Could not find HOME directory")?;
    Ok(PathBuf::from(home).join(".dbt"))
}

/// Parse `profiles.yml`. Only structural fields are read, so credentials in
/// the file are never deserialized into memory or printed.
pub fn load_profiles(dir: &Path) -> Result<BTreeMap<String, Profile>> {
    let path = dir.join("profiles.yml");
    let contents =
        fs::read_to_string(&path).with_context(|| format!("Could not read {}", path.display()))?;
    parse_profiles(&contents).with_context(|| format!("Could not parse {}", path.display()))
}

fn parse_profiles(contents: &str) -> Result<BTreeMap<String, Profile>> {
    let raw: BTreeMap<String, serde_yaml::Value> = serde_yaml::from_str(contents)?;
    let mut profiles = BTreeMap::new();
    for (name, value) in raw {
        // dbt allows a top-level `config:` block that is not a profile.
        if name == "config" {
            continue;
        }
        let profile: Profile =
            serde_yaml::from_value(value).with_context(|| format!("Invalid profile '{}'", name))?;
        profiles.insert(name, profile);
    }
    Ok(profiles)
}

pub fn print_profiles(flag: Option<&Path>, profile_filter: Option<&str>) -> Result<()> {
    let dir = resolve_profiles_dir(flag)?;
    let profiles = load_profiles(&dir)?;

    println!("profiles.yml: {}", dir.join("profiles.yml").display());

    if let Some(name) = profile_filter
        && !profiles.contains_key(name)
    {
        bail!("Profile '{}' not found", name);
    }

    println!("{:<25} {:<20} {:<12} DEFAULT", "PROFILE", "TARGET", "TYPE");
    for (name, profile) in profiles
        .iter()
        .filter(|(n, _)| profile_filter.is_none_or(|f| f == n.as_str()))
    {
        for (target, output) in &profile.outputs {
            let is_default = profile.target.as_deref() == Some(target.as_str());
            println!(
                "{:<25} {:<20} {:<12} {}",
                name,
                target,
                output.adapter.as_deref().unwrap_or("unknown"),
                if is_default { "*" } else { "" }
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
config:
  send_anonymous_usage_stats: false
my_project:
  target: dev
  outputs:
    dev:
      type: bigquery
      project: my-gcp-project
    prod:
      type: bigquery
      project: my-gcp-prod
"#;

    #[test]
    fn parses_profiles_and_skips_config_block() {
        let profiles = parse_profiles(SAMPLE).unwrap();
        assert_eq!(profiles.len(), 1);
        let p = &profiles["my_project"];
        assert_eq!(p.target.as_deref(), Some("dev"));
        assert_eq!(p.outputs.len(), 2);
        assert_eq!(p.outputs["prod"].adapter.as_deref(), Some("bigquery"));
    }

    #[test]
    fn flag_takes_precedence_over_everything() {
        let dir = Path::new("/some/dir");
        assert_eq!(resolve_profiles_dir(Some(dir)).unwrap(), dir);
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = std::env::temp_dir().join("dbt-ops-no-profiles-here");
        assert!(load_profiles(&dir).is_err());
    }
}

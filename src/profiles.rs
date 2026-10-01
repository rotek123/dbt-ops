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
    let env_dir = std::env::var_os("DBT_PROFILES_DIR").filter(|d| !d.is_empty());
    let cwd = std::env::current_dir().context("Could not read current directory")?;
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    pick_profiles_dir(
        flag,
        env_dir.map(PathBuf::from),
        &cwd,
        home.map(PathBuf::from),
    )
}

fn pick_profiles_dir(
    flag: Option<&Path>,
    env_dir: Option<PathBuf>,
    cwd: &Path,
    home: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(dir) = flag {
        return Ok(dir.to_path_buf());
    }
    if let Some(dir) = env_dir {
        return Ok(dir);
    }
    if cwd.join("profiles.yml").is_file() {
        return Ok(cwd.to_path_buf());
    }
    let home = home.context("Could not find HOME directory")?;
    Ok(home.join(".dbt"))
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

    let selected: Vec<_> = profiles
        .iter()
        .filter(|(n, _)| profile_filter.is_none_or(|f| f == n.as_str()))
        .collect();
    let width = |header: &str, widest: usize| widest.max(header.len());
    let name_w = width(
        "PROFILE",
        selected.iter().map(|(n, _)| n.len()).max().unwrap_or(0),
    );
    let target_w = width(
        "TARGET",
        selected
            .iter()
            .flat_map(|(_, p)| p.outputs.keys().map(|t| t.len()))
            .max()
            .unwrap_or(0),
    );

    println!(
        "{:<name_w$} {:<target_w$} {:<12} DEFAULT",
        "PROFILE", "TARGET", "TYPE"
    );
    for (name, profile) in selected {
        if profile.outputs.is_empty() {
            println!("{:<name_w$} (no targets)", name);
        }
        for (target, output) in &profile.outputs {
            // Compared as written, so a Jinja `target:` (e.g. env_var) is never marked
            let is_default = profile.target.as_deref() == Some(target.as_str());
            println!(
                "{:<name_w$} {:<target_w$} {:<12} {}",
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
        let env = Some(PathBuf::from("/env"));
        assert_eq!(
            pick_profiles_dir(Some(dir), env, Path::new("/cwd"), None).unwrap(),
            dir
        );
    }

    #[test]
    fn env_dir_beats_cwd_and_home() {
        let env = Some(PathBuf::from("/env"));
        let home = Some(PathBuf::from("/home/u"));
        let got = pick_profiles_dir(None, env, Path::new("/cwd"), home).unwrap();
        assert_eq!(got, Path::new("/env"));
    }

    #[test]
    fn cwd_used_when_it_has_profiles_yml_else_home() {
        let tmp = std::env::temp_dir().join("dbt-ops-cwd-profiles");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("profiles.yml"), SAMPLE).unwrap();
        let home = Some(PathBuf::from("/home/u"));
        assert_eq!(
            pick_profiles_dir(None, None, &tmp, home.clone()).unwrap(),
            tmp
        );

        let empty = std::env::temp_dir().join("dbt-ops-cwd-no-profiles");
        fs::create_dir_all(&empty).unwrap();
        let got = pick_profiles_dir(None, None, &empty, home).unwrap();
        assert_eq!(got, Path::new("/home/u/.dbt"));
        assert!(pick_profiles_dir(None, None, &empty, None).is_err());
    }

    #[test]
    fn missing_file_is_an_error() {
        let dir = std::env::temp_dir().join("dbt-ops-no-profiles-here");
        assert!(load_profiles(&dir).is_err());
    }

    #[test]
    fn parses_multiple_profiles_and_empty_outputs() {
        let profiles = parse_profiles(
            "a:\n  target: dev\n  outputs:\n    dev: {type: duckdb}\nb:\n  target: x\n",
        )
        .unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles["a"].outputs.len(), 1);
        assert!(profiles["b"].outputs.is_empty());
    }

    #[test]
    fn empty_file_has_no_profiles_and_non_mapping_profile_errors() {
        assert!(parse_profiles("").unwrap().is_empty());
        assert!(parse_profiles("a: just-a-string\n").is_err());
    }

    #[test]
    fn print_profiles_filters_and_rejects_unknown_profile() {
        let dir = std::env::temp_dir().join("dbt-ops-print-filter");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("profiles.yml"), SAMPLE).unwrap();
        assert!(print_profiles(Some(&dir), Some("my_project")).is_ok());
        assert!(print_profiles(Some(&dir), Some("nope")).is_err());
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Lightweight metadata extracted from a scenario TOML file,
/// used for listing in the web UI without full deserialisation.
#[derive(Debug, Serialize)]
pub struct ScenarioInfo {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub execution_mode: String,
}

/// List all `.toml` scenario files in the given directory.
pub fn list_scenarios(dir: &Path) -> Vec<ScenarioInfo> {
    let mut scenarios: Vec<ScenarioInfo> = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return scenarios,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        if let Some(info) = load_info(&path) {
            scenarios.push(info);
        }
    }

    scenarios.sort_by(|a, b| a.name.cmp(&b.name));
    scenarios
}

/// Read the raw TOML content of a scenario file.
pub fn read_scenario_raw(dir: &Path, name: &str) -> Result<String, String> {
    let path = dir.join(format!("{name}.toml"));
    fs::read_to_string(&path).map_err(|e| format!("Cannot read scenario: {e}"))
}

/// Write a TOML scenario file, validating that it parses correctly first.
pub fn save_scenario(dir: &Path, name: &str, content: &str) -> Result<(), String> {
    // Validate that the content parses as a valid ScenarioConfig.
    let _: crate::models::ScenarioConfig =
        toml::from_str(content).map_err(|e| format!("Invalid TOML: {e}"))?;

    let path = dir.join(format!("{name}.toml"));
    // Also ensure the dir exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create directory: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Cannot write file: {e}"))?;
    Ok(())
}

/// Delete a scenario file.
pub fn delete_scenario(dir: &Path, name: &str) -> Result<(), String> {
    let path = dir.join(format!("{name}.toml"));
    fs::remove_file(&path).map_err(|e| format!("Cannot delete scenario: {e}"))
}

// ── helpers ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_check_scenarios_directory() {
        let dir = PathBuf::from("scenarios");
        let canonical = std::fs::canonicalize(&dir).unwrap();
        eprintln!("DEBUG: canonical scenarios dir = {}", canonical.display());

        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                let files: Vec<_> = entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path().display().to_string())
                    .collect();
                eprintln!("DEBUG: files in dir: {files:?}");
            }
            Err(e) => eprintln!("DEBUG: read_dir error: {e}"),
        }

        assert!(dir.exists(), "scenarios/ directory should exist");
    }

    #[test]
    fn list_scenarios_finds_toml_files() {
        let dir = PathBuf::from("scenarios");
        let scenarios = list_scenarios(&dir);
        assert!(
            !scenarios.is_empty(),
            "list_scenarios('scenarios/') returned empty — expected at least scenario-mc and scenario-st"
        );
        let names: Vec<&str> = scenarios.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names.contains(&"scenario-mc"),
            "should find scenario-mc, got {names:?}"
        );
        assert!(
            names.contains(&"scenario-st"),
            "should find scenario-st, got {names:?}"
        );
    }

    #[test]
    fn load_info_debug_trace() {
        let path = PathBuf::from("scenarios/scenario-mc.toml");
        assert!(path.exists(), "file should exist: {}", path.display());

        let content = std::fs::read_to_string(&path).expect("should read file content");

        let value: toml::Value = toml::from_str(&content).expect("content should parse as TOML");

        let scenario = value
            .get("scenario")
            .expect("TOML should have [scenario] section");

        let stem = path.file_stem().expect("path should have a stem");
        let _name = stem.to_str().expect("stem should be valid UTF-8");

        let display_name = scenario
            .get("name")
            .expect("[scenario] should have 'name'")
            .as_str()
            .expect("name should be a string");

        assert_eq!(
            display_name,
            "Operation Distant Thunder - Multi-Axis Strike"
        );
    }

    #[test]
    fn load_info_parses_valid_toml() {
        let path = PathBuf::from("scenarios/scenario-mc.toml");
        let info = load_info(&path).expect("load_info should parse scenario-mc.toml");
        assert_eq!(info.name, "scenario-mc");
        assert_eq!(
            info.display_name,
            "Operation Distant Thunder - Multi-Axis Strike"
        );
    }
}

fn load_info(path: &Path) -> Option<ScenarioInfo> {
    let content = fs::read_to_string(path).ok()?;
    // Use toml::from_str for document-level parsing (Value::from_str parses
    // single values only in toml 1.x, so `[scenario]` would be rejected).
    let value: toml::Value = toml::from_str(&content).ok()?;

    let scenario = value.get("scenario")?;
    let name = path.file_stem()?.to_str()?.to_string();
    let display_name = scenario.get("name")?.as_str()?.to_string();
    let description = scenario
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();
    let execution_mode = scenario
        .get("execution_mode")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    Some(ScenarioInfo {
        name,
        display_name,
        description,
        execution_mode,
    })
}

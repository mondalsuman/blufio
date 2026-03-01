// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Scaffold generator for `blufio skill init`.
//!
//! Creates a new Rust WASM skill project with the correct structure:
//! ```text
//! {name}/
//! +-- Cargo.toml        # Rust lib crate targeting wasm32-wasip1
//! +-- src/
//! |   +-- lib.rs        # Minimal skill with run() export
//! +-- skill.toml        # Manifest with empty capabilities
//! ```

use std::path::Path;

use blufio_core::BlufioError;

/// Scaffolds a new Blufio skill project.
///
/// Creates a directory at `{target_dir}/{name}` containing:
/// - `Cargo.toml` configured for `wasm32-wasip1` target
/// - `src/lib.rs` with a minimal `run()` export
/// - `skill.toml` manifest with the skill name and default resources
///
/// Returns an error if the directory already exists or cannot be created.
pub fn scaffold_skill(name: &str, target_dir: &Path) -> Result<(), BlufioError> {
    // Validate skill name.
    if name.is_empty() {
        return Err(BlufioError::Skill {
            message: "skill name must not be empty".to_string(),
            source: None,
        });
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(BlufioError::Skill {
            message: format!(
                "skill name '{name}' contains invalid characters \
                 (only alphanumeric, hyphens, underscores allowed)"
            ),
            source: None,
        });
    }

    let skill_dir = target_dir.join(name);
    if skill_dir.exists() {
        return Err(BlufioError::Skill {
            message: format!("directory '{}' already exists", skill_dir.display()),
            source: None,
        });
    }

    // Create directory structure.
    let src_dir = skill_dir.join("src");
    std::fs::create_dir_all(&src_dir).map_err(|e| BlufioError::Skill {
        message: format!("failed to create directory '{}': {e}", src_dir.display()),
        source: Some(Box::new(e)),
    })?;

    // Generate Cargo.toml.
    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Add skill SDK dependencies here
"#
    );
    write_file(&skill_dir.join("Cargo.toml"), &cargo_toml)?;

    // Generate src/lib.rs.
    // Use the Rust crate name (hyphens replaced with underscores) for the doc comment.
    let lib_rs = format!(
        r#"//! {name} - A Blufio skill
//!
//! Build: cargo build --target wasm32-wasip1 --release
//! Install: blufio skill install target/wasm32-wasip1/release/{underscored_name}.wasm

#[no_mangle]
pub extern "C" fn run() {{
    // Skill implementation here.
    // Use the blufio host functions to interact with the agent:
    //   - log(level, ptr, len)       -- emit log output
    //   - get_input_len() -> i32     -- get input JSON length
    //   - get_input(ptr)             -- read input JSON into memory
    //   - set_output(ptr, len)       -- write result JSON to host
}}
"#,
        name = name,
        underscored_name = name.replace('-', "_"),
    );
    write_file(&skill_dir.join("src").join("lib.rs"), &lib_rs)?;

    // Generate skill.toml.
    let skill_toml = format!(
        r#"[skill]
name = "{name}"
version = "0.1.0"
description = "A Blufio skill"

# Capabilities this skill needs (all empty = no permissions).
[capabilities]
# network.domains = ["api.example.com"]
# filesystem.read = ["/tmp/cache"]
# filesystem.write = ["/tmp/cache"]
# env = ["MY_API_KEY"]

# Resource limits for the WASM sandbox.
[resources]
# fuel = 1_000_000_000
# memory_mb = 16
# epoch_timeout_secs = 5

[wasm]
entry = "{underscored_name}.wasm"
"#,
        name = name,
        underscored_name = name.replace('-', "_"),
    );
    write_file(&skill_dir.join("skill.toml"), &skill_toml)?;

    Ok(())
}

/// Helper to write a file, converting IO errors to BlufioError.
fn write_file(path: &Path, content: &str) -> Result<(), BlufioError> {
    std::fs::write(path, content).map_err(|e| BlufioError::Skill {
        message: format!("failed to write '{}': {e}", path.display()),
        source: Some(Box::new(e)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_creates_directory_structure() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("my-skill", tmp.path()).unwrap();

        let skill_dir = tmp.path().join("my-skill");
        assert!(skill_dir.exists());
        assert!(skill_dir.join("Cargo.toml").exists());
        assert!(skill_dir.join("src").join("lib.rs").exists());
        assert!(skill_dir.join("skill.toml").exists());
    }

    #[test]
    fn scaffold_cargo_toml_contains_name() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("weather-lookup", tmp.path()).unwrap();

        let cargo = std::fs::read_to_string(tmp.path().join("weather-lookup/Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"weather-lookup\""));
        assert!(cargo.contains("[lib]"));
        assert!(cargo.contains("cdylib"));
    }

    #[test]
    fn scaffold_lib_rs_contains_run_export() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("test-skill", tmp.path()).unwrap();

        let lib = std::fs::read_to_string(tmp.path().join("test-skill/src/lib.rs")).unwrap();
        assert!(lib.contains("#[no_mangle]"));
        assert!(lib.contains("pub extern \"C\" fn run()"));
        assert!(lib.contains("test_skill.wasm"));
    }

    #[test]
    fn scaffold_skill_toml_contains_name() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("my-tool", tmp.path()).unwrap();

        let manifest = std::fs::read_to_string(tmp.path().join("my-tool/skill.toml")).unwrap();
        assert!(manifest.contains("name = \"my-tool\""));
        assert!(manifest.contains("[capabilities]"));
        assert!(manifest.contains("[resources]"));
    }

    #[test]
    fn scaffold_empty_name_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let result = scaffold_skill("", tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must not be empty"));
    }

    #[test]
    fn scaffold_invalid_name_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let result = scaffold_skill("bad name!", tmp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn scaffold_duplicate_directory_fails() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("dupe", tmp.path()).unwrap();

        let result = scaffold_skill("dupe", tmp.path());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already exists"));
    }

    #[test]
    fn scaffold_generated_skill_toml_is_parseable() {
        let tmp = tempfile::tempdir().unwrap();
        scaffold_skill("parseable-skill", tmp.path()).unwrap();

        let toml_content =
            std::fs::read_to_string(tmp.path().join("parseable-skill/skill.toml")).unwrap();
        // The generated skill.toml should be parseable by our manifest parser.
        let result = crate::parse_manifest(&toml_content);
        assert!(result.is_ok(), "Generated skill.toml failed to parse: {result:?}");
        let manifest = result.unwrap();
        assert_eq!(manifest.name, "parseable-skill");
    }
}

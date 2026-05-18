/*
 * Integration tests for jj-spr workspace config support
 * Tests that config set in main workspace is accessible from secondary workspaces
 */

use std::process::Command;
use tempfile::tempdir;

// Import production code for setting config
use jj_spr::config::set_jj_config;

fn create_jj_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let path = temp_dir.path().to_path_buf();

    // Initialize git repo first
    Command::new("git")
        .args(["init"])
        .current_dir(&path)
        .output()
        .expect("Failed to init git repo");

    // Initialize jj repo (colocated)
    let jj_init = Command::new("jj")
        .args(["git", "init", "--colocate"])
        .current_dir(&path)
        .output()
        .expect("Failed to init jj repo");

    if !jj_init.status.success() {
        panic!("jj not available");
    }

    // Configure user for jj
    Command::new("jj")
        .args(["config", "set", "--repo", "user.name", "Test User"])
        .current_dir(&path)
        .output()
        .expect("Failed to set user.name");

    Command::new("jj")
        .args(["config", "set", "--repo", "user.email", "test@example.com"])
        .current_dir(&path)
        .output()
        .expect("Failed to set user.email");

    (temp_dir, path)
}

// Wrapper around production set_jj_config for test convenience
fn set_config_for_test(repo_path: &std::path::Path, key: &str, value: &str) {
    set_jj_config(key, value, repo_path)
        .unwrap_or_else(|e| panic!("Failed to set config {}: {}", key, e));
}

fn get_jj_config(repo_path: &std::path::Path, key: &str) -> Option<String> {
    let output = Command::new("jj")
        .args(["config", "get", key])
        .current_dir(repo_path)
        .output()
        .expect("Failed to get jj config");

    if output.status.success() {
        let value = String::from_utf8(output.stdout).ok()?;
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            Some(trimmed.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

// ============================================================================
// WORKSPACE CONFIG TESTS
// ============================================================================

#[test]
fn test_config_readable_in_secondary_workspace() {
    let (temp_dir, main_repo_path) = create_jj_repo();

    // Set config in main workspace
    set_config_for_test(&main_repo_path, "spr.githubRepository", "owner/repo");
    set_config_for_test(&main_repo_path, "spr.branchPrefix", "spr/test/");
    set_config_for_test(&main_repo_path, "spr.githubRemoteName", "origin");
    set_config_for_test(&main_repo_path, "spr.githubMasterBranch", "main");
    set_config_for_test(&main_repo_path, "spr.requireApproval", "false");

    // Verify config is readable from main workspace
    assert_eq!(
        get_jj_config(&main_repo_path, "spr.githubRepository"),
        Some("owner/repo".to_string())
    );
    assert_eq!(
        get_jj_config(&main_repo_path, "spr.branchPrefix"),
        Some("spr/test/".to_string())
    );

    // Create secondary workspace within the same temp directory
    let workspace2_path = temp_dir.path().join("ws-readable-test");
    let workspace_output = Command::new("jj")
        .args(["workspace", "add", workspace2_path.to_str().unwrap()])
        .current_dir(&main_repo_path)
        .output()
        .expect("Failed to create workspace");

    assert!(
        workspace_output.status.success(),
        "Failed to create workspace: {}",
        String::from_utf8_lossy(&workspace_output.stderr)
    );

    // Verify config is readable from secondary workspace
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.githubRepository"),
        Some("owner/repo".to_string()),
        "Config should be readable from secondary workspace"
    );
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.branchPrefix"),
        Some("spr/test/".to_string()),
        "Config should be readable from secondary workspace"
    );
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.githubRemoteName"),
        Some("origin".to_string()),
        "Config should be readable from secondary workspace"
    );
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.githubMasterBranch"),
        Some("main".to_string()),
        "Config should be readable from secondary workspace"
    );
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.requireApproval"),
        Some("false".to_string()),
        "Config should be readable from secondary workspace"
    );
}

#[test]
fn test_spr_commands_work_from_secondary_workspace() {
    let (temp_dir, main_repo_path) = create_jj_repo();

    // Set minimal config required for spr commands
    set_config_for_test(&main_repo_path, "spr.githubRepository", "owner/repo");
    set_config_for_test(&main_repo_path, "spr.branchPrefix", "spr/test/");

    // Create secondary workspace within the same temp directory
    let workspace2_path = temp_dir.path().join("ws-commands-test");
    let workspace_output = Command::new("jj")
        .args(["workspace", "add", workspace2_path.to_str().unwrap()])
        .current_dir(&main_repo_path)
        .output()
        .expect("Failed to create workspace");

    assert!(
        workspace_output.status.success(),
        "Failed to create workspace: {}",
        String::from_utf8_lossy(&workspace_output.stderr)
    );

    // Try running spr list from secondary workspace
    // This command should read config successfully (even if it fails for other reasons like no auth token)
    let spr_output = Command::new(env!("CARGO_BIN_EXE_jj-spr"))
        .args(["list"])
        .current_dir(&workspace2_path)
        .output()
        .expect("Failed to run spr list from workspace");

    let stderr = String::from_utf8_lossy(&spr_output.stderr);
    let stdout = String::from_utf8_lossy(&spr_output.stdout);

    // The command should be able to read config (not fail with config error)
    // It may fail with auth errors or network errors, but not config errors
    assert!(
        !stderr.contains("spr.githubRepository must be configured")
            && !stderr.contains("spr.branchPrefix must be configured"),
        "spr should be able to read config from workspace. stderr: {}, stdout: {}",
        stderr,
        stdout
    );
}

#[test]
fn test_config_set_in_workspace_is_shared() {
    let (temp_dir, main_repo_path) = create_jj_repo();

    // Create secondary workspace within the same temp directory
    let workspace2_path = temp_dir.path().join("ws-shared-test");
    let workspace_output = Command::new("jj")
        .args(["workspace", "add", workspace2_path.to_str().unwrap()])
        .current_dir(&main_repo_path)
        .output()
        .expect("Failed to create workspace");

    assert!(workspace_output.status.success());

    // Set config from secondary workspace
    set_config_for_test(
        &workspace2_path,
        "spr.githubRepository",
        "owner/repo-from-workspace2",
    );

    // Verify config is readable from main workspace
    assert_eq!(
        get_jj_config(&main_repo_path, "spr.githubRepository"),
        Some("owner/repo-from-workspace2".to_string()),
        "Config set in secondary workspace should be readable from main workspace"
    );

    // Verify config is readable from secondary workspace itself
    assert_eq!(
        get_jj_config(&workspace2_path, "spr.githubRepository"),
        Some("owner/repo-from-workspace2".to_string()),
        "Config should be readable from workspace where it was set"
    );
}

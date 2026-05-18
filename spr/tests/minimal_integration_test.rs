/*
 * Minimal integration tests for jj-spr
 * Tests only the functionality that definitely works without external dependencies
 */

use std::{fs::create_dir, process::Command};
use tempfile::tempdir;

fn run_jj_spr(args: &[&str], working_dir: Option<&std::path::Path>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jj-spr"));
    cmd.args(args);

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.output().expect("Failed to run jj-spr command")
}

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

    (temp_dir, path)
}

// ============================================================================
// CORE FUNCTIONALITY TESTS - These MUST work
// ============================================================================

#[test]
fn test_binary_exists_and_has_correct_identity() {
    // Test that the binary exists and identifies correctly
    let output = run_jj_spr(&["--version"], None);
    assert!(
        output.status.success(),
        "Binary should respond to --version"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("jj-spr"), "Should identify as jj-spr");
    assert!(stdout.contains("0.1.0"), "Should show correct version");
}

#[test]
fn test_help_shows_jujutsu_subcommand_identity() {
    let output = run_jj_spr(&["--help"], None);
    assert!(output.status.success(), "Should show help");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Jujutsu subcommand"),
        "Should identify as Jujutsu subcommand"
    );

    // Should list all main commands
    let commands = vec![
        "diff", "format", "land", "amend", "close", "list", "patch", "init",
    ];
    for cmd in commands {
        assert!(stdout.contains(cmd), "Help should mention {} command", cmd);
    }
}

#[test]
fn test_subcommands_have_help() {
    let commands = vec![
        ("diff", "Pull Request"),
        ("format", "commit message"),
        ("land", "Pull Request"),
        ("amend", "commit message"),
        ("close", "Pull request"),
        ("list", "Pull Requests"),
        ("patch", "branch"),
        ("init", "assistant"),
    ];

    for (cmd, expected_keyword) in commands {
        let output = run_jj_spr(&[cmd, "--help"], None);
        assert!(output.status.success(), "Command {} should have help", cmd);

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout
                .to_lowercase()
                .contains(&expected_keyword.to_lowercase()),
            "Help for {} should mention {}",
            cmd,
            expected_keyword
        );
    }
}

#[test]
fn test_requires_jujutsu_repository() {
    // Test without any repository
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let output = run_jj_spr(&["format"], Some(temp_dir.path()));
    assert!(
        !output.status.success(),
        "Should fail without any repository"
    );

    // Test with only git repository (no jj)
    Command::new("git")
        .args(["init"])
        .current_dir(temp_dir.path())
        .output()
        .expect("Failed to init git repo");

    let output = run_jj_spr(&["format"], Some(temp_dir.path()));
    assert!(
        !output.status.success(),
        "Should fail with only git repository"
    );

    // The error should mention Jujutsu
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        all_output.contains("Jujutsu") || all_output.contains("jj"),
        "Should mention Jujutsu requirement"
    );
}

#[test]
fn test_error_handling_is_graceful() {
    // Test invalid subcommand
    let output = run_jj_spr(&["nonexistent-command"], None);
    assert!(!output.status.success(), "Invalid command should fail");

    // Should not crash or panic
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all_output.is_empty(), "Should produce some error message");

    // Test incomplete argument
    let output = run_jj_spr(&["--revision"], None);
    assert!(!output.status.success(), "Incomplete argument should fail");
}

// ============================================================================
// JUJUTSU INTEGRATION TESTS - These should work with jj repo
// ============================================================================

#[test]
fn test_accepts_jujutsu_repository() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // Commands should at least get past repository detection
    // (they may fail later due to configuration, but not due to repo detection)
    let output = run_jj_spr(&["format"], Some(&repo_path));

    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should NOT fail due to repository detection
    assert!(
        !all_output.contains("This command requires a Jujutsu repository"),
        "Should accept jujutsu repository"
    );

    // Ensure running from a subdirectory works as well
    let subdir = repo_path.join("foo");
    create_dir(&subdir).unwrap();
    let output = run_jj_spr(&["format"], Some(&subdir));
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        !all_output.contains("This command requires a Jujutsu repository"),
        "Should accept jujutsu repository"
    );
}

#[test]
fn test_configuration_detection() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // Without configuration, should fail with config-related error
    let output = run_jj_spr(&["format"], Some(&repo_path));
    assert!(
        !output.status.success(),
        "Should fail without configuration"
    );

    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should mention configuration requirement
    assert!(
        all_output.contains("config")
            || all_output.contains("githubRepository")
            || all_output.contains("not found"),
        "Should mention configuration requirement"
    );
}

#[test]
fn test_revision_parameter_is_recognized() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // Test that revision parameter is recognized (even if command fails later)
    let output = run_jj_spr(&["format", "--revision", "@"], Some(&repo_path));

    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should NOT fail due to unrecognized argument
    assert!(
        !all_output.contains("unrecognized") && !all_output.contains("invalid argument"),
        "Should recognize --revision parameter"
    );
}

#[test]
fn test_global_revision_parameter() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // Test global revision parameter
    let output = run_jj_spr(&["--revision", "@", "format"], Some(&repo_path));

    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should NOT fail due to argument parsing
    assert!(
        !all_output.contains("unrecognized") && !all_output.contains("invalid argument"),
        "Should recognize global --revision parameter"
    );
}

// ============================================================================
// BEHAVIORAL TESTS - Test that commands behave as expected
// ============================================================================

#[test]
fn test_commands_fail_appropriately_without_config() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // These commands should all fail without GitHub configuration
    let github_commands = vec!["diff", "format", "land", "amend", "close"];

    for cmd in github_commands {
        let output = run_jj_spr(&[cmd], Some(&repo_path));
        assert!(
            !output.status.success(),
            "Command {} should fail without config",
            cmd
        );

        // Should fail due to configuration, not other reasons
        let all_output = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            all_output.contains("config")
                || all_output.contains("not found")
                || all_output.contains("githubRepository"),
            "Command {} should fail due to missing configuration",
            cmd
        );
    }
}

#[test]
fn test_list_command_behavior() {
    let (_temp_dir, repo_path) = create_jj_repo();

    // List command should fail appropriately
    let output = run_jj_spr(&["list"], Some(&repo_path));
    assert!(!output.status.success(), "List should fail without config");

    // Should not crash
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all_output.is_empty(), "Should produce error message");
}

// ============================================================================
// INTEGRATION SUMMARY TEST
// ============================================================================

#[test]
fn test_jj_spr_integration_summary() {
    println!("🧪 Testing jj-spr integration...");

    // 1. Binary works
    let output = run_jj_spr(&["--version"], None);
    assert!(output.status.success());
    println!("✅ Binary responds to --version");

    // 2. Shows correct identity
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("jj-spr"));
    println!("✅ Identifies as jj-spr binary");

    // Check help for Jujutsu identity
    let help_output = run_jj_spr(&["--help"], None);
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(help_stdout.contains("Jujutsu subcommand"));
    println!("✅ Identifies as Jujutsu subcommand");

    // 3. Repository detection works
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let output = run_jj_spr(&["format"], Some(temp_dir.path()));
    assert!(!output.status.success());
    println!("✅ Rejects non-Jujutsu repositories");

    // 4. Accepts Jujutsu repositories
    let (_temp_dir, repo_path) = create_jj_repo();
    let output = run_jj_spr(&["format"], Some(&repo_path));
    let all_output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all_output.contains("This command requires a Jujutsu repository"));
    println!("✅ Accepts Jujutsu repositories");

    // 5. Configuration system works
    assert!(all_output.contains("config") || all_output.contains("not found"));
    println!("✅ Configuration system functional");

    // 6. Command structure works
    let output = run_jj_spr(&["--help"], None);
    assert!(output.status.success());
    println!("✅ Command structure functional");

    println!("🎉 jj-spr integration test complete - ready for use as Jujutsu subcommand!");
}

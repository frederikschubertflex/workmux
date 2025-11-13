from pathlib import Path


from .conftest import (
    TmuxEnvironment,
    create_commit,
    create_dirty_file,
    get_window_name,
    get_worktree_path,
    run_workmux_add,
    run_workmux_remove,
    write_workmux_config,
)


def test_remove_clean_branch_succeeds_without_prompt(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove` on a branch with no unmerged commits succeeds without a prompt."""
    env = isolated_tmux_server
    branch_name = "clean-branch"
    window_name = get_window_name(branch_name)
    write_workmux_config(repo_path)

    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)
    worktree_path = get_worktree_path(repo_path, branch_name)
    assert worktree_path.is_dir()

    # This should succeed without any user input because the branch has no new commits
    run_workmux_remove(env, workmux_exe_path, repo_path, branch_name, force=False)

    assert not worktree_path.exists()
    list_windows_result = env.tmux(["list-windows", "-F", "#{window_name}"])
    assert window_name not in list_windows_result.stdout
    branch_list_result = env.run_command(["git", "branch", "--list", branch_name])
    assert branch_name not in branch_list_result.stdout


def test_remove_unmerged_branch_with_confirmation(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove` on an unmerged branch succeeds after user confirmation."""
    env = isolated_tmux_server
    branch_name = "unmerged-branch"
    window_name = get_window_name(branch_name)
    write_workmux_config(repo_path)
    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)

    # Create a new commit to make the branch "unmerged"
    worktree_path = get_worktree_path(repo_path, branch_name)
    create_commit(env, worktree_path, "feat: new feature")

    # Run remove, piping 'y' to the confirmation prompt
    run_workmux_remove(
        env, workmux_exe_path, repo_path, branch_name, force=False, user_input="y"
    )

    assert not worktree_path.exists(), "Worktree should be removed after confirmation"
    list_windows_result = env.tmux(["list-windows", "-F", "#{window_name}"])
    assert window_name not in list_windows_result.stdout
    branch_list_result = env.run_command(["git", "branch", "--list", branch_name])
    assert branch_name not in branch_list_result.stdout


def test_remove_unmerged_branch_aborted(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove` on an unmerged branch is aborted if not confirmed."""
    env = isolated_tmux_server
    branch_name = "unmerged-aborted"
    window_name = get_window_name(branch_name)
    write_workmux_config(repo_path)
    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)

    worktree_path = get_worktree_path(repo_path, branch_name)
    create_commit(env, worktree_path, "feat: another feature")

    # Run remove, piping 'n' to abort
    run_workmux_remove(
        env, workmux_exe_path, repo_path, branch_name, force=False, user_input="n"
    )

    assert worktree_path.exists(), "Worktree should NOT be removed after aborting"
    list_windows_result = env.tmux(["list-windows", "-F", "#{window_name}"])
    assert window_name in list_windows_result.stdout
    branch_list_result = env.run_command(["git", "branch", "--list", branch_name])
    assert branch_name in branch_list_result.stdout


def test_remove_fails_on_uncommitted_changes(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove` fails if the worktree has uncommitted changes."""
    env = isolated_tmux_server
    branch_name = "dirty-worktree"
    write_workmux_config(repo_path)
    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)

    worktree_path = get_worktree_path(repo_path, branch_name)
    create_dirty_file(worktree_path)

    # This should fail because of uncommitted changes
    run_workmux_remove(
        env,
        workmux_exe_path,
        repo_path,
        branch_name,
        force=False,
        expect_fail=True,
    )

    assert worktree_path.exists(), "Worktree should not be removed when command fails"


def test_remove_with_force_on_unmerged_branch(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove -f` removes an unmerged branch without a prompt."""
    env = isolated_tmux_server
    branch_name = "force-remove-unmerged"
    write_workmux_config(repo_path)
    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)

    worktree_path = get_worktree_path(repo_path, branch_name)
    create_commit(env, worktree_path, "feat: something unmerged")

    # Force remove should succeed without interaction
    run_workmux_remove(env, workmux_exe_path, repo_path, branch_name, force=True)

    assert not worktree_path.exists(), "Worktree should be removed"


def test_remove_with_force_on_uncommitted_changes(
    isolated_tmux_server: TmuxEnvironment, workmux_exe_path: Path, repo_path: Path
):
    """Verifies `workmux remove -f` removes a worktree with uncommitted changes."""
    env = isolated_tmux_server
    branch_name = "force-remove-dirty"
    write_workmux_config(repo_path)
    run_workmux_add(env, workmux_exe_path, repo_path, branch_name)

    worktree_path = get_worktree_path(repo_path, branch_name)
    create_dirty_file(worktree_path)

    # Force remove should succeed despite uncommitted changes
    run_workmux_remove(env, workmux_exe_path, repo_path, branch_name, force=True)

    assert not worktree_path.exists(), "Worktree should be removed"

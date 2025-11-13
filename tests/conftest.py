import os
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Callable, Dict, Generator, List, Optional

import pytest
import yaml


class TmuxEnvironment:
    """
    A helper class to manage the state of an isolated test environment.
    It controls a dedicated tmux server via a private socket file.
    """

    def __init__(self, tmp_path: Path):
        # The base directory for all temporary test files
        self.tmp_path = tmp_path

        # Create a dedicated home directory for the test to prevent
        # loading the user's real shell configuration (.zshrc, .bash_history, etc.)
        self.home_path = self.tmp_path / "test_home"
        self.home_path.mkdir()

        # Use a short socket path in /tmp to avoid macOS socket path length limits
        # Create a temporary file and use its name for the socket
        tmp_file = tempfile.NamedTemporaryFile(
            prefix="tmux_", suffix=".sock", delete=False
        )
        self.socket_path = Path(tmp_file.name)
        tmp_file.close()
        self.socket_path.unlink()  # Remove the file, we just want the path

        # Create a copy of the current environment variables
        self.env = os.environ.copy()

        # Isolate the shell environment completely to prevent history pollution
        # and other side effects from user's shell configuration
        self.env["HOME"] = str(self.home_path)

        # IMPORTANT: Tell all future tmux commands to use our private socket.
        # This is the key to isolating the test from the user's live tmux session.
        self.env["TMUX_SOCKET"] = str(self.socket_path)

        # Prevent tmux from loading the user's real ~/.tmux.conf file
        self.env["TMUX_CONF"] = "/dev/null"

    def run_command(self, cmd: list[str], check: bool = True):
        """Runs a generic command within the isolated environment."""
        return subprocess.run(
            cmd,
            cwd=self.tmp_path,
            env=self.env,
            capture_output=True,
            text=True,
            check=check,
        )

    def tmux(self, tmux_args: list[str], check: bool = True):
        """
        Runs a tmux command targeting our isolated server.
        It explicitly uses the '-S' flag for clarity and robustness.
        """
        base_cmd = ["tmux", "-S", str(self.socket_path)]
        return self.run_command(base_cmd + tmux_args, check=check)


@pytest.fixture
def isolated_tmux_server(tmp_path: Path) -> Generator[TmuxEnvironment, None, None]:
    """
    A pytest fixture that provides a fully isolated tmux server for a single test.

    It performs the following steps:
    1. Creates a TmuxEnvironment instance.
    2. Starts a new, isolated tmux server process.
    3. Yields the environment manager to the test function.
    4. After the test runs, it kills the isolated tmux server for cleanup.
    """
    # 1. Setup
    test_env = TmuxEnvironment(tmp_path)

    # Start the dedicated tmux server with a new session
    # -d runs in detached mode (doesn't attach to the session)
    # -s names the session "test"
    test_env.tmux(["new-session", "-d", "-s", "test"], check=True)

    # 2. Yield control to the test function
    yield test_env

    # 3. Teardown
    # Kill the isolated server after the test is complete.
    # This will also clean up the socket file
    test_env.tmux(["kill-server"], check=False)

    # Clean up the socket file if it still exists
    if test_env.socket_path.exists():
        test_env.socket_path.unlink()


def setup_git_repo(path: Path):
    """Initializes a git repository in the given path with an initial commit."""
    subprocess.run(["git", "init"], cwd=path, check=True, capture_output=True)
    subprocess.run(
        ["git", "commit", "--allow-empty", "-m", "Initial commit"],
        cwd=path,
        check=True,
        capture_output=True,
    )


@pytest.fixture
def repo_path(isolated_tmux_server: "TmuxEnvironment") -> Path:
    """Initializes a git repo in the test env and returns its path."""
    path = isolated_tmux_server.tmp_path
    setup_git_repo(path)
    return path


def poll_until(
    condition: Callable[[], bool],
    timeout: float = 5.0,
    poll_interval: float = 0.1,
) -> bool:
    """
    Poll until a condition is met or timeout is reached.

    Args:
        condition: A callable that returns True when the condition is met
        timeout: Maximum time to wait in seconds
        poll_interval: Time to wait between checks in seconds

    Returns:
        True if condition was met, False if timeout was reached
    """
    start_time = time.time()
    while time.time() - start_time < timeout:
        if condition():
            return True
        time.sleep(poll_interval)
    return False


@pytest.fixture(scope="session")
def workmux_exe_path() -> Path:
    """
    Returns the path to the local workmux build for testing.
    """
    local_path = Path(__file__).parent.parent / "target/debug/workmux"
    if not local_path.exists():
        pytest.fail("Could not find workmux executable. Run 'cargo build' first.")
    return local_path


def write_workmux_config(
    repo_path: Path,
    panes: Optional[List[Dict[str, Any]]] = None,
    post_create: Optional[List[str]] = None,
):
    """Creates a .workmux.yaml file from structured data."""
    config: Dict[str, Any] = {"panes": panes if panes is not None else []}
    if post_create:
        config["post_create"] = post_create
    (repo_path / ".workmux.yaml").write_text(yaml.dump(config))


def get_worktree_path(repo_path: Path, branch_name: str) -> Path:
    """Returns the expected path for a worktree directory."""
    return repo_path.parent / f"{repo_path.name}__worktrees" / branch_name


def get_window_name(branch_name: str) -> str:
    """Returns the expected tmux window name for a worktree."""
    return f"wm-{branch_name}"


def run_workmux_command(
    env: TmuxEnvironment,
    workmux_exe_path: Path,
    repo_path: Path,
    command: str,
    pre_run_tmux_cmds: Optional[List[List[str]]] = None,
) -> None:
    """
    Helper to run a workmux command inside the isolated tmux session.

    Asserts that the command completes successfully.

    Args:
        env: The isolated tmux environment
        workmux_exe_path: Path to the workmux executable
        repo_path: Path to the git repository
        command: The workmux command to run (e.g., "add feature-branch")
        pre_run_tmux_cmds: Optional list of tmux commands to run before the command
    """
    stdout_file = env.tmp_path / "workmux_stdout.txt"
    stderr_file = env.tmp_path / "workmux_stderr.txt"
    exit_code_file = env.tmp_path / "workmux_exit_code.txt"

    # Clean up any previous files
    for f in [stdout_file, stderr_file, exit_code_file]:
        if f.exists():
            f.unlink()

    # Execute any pre-run setup commands in tmux
    if pre_run_tmux_cmds:
        for cmd_args in pre_run_tmux_cmds:
            env.tmux(cmd_args)

    workmux_cmd = (
        f"cd {repo_path} && "
        f"{workmux_exe_path} {command} "
        f"> {stdout_file} 2> {stderr_file}; "
        f"echo $? > {exit_code_file}"
    )

    env.tmux(["send-keys", "-t", "test:", workmux_cmd, "C-m"])

    # Wait for command to complete
    assert poll_until(exit_code_file.exists, timeout=5.0), (
        "workmux command did not complete in time"
    )

    exit_code = int(exit_code_file.read_text().strip())
    if exit_code != 0:
        stderr = stderr_file.read_text() if stderr_file.exists() else ""
        raise AssertionError(
            f"workmux {command} failed with exit code {exit_code}\n{stderr}"
        )


def run_workmux_add(
    env: TmuxEnvironment,
    workmux_exe_path: Path,
    repo_path: Path,
    branch_name: str,
    pre_run_tmux_cmds: Optional[List[List[str]]] = None,
) -> None:
    """
    Helper to run `workmux add` command inside the isolated tmux session.

    Asserts that the command completes successfully.

    Args:
        env: The isolated tmux environment
        workmux_exe_path: Path to the workmux executable
        repo_path: Path to the git repository
        branch_name: Name of the branch/worktree to create
        pre_run_tmux_cmds: Optional list of tmux commands to run before workmux add
    """
    run_workmux_command(
        env, workmux_exe_path, repo_path, f"add {branch_name}", pre_run_tmux_cmds
    )


def create_commit(env: TmuxEnvironment, path: Path, message: str):
    """Creates and commits a file within the test env at a specific path."""
    (path / f"file_for_{message.replace(' ', '_').replace(':', '')}.txt").write_text(
        f"content for {message}"
    )
    # Use subprocess directly to specify cwd easily
    subprocess.run(["git", "add", "."], cwd=path, check=True, env=env.env)
    subprocess.run(["git", "commit", "-m", message], cwd=path, check=True, env=env.env)


def create_dirty_file(path: Path, filename: str = "dirty.txt"):
    """Creates an uncommitted file."""
    (path / filename).write_text("uncommitted changes")


def run_workmux_remove(
    env: TmuxEnvironment,
    workmux_exe_path: Path,
    repo_path: Path,
    branch_name: str,
    force: bool = False,
    user_input: Optional[str] = None,
    expect_fail: bool = False,
) -> None:
    """
    Helper to run `workmux remove` command inside the isolated tmux session.

    Uses tmux run-shell -b to avoid hanging when remove kills its own window.
    Asserts that the command completes successfully unless expect_fail is True.

    Args:
        env: The isolated tmux environment
        workmux_exe_path: Path to the workmux executable
        repo_path: Path to the git repository
        branch_name: Name of the branch/worktree to remove
        force: Whether to use -f flag to skip confirmation
        user_input: Optional string to pipe to stdin (e.g., 'y' for confirmation)
        expect_fail: If True, asserts the command fails (non-zero exit code)
    """
    stdout_file = env.tmp_path / "workmux_remove_stdout.txt"
    stderr_file = env.tmp_path / "workmux_remove_stderr.txt"
    exit_code_file = env.tmp_path / "workmux_remove_exit_code.txt"

    # Clean up any previous files
    for f in [stdout_file, stderr_file, exit_code_file]:
        if f.exists():
            f.unlink()

    force_flag = "-f " if force else ""
    input_cmd = f"echo '{user_input}' | " if user_input else ""
    remove_script = (
        f"cd {repo_path} && "
        f"{input_cmd}"
        f"{workmux_exe_path} remove {force_flag}{branch_name} "
        f"> {stdout_file} 2> {stderr_file}; "
        f"echo $? > {exit_code_file}"
    )

    env.tmux(["run-shell", "-b", remove_script])

    # Wait for command to complete
    assert poll_until(exit_code_file.exists, timeout=5.0), (
        "workmux remove did not complete in time"
    )

    exit_code = int(exit_code_file.read_text().strip())
    stderr = stderr_file.read_text() if stderr_file.exists() else ""

    if expect_fail:
        if exit_code == 0:
            raise AssertionError(
                f"workmux remove was expected to fail but succeeded.\nStderr:\n{stderr}"
            )
    else:
        if exit_code != 0:
            raise AssertionError(
                f"workmux remove failed with exit code {exit_code}\nStderr:\n{stderr}"
            )

import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Callable, Generator

import pytest

class TmuxEnvironment:
    """
    A helper class to manage the state of an isolated test environment.
    It controls a dedicated tmux server via a private socket file.
    """
    def __init__(self, tmp_path: Path):
        # The base directory for all temporary test files
        self.tmp_path = tmp_path

        # Use a short socket path in /tmp to avoid macOS socket path length limits
        # Create a temporary file and use its name for the socket
        tmp_file = tempfile.NamedTemporaryFile(prefix="tmux_", suffix=".sock", delete=False)
        self.socket_path = Path(tmp_file.name)
        tmp_file.close()
        self.socket_path.unlink()  # Remove the file, we just want the path

        # Create a copy of the current environment variables
        self.env = os.environ.copy()

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

#!/usr/bin/env python3
"""Spawn a zellij session headlessly with a pseudo-TTY.

Usage: headless-zellij <session-name> <shell-script> [working-dir]

Creates a detached zellij session running the given shell as SHELL.
Attach with `zellij attach <name>`.
"""

import os
import pty
import subprocess
import sys
import signal

if len(sys.argv) < 3:
    print(
        f"Usage: {sys.argv[0]} <session-name> <shell-script> [working-dir]",
        file=sys.stderr,
    )
    sys.exit(1)

session_name = sys.argv[1]
shell_script = sys.argv[2]
cwd = sys.argv[3] if len(sys.argv) > 3 else None

master, slave = pty.openpty()

env = os.environ.copy()
env["SHELL"] = shell_script
# Strip env vars that prevent nested zellij/claude sessions
env.pop("ZELLIJ", None)
env.pop("ZELLIJ_SESSION_NAME", None)
env.pop("CLAUDECODE", None)
env.pop("CLAUDE_CODE_ENTRYPOINT", None)

proc = subprocess.Popen(
    ["zellij", "-s", session_name],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    start_new_session=True,
    cwd=cwd,
    env=env,
)

os.close(slave)

# Detach: fork so parent can return, child keeps PTY master alive
pid = os.fork()
if pid > 0:
    # Parent: close master and exit cleanly
    os.close(master)
    sys.exit(0)

# Child: keep master open, wait for zellij to finish
signal.signal(signal.SIGHUP, signal.SIG_IGN)

# Redirect own stdio to /dev/null so we're fully detached
devnull = os.open(os.devnull, os.O_RDWR)
os.dup2(devnull, 0)
os.dup2(devnull, 1)
os.dup2(devnull, 2)
os.close(devnull)

proc.wait()
os.close(master)

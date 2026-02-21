#!/usr/bin/env python3
"""Spawn a zellij session headlessly with a pseudo-TTY.

Usage: headless-zellij.py <session-name> <shell-script>

Creates a detached zellij session running the given shell as SHELL.
Attach with `zellij attach <name>`.
"""

import os
import pty
import subprocess
import sys

if len(sys.argv) < 3:
    print(f"Usage: {sys.argv[0]} <session-name> <shell-script>", file=sys.stderr)
    sys.exit(1)

session_name = sys.argv[1]
shell_script = sys.argv[2]

# Double fork to fully detach
if os.fork() > 0:
    sys.exit(0)

os.setsid()

if os.fork() > 0:
    sys.exit(0)

# We're the daemon now. Create a PTY and run zellij on it.
master, slave = pty.openpty()

env = os.environ.copy()
env["SHELL"] = shell_script

proc = subprocess.Popen(
    ["zellij", "-s", session_name],
    stdin=slave,
    stdout=slave,
    stderr=slave,
    close_fds=True,
    env=env,
)

os.close(slave)
# Keep master open while zellij runs
proc.wait()
os.close(master)

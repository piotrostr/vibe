#!/usr/bin/env python3
"""Spawn a zellij session headlessly with a pseudo-TTY.

Usage: headless-zellij <session-name> <shell-script> [working-dir]

Creates a detached zellij session running the given shell as SHELL.
Attach with `zellij attach <name>`.
"""

import fcntl
import os
import pty
import struct
import subprocess
import sys
import signal
import termios

args = [a for a in sys.argv[1:] if not a.startswith("--")]
flags = [a for a in sys.argv[1:] if a.startswith("--")]

if len(args) < 2:
    print(
        f"Usage: {sys.argv[0]} [--no-fork] <session-name> <shell-script> [working-dir]",
        file=sys.stderr,
    )
    sys.exit(1)

session_name = args[0]
shell_script = args[1]
cwd = args[2] if len(args) > 2 else None

master, slave = pty.openpty()

# Set terminal size large enough that zellij/claude never feels cramped.
# When you attach, zellij resizes to your real terminal anyway - this just
# sets the initial headless dimensions.
COLS, ROWS = 220, 150
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))

env = os.environ.copy()
env["SHELL"] = shell_script
env["TERM"] = "xterm-256color"
env["COLORTERM"] = "truecolor"
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

no_fork = "--no-fork" in flags

if no_fork:
    # Stay in foreground - caller is responsible for backgrounding (e.g. Rust .spawn())
    signal.signal(signal.SIGHUP, signal.SIG_IGN)
    devnull = os.open(os.devnull, os.O_RDWR)
    os.dup2(devnull, 0)
    os.dup2(devnull, 1)
    os.dup2(devnull, 2)
    os.close(devnull)
    proc.wait()
    os.close(master)
else:
    # Double-fork to become a true daemon reparented to PID 1 (launchd).
    # Single fork isn't enough - Claude Code's sandbox reaps the whole
    # process group. Double fork + setsid escapes it.
    pid = os.fork()
    if pid > 0:
        os.close(master)
        sys.exit(0)

    # First child: become session leader, detach from parent's process group
    os.setsid()

    # Second fork: the grandchild is the actual daemon.
    # It can't be a session leader, so it can never acquire a controlling terminal.
    pid2 = os.fork()
    if pid2 > 0:
        os._exit(0)

    # Grandchild: true daemon, reparented to launchd
    signal.signal(signal.SIGHUP, signal.SIG_IGN)

    devnull = os.open(os.devnull, os.O_RDWR)
    os.dup2(devnull, 0)
    os.dup2(devnull, 1)
    os.dup2(devnull, 2)
    os.close(devnull)

    proc.wait()
    os.close(master)

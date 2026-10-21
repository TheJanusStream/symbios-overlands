"""Shared plumbing for the agent's helper scripts (see README.md here).

The agent binary and the render tool are found from this file's place in the
repo (docs/agent/tools/ -> the repo root), built with `--profile test-release`.
Set AGENT_ACCOUNT to the agent's handle when more than one session is saved:
it is added to every command, at its end - any place would do, as
`--account` goes anywhere on the line (#1473).
"""
import json
import os
import subprocess

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
# AGENT_BIN and AGENT_RENDER point the tools at copies of the agent binary and the render tool:
# a sub-agent rebuilding either (with mutant guards, or half-way through a link) must not change
# the binary a session is using - the daemon a session started runs its own copy anyway.
AGENT = os.environ.get("AGENT_BIN") or os.path.join(REPO, "target", "test-release", "agent")
RENDER = os.environ.get("AGENT_RENDER") or os.path.join(REPO, "target", "test-release", "render")
ENV = dict(os.environ, BEVY_ASSET_ROOT=REPO)


def account_args():
    account = os.environ.get("AGENT_ACCOUNT")
    return ["--account", account] if account else []


def agent(*args):
    """Run one agent command and return its JSON answer (never raises on a refusal)."""
    out = subprocess.run([AGENT, *args, *account_args()], capture_output=True, text=True, env=ENV)
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return {"ok": False, "error": f"no JSON from agent: {out.stdout[:400]} {out.stderr[:400]}"}


def result(answer, what):
    """The `result` of a wrapped answer, or exit with its error."""
    if not answer.get("ok"):
        raise SystemExit(f"{what} failed: {answer.get('error')}")
    return answer["result"]


# The render tool's flags for a world's hour and weather, which views.py and compare.py pass
# on to every render they make: a turning day or a weather trial judged offline (#1730).
RENDER_FLAGS = ("--hour", "--weather")


def split_render_flags(argv):
    """`argv` without its `--hour H` and `--weather KIND:I` (anywhere on the line, as `--flag
    value` or `--flag=value`), and those flags as the render tool takes them: always
    `--flag=value`, so a negative hour reaches it as a value and not as a flag of its own. A
    flag with no value, an empty one or another flag where its value should be is refused here,
    where the mistake is plain, rather than as a view that did not render."""
    rest, flags = [], []
    i = 0
    while i < len(argv):
        arg = argv[i]
        name = arg.split("=", 1)[0]
        if name not in RENDER_FLAGS:
            rest.append(arg)
            i += 1
            continue
        if "=" in arg:
            value = arg.split("=", 1)[1]
            i += 1
        elif i + 1 < len(argv):
            value = argv[i + 1]
            i += 2
        else:
            raise SystemExit(f"{name} needs a value")
        if not value or value.startswith("--"):
            raise SystemExit(f"{name} needs a value, not {value!r}")
        flags.append(f"{name}={value}")
    return rest, flags


def render(*args, timeout=180):
    """Run the render tool; returns (stdout, stderr)."""
    out = subprocess.run(["timeout", str(timeout), RENDER, *args], capture_output=True, text=True, env=ENV)
    return out.stdout, out.stderr

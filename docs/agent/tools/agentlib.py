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


def render(*args, timeout=180):
    """Run the render tool; returns (stdout, stderr)."""
    out = subprocess.run(["timeout", str(timeout), RENDER, *args], capture_output=True, text=True, env=ENV)
    return out.stdout, out.stderr

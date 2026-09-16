#!/usr/bin/env python3
"""MCP conformance oracle: a REAL mcp-SDK client drives `ign mcp serve`.

Test-time-only dependency — the `mcp` SDK is fetched by uv and is NEVER
a Cargo dependency of this repo. Run via the optional gate:

    cargo test -p ignition-cli --test contract_mcp mcp_oracle -- --ignored

or directly (the wiremock-backed profile config is the caller's):

    uv run --with mcp python crates/ignition-cli/tests/mcp_oracle.py -- <path-to-ign>

Requires IGNITION_CLI_CONFIG in the environment (a profile-backed
config — the gate's points at a wiremock serving the status fixtures)
and, if the profile declares `auth.token_env`, that variable. Ambient
IGNITION_* knobs are stripped so a developer shell cannot skew the
run. Exits 0 printing `ORACLE OK` when the full lifecycle — initialize
→ notifications/initialized → tools/list (non-empty) → tools/call with
a text, ok:true envelope — completes.
"""

import asyncio
import json
import os
import sys

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client


def oracle_env() -> dict:
    """A clean env for the spawned server: ambient IGNITION_* stripped,
    the oracle's config/token restored, trace noise ON (the byte-scan's
    worst case must hold for real clients too)."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("IGNITION_")}
    env["IGNITION_CLI_CONFIG"] = os.environ["IGNITION_CLI_CONFIG"]
    env["IGNITION_TOKEN"] = os.environ.get("IGNITION_TOKEN", "mock:name-key")
    env["IGNITION_LOG"] = "trace"
    return env


def server_name(initialize_result) -> str:
    """serverInfo across SDK spellings (server_info alias / camelCase)."""
    info = getattr(initialize_result, "server_info", None) or getattr(
        initialize_result, "serverInfo", None
    )
    return getattr(info, "name", None)


async def main() -> int:
    # The canonical invocation passes `--` before the binary path; uv
    # forwards it verbatim into argv, so skip it when present.
    operands = [a for a in sys.argv[1:] if a != "--"]
    if not operands or not operands[0]:
        print("usage: mcp_oracle.py [--] <path-to-ign>", file=sys.stderr)
        return 2
    ign_path = operands[0]
    if not os.environ.get("IGNITION_CLI_CONFIG"):
        print(
            "IGNITION_CLI_CONFIG must point at the oracle's profile config",
            file=sys.stderr,
        )
        return 2

    params = StdioServerParameters(
        command=ign_path, args=["mcp", "serve"], env=oracle_env()
    )

    async with stdio_client(params) as (read, write):
        async with ClientSession(read, write) as session:
            # 1. initialize — the SDK sends notifications/initialized
            #    itself after the response (lifecycle step 2).
            init = await session.initialize()
            name = server_name(init)
            assert name == "ign", f"serverInfo.name must be ign, got {name!r}"

            # 3. tools/list — the catalog is non-empty and every entry
            #    carries name/description/inputSchema (attribute
            #    spellings vary across SDK versions: snake_case fields
            #    with camelCase aliases).
            listed = await session.list_tools()
            tools = list(listed.tools)
            assert len(tools) > 0, "tools/list returned an empty catalog"
            for tool in tools:
                assert tool.name, "every tool has a name"
                assert tool.description, "every tool has a description"
                schema = getattr(tool, "input_schema", None) or getattr(
                    tool, "inputSchema", None
                )
                assert schema is not None, "every tool has inputSchema"

            # 4. tools/call — a read verb returns text content holding
            #    the frozen ok:true envelope.
            called = await session.call_tool("status", {})
            is_error = getattr(called, "is_error", None)
            if is_error is None:
                is_error = getattr(called, "isError")
            assert not is_error, f"tools/call status errored: {called.content}"
            assert len(called.content) >= 1, "tools/call returned no content"
            first = called.content[0]
            text = getattr(first, "text", None)
            assert text, f"the first content block is text: {first}"
            envelope = json.loads(text)
            assert envelope.get("ok") is True, f"envelope not ok:true: {envelope}"

    print("ORACLE OK")
    return 0


if __name__ == "__main__":
    sys.exit(asyncio.run(main()))

from __future__ import annotations

import argparse
import inspect
import json
from typing import Any, Dict

from .. import CFTestClient  # package-level client
from . import multi_system, single_system  # ensure scenarios are imported


def _discover_scenarios():
    out: Dict[str, Any] = {}
    for mod in (single_system, multi_system):
        for name, obj in vars(mod).items():
            if name.startswith("scenario_") and callable(obj):
                out[name.removeprefix("scenario_")] = obj
    return out


def _coerce_arg(val: str):
    # minimal string→type coercion
    low = val.lower()
    if low in {"true", "false"}:
        return low == "true"
    for caster in (int, float):
        try:
            return caster(val)
        except Exception:
            pass
    return val


def _filter_kwargs(fn, ns):
    sig = inspect.signature(fn)
    allowed = {k for k in sig.parameters if k != "client"}
    return {k: v for k, v in ns.items() if k in allowed and v is not None}


def scenarios_main(argv=None):
    scenarios = _discover_scenarios()

    parser = argparse.ArgumentParser(prog="cf-scenarios", add_help=True)
    parser.add_argument(
        "-s",
        "--scenario",
        choices=sorted(scenarios.keys()),
        required=True,
        help="scenario to run (without 'scenario_' prefix)",
    )
    # common optional knobs used by many scenarios; ignored if not in fn signature
    parser.add_argument("--hostname")
    parser.add_argument("--num-systems", type=int, dest="num_systems")
    parser.add_argument("--num-overdue", type=int, dest="num_overdue")
    parser.add_argument("--overdue-minutes", type=int, dest="overdue_minutes")
    parser.add_argument("--ok-heartbeat-minutes", type=int, dest="ok_heartbeat_minutes")
    parser.add_argument("--flake-name", dest="flake_name")
    parser.add_argument("--repo-url", dest="repo_url")
    parser.add_argument("--agent-version", dest="agent_version")
    parser.add_argument("--days", type=int)
    parser.add_argument(
        "--heartbeat-interval-minutes", type=int, dest="heartbeat_interval_minutes"
    )
    parser.add_argument("--heartbeat-hours", type=int, dest="heartbeat_hours")
    parser.add_argument(
        "--stagger-window-minutes", type=int, dest="stagger_window_minutes"
    )
    parser.add_argument("--base-hostname", dest="base_hostname")

    # generic passthrough: --param key=value (repeatable)
    parser.add_argument(
        "--param",
        action="append",
        default=[],
        help="extra key=value args to pass to the scenario function",
    )

    args = parser.parse_args(argv)
    fn = scenarios[args.scenario]

    # build kwargs
    kwargs = vars(args).copy()
    params: Dict[str, Any] = {}
    for kv in args.param:
        if "=" not in kv:
            parser.error(f"--param must be key=value, got: {kv}")
        k, v = kv.split("=", 1)
        params[k.replace("-", "_")] = _coerce_arg(v)
    kwargs.update(params)

    # only send kwargs the function accepts
    call_kwargs = _filter_kwargs(fn, kwargs)

    client = CFTestClient()
    result = fn(client, **call_kwargs)

    # print a compact JSON summary
    print(json.dumps(result, default=str, indent=2))

"""Builds the Wokwi simulation firmware and runs the automated scenarios.

Usage:
    python tests/wokwi/run.py                 # build and run every scenario
    python tests/wokwi/run.py smoke crash     # run only the named scenarios
    python tests/wokwi/run.py --no-build      # reuse the last simulation build

Requires `cargo +esp`, `wokwi-cli` 0.29+ and WOKWI_CLI_TOKEN in the environment.
Serial logs are written to tests/wokwi/out/<scenario>.log.
"""

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCENARIOS_DIR = Path(__file__).resolve().parent
OUT_DIR = SCENARIOS_DIR / "out"

# Intervalos cortos para que cada escena dure segundos en vez de minutos.
SIM_ENV = {
    "TELEMETRY_INTERVAL_SECS": "2",
    "SENSOR_INTERVAL_SECS": "1",
    "OFFLINE_CUTOFF_SECS": "6",
    "LIVENESS_TIMEOUT_SECS": "10",
    "WATCHDOG_TIMEOUT_SECS": "10",
    "HEAP_LOG_SECS": "5",
}

# Escena -> (tiempo límite simulado en ms, texto que hace fallar la escena).
SCENARIOS = {
    "smoke": (40_000, "SMARTPOT-FATAL"),
    "actuators": (60_000, "SMARTPOT-FATAL"),
    "crash": (40_000, None),
    "hang-core1": (60_000, None),
    "hang-core0": (60_000, None),
}


def build(extra_env):
    env = {**os.environ, **SIM_ENV, **extra_env}
    command = ["cargo", "+esp", "build", "--features", "sim"]
    print("[BUILD]", " ".join(command))
    subprocess.run(command, cwd=ROOT, env=env, check=True)


def run_scenario(name):
    timeout_ms, fail_text = SCENARIOS[name]
    OUT_DIR.mkdir(exist_ok=True)
    log_file = OUT_DIR / f"{name}.log"
    command = [
        shutil.which("wokwi-cli") or "wokwi-cli",
        str(ROOT),
        "--scenario",
        str(SCENARIOS_DIR / f"{name}.yaml"),
        "--timeout",
        str(timeout_ms),
        "--serial-log-file",
        str(log_file),
    ]
    if fail_text:
        command += ["--fail-text", fail_text]
    print(f"[RUN] {name} (timeout {timeout_ms} ms)")
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, errors="replace")
    passed = result.returncode == 0
    print(f"[{'PASS' if passed else 'FAIL'}] {name} (exit {result.returncode}, log {log_file})")
    if not passed:
        print("\n".join(result.stdout.splitlines()[-15:]))
        print(result.stderr[-2000:])
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scenarios", nargs="*", choices=[*SCENARIOS, []], help="scenarios to run (default: all)")
    parser.add_argument("--no-build", action="store_true", help="reuse the last simulation build")
    args = parser.parse_args()

    if not os.environ.get("WOKWI_CLI_TOKEN"):
        sys.exit("WOKWI_CLI_TOKEN is not set: create one at https://wokwi.com/dashboard/ci")

    if not args.no_build:
        build({})

    selected = args.scenarios or list(SCENARIOS)
    failures = [name for name in selected if not run_scenario(name)]
    print(f"\n{len(selected) - len(failures)}/{len(selected)} scenarios passed")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

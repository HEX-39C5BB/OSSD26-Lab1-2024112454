"""A/B smoke test for the read-only NC/Shape notification agent.

Runs the existing simple piston fixture once without the notification agent
and once with it, then compares final observable state and validates the
enter/exit, depth/parent and loss-count invariants of the notification trace.
"""
import argparse
import json
import tempfile
import time
from pathlib import Path

from run_dedicated_bud013 import (
    AWAY,
    Bot,
    NOTIFICATION_JAR,
    PISTON,
    PISTON_HEAD,
    PISTON_LOAD,
    PISTON_POWER,
    REFERENCE,
    ROOT,
    SPAWN,
    Server,
    block_state,
    build_notification_agent,
)


def read_agent(server):
    path = server.agent_log
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def wait_agent(server, predicate, timeout=10):
    deadline = time.time() + timeout
    last = []
    while time.time() < deadline:
        last = read_agent(server)
        if last and predicate(last[-1]):
            return last[-1]
        time.sleep(0.02)
    raise RuntimeError(f"agent state barrier timed out; last={last[-1] if last else None}")


def chunk_state(row):
    chunks = row.get("chunks", [])
    return chunks[0] if chunks else {}


def piston_event(row):
    return any(entry.get("pos") == list(PISTON) for entry in row.get("block_events", []))


def run_once(prism_root, java, timeout, notify):
    extra = []
    notify_path = None
    if notify:
        jar = build_notification_agent(java)
        directory = Path(
            tempfile.mkdtemp(prefix="notification-smoke.", dir=ROOT / "target")
        )
        notify_path = directory / "notify.jsonl"
        extra.append(f"{jar}={notify_path}")
    server = Server(prism_root, java, timeout, extra_agents=extra)
    bot = None
    try:
        bot = Bot(server.port)
        server.wait_for(f"{REFERENCE} joined the game", 30)
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        server.command(
            f"setblock {PISTON[0]} {PISTON[1]} {PISTON[2]} "
            "piston[facing=east]"
        )
        server.command(
            f"setblock {PISTON_LOAD[0]} {PISTON_LOAD[1]} {PISTON_LOAD[2]} "
            "smooth_stone"
        )
        wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is True,
            10,
        )
        server.command_now(
            f"setblock {PISTON_POWER[0]} {PISTON_POWER[1]} {PISTON_POWER[2]} "
            "redstone_block"
        )
        server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
        away = wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is False
            and piston_event(row),
            10,
        )
        retained = piston_event(away)
        server.command_now(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        returned = wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is True
            and not piston_event(row),
            10,
        )
        cleared = not piston_event(returned)
        deadline = time.time() + 5
        extended = False
        while time.time() < deadline:
            extended = (
                block_state(server, PISTON, "minecraft:piston[extended=true]")
                is True
            )
            if extended:
                break
            time.sleep(0.05)
        head = block_state(
            server, PISTON_HEAD, "minecraft:piston_head[facing=east]"
        )
        load = block_state(server, PISTON_LOAD, "minecraft:smooth_stone")
        return {
            "final": (extended, head, load),
            "retained": retained,
            "cleared": cleared,
            "away": chunk_state(away),
            "return": chunk_state(returned),
        }, notify_path
    finally:
        if bot is not None:
            bot.close()
        server.stop()


def validate(path):
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    stack = []
    enters = {}
    exits = set()
    kinds = set()
    max_depth = 0
    max_dropped = 0
    max_gt = 0
    last_seq = 0
    for row in rows:
        if row.get("record") == "dropped":
            max_dropped = max(max_dropped, int(row.get("count", 0)))
            continue
        if row.get("record") != "notify":
            continue
        max_dropped = max(max_dropped, int(row.get("dropped", 0)))
        max_gt = max(max_gt, int(row.get("gt", 0)))
        phase = row.get("phase")
        event_id = int(row["id"])
        if phase == "enter":
            parent = int(row.get("parent", 0))
            expected_parent = stack[-1] if stack else 0
            if row.get("depth") != len(stack) or parent != expected_parent:
                raise SystemExit(
                    f"bad enter depth/parent at id {event_id}: {row}"
                )
            if event_id <= last_seq:
                raise SystemExit(f"non-monotonic sequence at id {event_id}")
            last_seq = event_id
            stack.append(event_id)
            enters[event_id] = row
            kinds.add(row["kind"])
            max_depth = max(max_depth, int(row["depth"]))
        elif phase == "exit":
            if not stack or stack[-1] != event_id:
                raise SystemExit(f"unbalanced exit at id {event_id}")
            stack.pop()
            exits.add(event_id)
            if row.get("depth") != len(stack):
                raise SystemExit(f"bad exit depth at id {event_id}: {row}")
            expected_parent = stack[-1] if stack else 0
            if int(row.get("parent", 0)) != expected_parent:
                raise SystemExit(f"bad exit parent at id {event_id}: {row}")
        else:
            raise SystemExit(f"unknown notification phase: {row}")
    if stack:
        raise SystemExit(f"unclosed notification frames: {stack}")
    if set(enters) != exits:
        raise SystemExit("notification enter/exit ids do not pair")
    if max_dropped != 0:
        raise SystemExit(f"notification buffer dropped {max_dropped} events")
    for required in ("neighbor_shape_changed", "neighbor_changed", "update_shape"):
        if required not in kinds:
            raise SystemExit(f"missing {required} in {sorted(kinds)}")
    matched = False
    for event in enters.values():
        if event.get("kind") != "neighbor_changed":
            continue
        args = event.get("args", [])
        if len(args) >= 4 and args[1] == list(PISTON) and args[3] == list(PISTON_POWER):
            matched = True
            break
    if not matched:
        raise SystemExit("no neighbor_changed event from the power block to the piston")
    if max_depth == 0:
        raise SystemExit("notification trace has no nested calls")
    if max_gt == 0:
        raise SystemExit("notification trace has no game time")
    print(
        "PASS notification smoke:"
        f" events={len(enters)} kinds={sorted(kinds)}"
        f" max_depth={max_depth} gt={max_gt} dropped=0"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--prism-root",
        type=Path,
        default=Path.home() / ".local/share/PrismLauncher",
    )
    parser.add_argument("--java", default="/opt/openjdk-bin-17/bin/java")
    parser.add_argument("--timeout", type=int, default=60)
    args = parser.parse_args()
    root = args.prism_root.expanduser()
    baseline, _ = run_once(root, args.java, args.timeout, False)
    notified, trace = run_once(root, args.java, args.timeout, True)
    behavior = ("final", "retained", "cleared")
    if any(baseline[key] != notified[key] for key in behavior):
        raise SystemExit(f"A/B observable mismatch: {baseline} != {notified}")
    print(
        "PASS A/B behavior:"
        f" final={notified['final']} retained={notified['retained']}"
        f" cleared={notified['cleared']}"
    )
    print(
        "CHUNK away="
        f"{notified['away']} return={notified['return']}"
    )
    validate(trace)


if __name__ == "__main__":
    main()

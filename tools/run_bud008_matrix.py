"""Run the BUD-008/009 dual-source NC/Shape matrix on a DedicatedServer.

Each scenario gets a fresh server and a datapack function. The function runs
the source setblocks and the away teleport in one tick, so the source order is
explicit and the Block Event is retained while the target chunk is unloaded.
"""
import argparse
import json
import re
import tempfile
import time
from pathlib import Path

from run_dedicated_bud013 import (
    AWAY,
    Bot,
    NOTIFICATION_JAR,
    PISTON,
    REFERENCE,
    ROOT,
    SPAWN,
    Server,
    block_state,
    build_notification_agent,
)

FIXTURE = "BUD008-MATRIX"
PISTON_POS = (258, 110, 12)
HEAD_POS = (259, 110, 12)
QC_POS = (258, 111, 11)
SOURCE_A = (258, 109, 12)
SOURCE_B = (258, 111, 12)


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
    raise RuntimeError(
        f"agent barrier timed out; last={last[-1] if last else None}"
    )


def chunk_state(row):
    chunks = row.get("chunks", [])
    return chunks[0] if chunks else {}


def piston_events(row):
    return [
        entry
        for entry in row.get("block_events", [])
        if entry.get("pos") == list(PISTON_POS)
    ]


def write_pack(server):
    world = server.directory / "world"
    pack = world / "datapacks" / "bud008"
    functions = pack / "data" / "bud008" / "functions"
    functions.mkdir(parents=True, exist_ok=True)
    (pack / "pack.mcmeta").write_text(
        json.dumps(
            {
                "pack": {
                    "pack_format": 15,
                    "description": "RustCraft BUD-008/009 matrix",
                }
            }
        ),
        encoding="utf-8",
    )

    def command(position):
        return f"setblock {position[0]} {position[1]} {position[2]} glass"

    def function(name, positions):
        body = [command(position) for position in positions]
        (functions / f"{name}.mcfunction").write_text(
            "\n".join(body) + "\n", encoding="utf-8"
        )

    function("a", [SOURCE_A])
    function("b", [SOURCE_B])
    for name, source in (("a-repeat", SOURCE_A), ("b-repeat", SOURCE_B)):
        body = [
            command(source),
            f"setblock {source[0]} {source[1]} {source[2]} air",
            command(source),
        ]
        (functions / f"{name}.mcfunction").write_text(
            "\n".join(body) + "\n", encoding="utf-8"
        )
    function("ab", [SOURCE_A, SOURCE_B])
    function("ba", [SOURCE_B, SOURCE_A])
    function("control", [(270, 110, 2), (270, 110, 8)])


def run_scenario(root, java, timeout, name):
    jar = build_notification_agent(java)
    directory = Path(
        tempfile.mkdtemp(prefix=f"bud008-{name}.", dir=ROOT / "target")
    )
    notify_path = directory / "notify.jsonl"
    server = Server(
        root,
        java,
        timeout,
        extra_agents=[f"{jar}={notify_path}"],
    )
    bot = None
    try:
        bot = Bot(server.port)
        server.wait_for(f"{REFERENCE} joined the game", 30)
        write_pack(server)
        server.command("reload", settle=0.4)
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        server.command(
            f"setblock {PISTON_POS[0]} {PISTON_POS[1]} {PISTON_POS[2]} "
            "piston[facing=east]"
        )
        server.command(
            f"setblock {QC_POS[0]} {QC_POS[1]} {QC_POS[2]} redstone_block"
        )
        wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is True,
            10,
        )
        time.sleep(0.4)
        ready = block_state(
            server, PISTON_POS, "minecraft:piston[extended=true]"
        )
        if ready is True:
            raise RuntimeError("piston extended during setup")
        before = len(notify_path.read_text().splitlines()) if notify_path.exists() else 0
        server.command_now(f"function bud008:{name}")
        server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
        away = wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is False
            and piston_events(row),
            10,
        )
        event_count = len(piston_events(away))
        server.command_now(
            f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}"
        )
        returned = wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is True
            and not piston_events(row),
            10,
        )
        deadline = time.time() + 5
        extended = False
        while time.time() < deadline:
            extended = (
                block_state(
                    server, PISTON_POS, "minecraft:piston[extended=true]"
                )
                is True
            )
            if extended:
                break
            time.sleep(0.05)
        head = block_state(
            server, HEAD_POS, "minecraft:piston_head[facing=east]"
        )
        rows = [
            json.loads(line)
            for line in notify_path.read_text().splitlines()
            if line.strip()
        ][before:]
        return {
            "scenario": name,
            "notify": str(notify_path),
            "trace_start": before,
            "away": chunk_state(away),
            "return": chunk_state(returned),
            "event_count": event_count,
            "extended": extended,
            "head": head,
            "target_events": target_events(rows),
        }
    finally:
        if bot is not None:
            bot.close()
        server.stop()


def normalize(value):
    if isinstance(value, list):
        return value
    if isinstance(value, str):
        match = re.search(r"x=(-?\d+), y=(-?\d+), z=(-?\d+)", value)
        if match:
            return [int(part) for part in match.groups()]
    return value


def parsed_events(rows):
    selected = []
    for row in rows:
        if row.get("record") != "notify" or row.get("phase") != "enter":
            continue
        kind = row.get("kind")
        args = row.get("args", [])
        target = source = direction = None
        if kind == "neighbor_changed" and len(args) >= 4:
            target, source = normalize(args[1]), normalize(args[3])
        elif kind == "update_shape" and len(args) >= 5:
            target, source, direction = normalize(args[3]), normalize(args[4]), args[0]
        elif kind == "neighbor_shape_changed" and len(args) >= 4:
            target, source, direction = normalize(args[2]), normalize(args[3]), args[0]
        elif kind in ("update_neighbors_at", "update_neighbors_except") and len(args) >= 2:
            target = normalize(args[1])
            if kind == "update_neighbors_except" and len(args) >= 4:
                direction = args[3]
        elif kind == "neighbor_changed_dispatch" and len(args) >= 4:
            target, source = normalize(args[1]), normalize(args[3])
        selected.append(
            {
                "gt": row.get("gt"),
                "seq": row.get("seq"),
                "id": row.get("id"),
                "parent": row.get("parent"),
                "depth": row.get("depth"),
                "kind": kind,
                "target": target,
                "source": source,
                "direction": direction,
            }
        )
    return selected


def target_events(rows):
    return [
        event
        for event in parsed_events(rows)
        if event["target"] == list(PISTON_POS)
    ]


def control_summary(rows):
    positions = [[270, 110, 2], [270, 110, 8]]
    events = [
        event
        for event in parsed_events(rows)
        if event["target"] in positions or event["source"] in positions
    ]
    first = {}
    for event in events:
        for position in positions:
            if event["target"] == position or event["source"] == position:
                first.setdefault(json.dumps(position), event)
    gts = {
        json.dumps(position): first.get(json.dumps(position), {}).get("gt")
        for position in positions
    }
    return {
        "gts": gts,
        "same_gt": len(set(gts.values())) == 1 and None not in gts.values(),
        "events": events,
    }


def run_control(root, java, timeout):
    jar = build_notification_agent(java)
    directory = Path(tempfile.mkdtemp(prefix="bud008-control.", dir=ROOT / "target"))
    notify_path = directory / "notify.jsonl"
    server = Server(
        root,
        java,
        timeout,
        extra_agents=[f"{jar}={notify_path}"],
    )
    bot = None
    try:
        bot = Bot(server.port)
        server.wait_for(f"{REFERENCE} joined the game", 30)
        write_pack(server)
        server.command("reload", settle=0.4)
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        wait_agent(
            server,
            lambda row: chunk_state(row).get("resident") is True,
            10,
        )
        before = len(notify_path.read_text().splitlines()) if notify_path.exists() else 0
        server.command_now("function bud008:control")
        time.sleep(0.5)
        rows = [
            json.loads(line)
            for line in notify_path.read_text().splitlines()
            if line.strip()
        ][before:]
        summary = control_summary(rows)
        summary["notify"] = str(notify_path)
        return summary
    finally:
        if bot is not None:
            bot.close()
        server.stop()


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
    control = run_control(root, args.java, args.timeout)
    print(json.dumps({"control": control}, ensure_ascii=False), flush=True)
    results = []
    for name in ("a", "b", "ab", "ba", "a-repeat", "b-repeat"):
        result = run_scenario(root, args.java, args.timeout, name)
        results.append(result)
        print(json.dumps(result, ensure_ascii=False), flush=True)
    for result in results:
        if result["extended"] is not True or result["head"] is not True:
            raise SystemExit(f"{result['scenario']} did not extend/head")
        if result["event_count"] < 1:
            raise SystemExit(f"{result['scenario']} queued no piston event")
        if not result["target_events"]:
            raise SystemExit(f"{result['scenario']} observed no target notification")
    controls = {
        "a": [list(SOURCE_A)],
        "b": [list(SOURCE_B)],
    }
    for name, sources in controls.items():
        result = next(result for result in results if result["scenario"] == name)
        observed = {json.dumps(event["source"]) for event in result["target_events"]}
        if json.dumps(sources[0]) not in observed:
            raise SystemExit(f"{name} control missing source notification")
    ab = next(result for result in results if result["scenario"] == "ab")
    ba = next(result for result in results if result["scenario"] == "ba")
    for result in (ab, ba):
        sources = {json.dumps(event["source"]) for event in result["target_events"]}
        for source in (SOURCE_A, SOURCE_B):
            if json.dumps(list(source)) not in sources:
                raise SystemExit(
                    f"{result['scenario']} missing source {source} notification"
                )
        source_events = [
            event
            for event in result["target_events"]
            if event["source"] in (list(SOURCE_A), list(SOURCE_B))
        ]
        gts = {event["gt"] for event in source_events}
        if len(gts) != 1:
            raise SystemExit(
                f"{result['scenario']} source notifications did not share one GT: {gts}"
            )
        first = next(
            event
            for event in result["target_events"]
            if event["source"] in (list(SOURCE_A), list(SOURCE_B))
        )
        expected = (
            list(SOURCE_A)
            if result["scenario"] in ("a", "ab", "a-repeat")
            else list(SOURCE_B)
        )
        if first["source"] != expected:
            raise SystemExit(
                f"{result['scenario']} first source was {first['source']}, expected {expected}"
            )
    for result in (r for r in results if r["scenario"].endswith("-repeat")):
        source = list(SOURCE_A if result["scenario"].startswith("a-") else SOURCE_B)
        events = [event for event in result["target_events"] if event["source"] == source]
        if len(events) < 3:
            raise SystemExit(
                f"{result['scenario']} expected repeated source notifications: {events}"
            )
        if len({event["gt"] for event in events[:3]}) != 1:
            raise SystemExit(
                f"{result['scenario']} repeats crossed GT boundaries: {events[:3]}"
            )
        result["repeated_source_count"] = len(events)
    baseline = []
    for result in results:
        source_events = [
            event
            for event in result["target_events"]
            if event["source"] in (list(SOURCE_A), list(SOURCE_B))
        ]
        baseline.append(
            {
                "scenario": result["scenario"],
                "source_events": source_events,
                "event_count": result["event_count"],
                "extended": result["extended"],
                "head": result["head"],
            }
        )
    baseline_path = ROOT / "target/bud008-java.json"
    baseline_path.write_text(
        json.dumps(baseline, indent=2, ensure_ascii=False), encoding="utf-8"
    )
    print(f"baseline: {baseline_path.relative_to(ROOT)}")
    print("PASS BUD-008/009 matrix: a, b, ab, ba, a-repeat, b-repeat")


if __name__ == "__main__":
    main()

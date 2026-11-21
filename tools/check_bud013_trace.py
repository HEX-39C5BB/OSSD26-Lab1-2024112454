"""Validate DedicatedServer BUD-013 traces and compare the Rust queue trace."""
import json
import sys
from pathlib import Path

CHECKS = (
    ("DEDICATED-BUD013-LAMP", [264, 110, 6], "block_ticks", "lit", False),
    ("DEDICATED-BUD013-PISTON", [264, 110, 10], "block_events", "extended", True),
    ("DEDICATED-BUD013-SECTION", [264, 15, 6], "block_events", "extended", True),
)
CHAIN = "DEDICATED-BUD013-CHAIN"
CHAIN_POSITIONS = [[x, 110, 2] for x in range(264, 269)]


def load(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def one(rows, fixture, phase):
    found = [
        row
        for row in rows
        if row.get("fixture") == fixture and row.get("phase") == phase
    ]
    if len(found) != 1:
        raise SystemExit(f"expected one {fixture} {phase} row, got {len(found)}")
    return found[0]


def queued_between(agent, position, after_gt, before_gt, field):
    for row in agent:
        if not after_gt <= row.get("gt", -1) <= before_gt:
            continue
        for entry in row.get(field, []):
            if entry.get("pos") == position:
                return True
    return False


def cleared_after(agent, position, gt, field):
    for row in agent:
        if row.get("gt", -1) <= gt:
            continue
        if any(entry.get("pos") == position for entry in row.get(field, [])):
            return False
    return True


def chunk_states(agent, after_gt, before_gt):
    states = []
    for row in agent:
        if not after_gt <= row.get("gt", -1) <= before_gt:
            continue
        chunks = row.get("chunks", [])
        if chunks:
            states.append(chunks[0])
    return states


def chain_order(agent, after_gt, before_gt):
    for row in agent:
        if not after_gt <= row.get("gt", -1) <= before_gt:
            continue
        order = []
        for entry in row.get("block_events", []):
            position = entry.get("pos")
            if position in CHAIN_POSITIONS and position not in order:
                order.append(position)
        if len(order) == len(CHAIN_POSITIONS):
            return order
    return None


def java_summary(root):
    trace = [row for row in load(root / "trace.jsonl") if row.get("record") == "sample"]
    agent = load(root / "agent.jsonl")
    result = {}
    for fixture, position, field, final_field, final_value in CHECKS:
        away = one(trace, fixture, "away")
        returned = one(trace, fixture, "after_return")
        if fixture.endswith("LAMP"):
            powered = one(trace, fixture, "powered")
            if powered.get("lit") is not True:
                raise SystemExit("lamp was not lit before unload")
            if returned.get("lit") is not False:
                raise SystemExit("lamp did not settle off after return")
        elif returned.get(final_field) is not final_value:
            raise SystemExit(f"{fixture} did not settle after return")
        if away.get("loaded") is not False:
            raise SystemExit(f"{fixture} chunk was not unloaded")
        retained = queued_between(agent, position, away["gt"], returned["gt"], field)
        cleared = cleared_after(agent, position, returned["gt"], field)
        if not retained or not cleared:
            raise SystemExit(f"{fixture} queue was not retained/cleared")
        states = chunk_states(agent, away["gt"], returned["gt"])
        if states:
            if not any(state.get("resident") is False for state in states):
                raise SystemExit(f"{fixture} agent never observed an unloaded chunk")
            if not any(state.get("resident") is True for state in states):
                raise SystemExit(f"{fixture} agent never observed the chunk resident again")
        result[fixture] = {
            "retained": retained,
            "cleared": cleared,
            "final": returned.get(final_field) is final_value,
        }

    ready = one(trace, CHAIN, "ready")
    away = one(trace, CHAIN, "away")
    returned = one(trace, CHAIN, "after_return")
    settled = one(trace, CHAIN, "after_return_settled")
    if ready.get("extended") != 0:
        raise SystemExit("chain probes triggered before the power source")
    if away.get("loaded") is not False:
        raise SystemExit("chain chunk was not unloaded")
    if returned.get("extended") != len(CHAIN_POSITIONS):
        raise SystemExit("chain probes did not all extend after return")
    if returned.get("heads") != len(CHAIN_POSITIONS):
        raise SystemExit("chain piston heads missing after return")
    if settled.get("extended") != len(CHAIN_POSITIONS):
        raise SystemExit("chain probes did not settle extended")
    retained = all(
        queued_between(agent, position, away["gt"], returned["gt"], "block_events")
        for position in CHAIN_POSITIONS
    )
    cleared = all(
        cleared_after(agent, position, returned["gt"], "block_events")
        for position in CHAIN_POSITIONS
    )
    order = chain_order(agent, away["gt"], returned["gt"])
    if not retained or not cleared or order is None:
        raise SystemExit("chain queue was not retained/cleared/ordered")
    states = chunk_states(agent, away["gt"], returned["gt"])
    if states:
        if not any(state.get("resident") is False for state in states):
            raise SystemExit("chain agent never observed an unloaded chunk")
        if not any(state.get("resident") is True for state in states):
            raise SystemExit("chain agent never observed the chunk resident again")
    result[CHAIN] = {
        "retained": retained,
        "cleared": cleared,
        "final": True,
        "order": order,
    }
    return result


def rust_summary(path):
    rows = load(path)
    result = {}
    for fixture, position, field, final_field, final_value in CHECKS:
        away = one(rows, fixture, "away")
        returned = one(rows, fixture, "after_return")
        final = one(rows, fixture, "final")
        retained = any(entry.get("pos") == position for entry in away.get(field, []))
        cleared = not any(
            entry.get("pos") == position for entry in returned.get(field, [])
        )
        settled = final.get(final_field) is final_value
        if not retained or not cleared or not settled:
            raise SystemExit(f"{fixture} Rust trace failed retained/cleared/final")
        result[fixture] = {
            "retained": retained,
            "cleared": cleared,
            "final": settled,
        }

    away = one(rows, CHAIN, "away")
    returned = one(rows, CHAIN, "after_return")
    final = one(rows, CHAIN, "final")
    retained = all(
        any(entry.get("pos") == position for entry in away.get("block_events", []))
        for position in CHAIN_POSITIONS
    )
    cleared = not any(
        entry.get("pos") in CHAIN_POSITIONS
        for entry in returned.get("block_events", [])
    )
    order = []
    for entry in away.get("block_events", []):
        position = entry.get("pos")
        if position in CHAIN_POSITIONS and position not in order:
            order.append(position)
    settled = final.get("extended") == len(CHAIN_POSITIONS) and final.get(
        "heads"
    ) == len(CHAIN_POSITIONS)
    if not retained or not cleared or len(order) != len(CHAIN_POSITIONS) or not settled:
        raise SystemExit("Rust chain trace failed retained/cleared/order/final")
    result[CHAIN] = {
        "retained": retained,
        "cleared": cleared,
        "final": settled,
        "order": order,
    }
    return result


def main():
    if len(sys.argv) not in (2, 3):
        raise SystemExit(
            "usage: check_bud013_trace.py TARGET_DIR [RUST_TRACE.jsonl]"
        )
    root = Path(sys.argv[1])
    java = java_summary(root)
    for fixture, status in java.items():
        print(f"PASS java {fixture} {status}")
    print(f"PASS {root}")

    if len(sys.argv) == 3:
        rust = rust_summary(Path(sys.argv[2]))
        for fixture, status in rust.items():
            print(f"PASS rust {fixture} {status}")
        for fixture in java:
            if java[fixture] != rust[fixture]:
                raise SystemExit(
                    f"{fixture} Java/Rust mismatch: {java[fixture]} != {rust[fixture]}"
                )
        print("PASS Java/Rust BUD-013 queue comparison")


if __name__ == "__main__":
    main()

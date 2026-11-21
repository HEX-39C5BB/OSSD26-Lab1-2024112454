"""Compare normalized BUD-008/009 Java and Rust semantic traces.

Java input is target/bud008-java.json from tools/run_bud008_matrix.py.
Rust input is JSONL from the bud008_trace example. The comparator reports the
first semantic divergence and never turns UNKNOWN or missing evidence into a
MATCH.
"""
import argparse
import difflib
import json
import sys
from pathlib import Path

SOURCE_A = [258, 109, 12]
SOURCE_B = [258, 111, 12]


def normalize_java(scenario):
    events = scenario.get("source_events")
    if not isinstance(events, list):
        return None
    by_id = {event.get("id"): event for event in events}
    shape_dispatches = set()
    normalized = []
    for event in events:
        kind = event.get("kind")
        source = event.get("source")
        direction = event.get("direction")
        gt = event.get("gt")
        if kind == "neighbor_shape_changed":
            shape_dispatches.add(event.get("id"))
            normalized.append(
                {"kind": "shape", "source": source, "direction": direction, "gt": gt}
            )
        elif kind == "update_shape" and event.get("parent") in shape_dispatches:
            # The nested BlockState.updateShape is the behavior entry of the
            # same semantic shape notification, not a second notification.
            continue
        elif kind in ("neighbor_changed", "neighbor_changed_dispatch"):
            normalized.append(
                {"kind": "nc", "source": source, "direction": direction, "gt": gt}
            )
        elif kind in ("update_neighbors_at", "update_neighbors_except"):
            normalized.append(
                {"kind": "nc", "source": "UNKNOWN", "direction": direction, "gt": gt}
            )
        else:
            normalized.append(
                {"kind": kind, "source": source, "direction": direction, "gt": gt}
            )
    return normalized


def normalize_rust(scenario):
    checks = scenario.get("checks")
    shapes = scenario.get("shapes")
    if not isinstance(checks, list) or not isinstance(shapes, list):
        return None
    events = []
    for event in shapes:
        direction = event.get("direction")
        source = SOURCE_A if direction == "down" else SOURCE_B if direction == "up" else "UNKNOWN"
        events.append(
            {
                "kind": "shape",
                "source": source,
                "direction": direction,
                "gt": event.get("gt"),
                "sequence": event.get("sequence"),
            }
        )
    for event in checks:
        events.append(
            {
                "kind": "nc",
                "source": event.get("source"),
                "direction": None,
                "gt": event.get("gt"),
                "sequence": event.get("sequence"),
            }
        )
    events.sort(key=lambda event: event.get("sequence", 0))
    return events


def relative(events):
    gts = [event["gt"] for event in events if isinstance(event.get("gt"), int)]
    if not gts:
        return None
    origin = min(gts)
    for event in events:
        if isinstance(event.get("gt"), int):
            event["relative_gt"] = event["gt"] - origin
        else:
            event["relative_gt"] = None
    return events


def signature(event):
    return (
        event.get("kind"),
        json.dumps(event.get("source"), ensure_ascii=False),
        event.get("direction"),
        event.get("relative_gt"),
    )


def classify(expected, actual):
    if expected is None:
        return "extra"
    if actual is None:
        return "missing"
    if expected[0] != actual[0]:
        return "type"
    if expected[1] != actual[1]:
        return "source"
    if expected[2] != actual[2]:
        return "direction"
    if expected[3] != actual[3]:
        return "gt"
    return "order"


def compare_scenario(java, rust):
    scenario = java.get("scenario", rust.get("scenario"))
    if scenario is None:
        return {"status": "INCONCLUSIVE", "reason": "missing scenario id"}
    java_events = normalize_java(java)
    rust_events = normalize_rust(rust)
    if java_events is None or rust_events is None:
        return {"status": "INCONCLUSIVE", "reason": "missing events"}
    java_events = relative(java_events)
    rust_events = relative(rust_events)
    if java_events is None or rust_events is None:
        return {"status": "INCONCLUSIVE", "reason": "missing GT"}
    for event in java_events + rust_events:
        if event.get("source") == "UNKNOWN" or event.get("source") is None:
            return {"status": "INCONCLUSIVE", "reason": "UNKNOWN source"}
    expected = [signature(event) for event in java_events]
    actual = [signature(event) for event in rust_events]
    divergence = None
    if expected != actual:
        matcher = difflib.SequenceMatcher(None, expected, actual)
        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag == "equal":
                continue
            e = expected[i1] if i1 < len(expected) else None
            a = actual[j1] if j1 < len(actual) else None
            category = classify(e, a)
            if category == "missing" and i1 < len(expected) and j1 < len(actual):
                category = "order"
            divergence = {
                "category": category,
                "expected": e,
                "actual": a,
                "context_before": expected[max(0, i1 - 2):i1],
                "context_after": expected[i2:i2 + 2],
            }
            break
    queue_ok = java.get("event_count") == rust.get("pending_events")
    final_ok = java.get("extended") == rust.get("final_extended") and java.get(
        "head"
    ) == rust.get("final_head")
    if divergence is not None:
        return {
            "scenario": scenario,
            "status": "MISMATCH",
            "first_divergence": divergence,
            "java_events": java_events,
            "rust_events": rust_events,
        }
    if not queue_ok:
        return {
            "scenario": scenario,
            "status": "MISMATCH",
            "first_divergence": {
                "category": "queue",
                "expected": java.get("event_count"),
                "actual": rust.get("pending_events"),
                "context_before": [],
                "context_after": [],
            },
        }
    if not final_ok:
        return {
            "scenario": scenario,
            "status": "MISMATCH",
            "first_divergence": {
                "category": "final",
                "expected": [java.get("extended"), java.get("head")],
                "actual": [rust.get("final_extended"), rust.get("final_head")],
                "context_before": [],
                "context_after": [],
            },
        }
    return {"scenario": scenario, "status": "MATCH"}


def load_java(path):
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(data, list):
        raise SystemExit("Java baseline must be a JSON list")
    return data


def load_rust(path):
    rows = [
        json.loads(line)
        for line in Path(path).read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    return rows


def _java_sources(events):
    return {
        "scenario": "test",
        "source_events": events,
        "event_count": 1,
        "extended": True,
        "head": True,
    }


def _java_shape(source, direction, gt, event_id, parent=0):
    return {
        "id": event_id,
        "parent": parent,
        "kind": "neighbor_shape_changed",
        "source": source,
        "direction": direction,
        "gt": gt,
    }


def _java_nc(source, gt, event_id):
    return {
        "id": event_id,
        "parent": 0,
        "kind": "neighbor_changed",
        "source": source,
        "direction": None,
        "gt": gt,
    }


def _rust(checks, shapes, pending=1, extended=True, head=True):
    return {
        "scenario": "test",
        "checks": checks,
        "shapes": shapes,
        "pending_events": pending,
        "final_extended": extended,
        "final_head": head,
    }


def _rust_check(source, gt, sequence):
    return {"kind": "nc", "source": source, "gt": gt, "sequence": sequence}


def _rust_shape(direction, gt, sequence):
    return {"kind": "shape", "direction": direction, "gt": gt, "sequence": sequence}


def self_test():
    cases = [
        ("match", _java_sources([_java_shape(SOURCE_A, "down", 10, 1), _java_nc(SOURCE_A, 10, 2)]),
         _rust([_rust_check(SOURCE_A, 0, 2)], [_rust_shape("down", 0, 1)], 1, True, True), "MATCH"),
        ("missing", _java_sources([_java_shape(SOURCE_A, "down", 10, 1), _java_nc(SOURCE_A, 10, 2)]),
         _rust([], [_rust_shape("down", 0, 1)], 1, True, True), "MISMATCH"),
        ("extra", _java_sources([_java_shape(SOURCE_A, "down", 10, 1)]),
         _rust([_rust_check(SOURCE_A, 0, 2)], [_rust_shape("down", 0, 1)], 1, True, True), "MISMATCH"),
        ("order", _java_sources([_java_shape(SOURCE_A, "down", 10, 1), _java_nc(SOURCE_A, 10, 2)]),
         _rust([_rust_check(SOURCE_A, 0, 1)], [_rust_shape("down", 0, 2)], 1, True, True), "MISMATCH"),
        ("duplicate", _java_sources([_java_nc(SOURCE_A, 10, 1), _java_nc(SOURCE_A, 10, 2)]),
         _rust([_rust_check(SOURCE_A, 0, 1)], [], 1, True, True), "MISMATCH"),
        ("repeat-match", _java_sources([
            _java_shape(SOURCE_A, "down", 10, 1),
            _java_nc(SOURCE_A, 10, 2),
            _java_shape(SOURCE_A, "down", 10, 3),
            _java_nc(SOURCE_A, 10, 4),
            _java_shape(SOURCE_A, "down", 10, 5),
            _java_nc(SOURCE_A, 10, 6),
        ]), _rust([
            _rust_check(SOURCE_A, 0, 2),
            _rust_check(SOURCE_A, 0, 4),
            _rust_check(SOURCE_A, 0, 6),
        ], [
            _rust_shape("down", 0, 1),
            _rust_shape("down", 0, 3),
            _rust_shape("down", 0, 5),
        ], 1, True, True), "MATCH"),
        ("direction", _java_sources([_java_shape(SOURCE_A, "down", 10, 1)]),
         _rust([], [_rust_shape("up", 0, 1)], 1, True, True), "MISMATCH"),
        ("gt", _java_sources([_java_shape(SOURCE_A, "down", 10, 1), _java_nc(SOURCE_A, 11, 2)]),
         _rust([_rust_check(SOURCE_A, 0, 2)], [_rust_shape("down", 0, 1)], 1, True, True), "MISMATCH"),
        ("unknown", _java_sources([_java_nc("UNKNOWN", 10, 1)]),
         _rust([], [], 1, True, True), "INCONCLUSIVE"),
        ("queue", _java_sources([_java_shape(SOURCE_A, "down", 10, 1)]),
         _rust([], [_rust_shape("down", 0, 1)], 0, True, True), "MISMATCH"),
        ("final", _java_sources([_java_shape(SOURCE_A, "down", 10, 1)]),
         _rust([], [_rust_shape("down", 0, 1)], 1, False, True), "MISMATCH"),
    ]
    for name, java, rust, expected in cases:
        result = compare_scenario(java, rust)
        if result["status"] != expected:
            raise SystemExit(f"self-test {name}: expected {expected}, got {result}")
        if expected == "MISMATCH" and "first_divergence" not in result:
            raise SystemExit(f"self-test {name}: missing first_divergence")
    print(f"PASS comparator self-test: {len(cases)} cases")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("java", nargs="?")
    parser.add_argument("rust", nargs="?")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if not args.java or not args.rust:
        raise SystemExit("usage: compare_bud008_trace.py JAVA.json RUST.jsonl")
    java = load_java(args.java)
    rust_rows = load_rust(args.rust)
    statuses = []
    for scenario in java:
        name = scenario.get("scenario")
        rust = next((row for row in rust_rows if row.get("scenario") == name), None)
        if rust is None:
            statuses.append({"scenario": name, "status": "INCONCLUSIVE", "reason": "missing Rust scenario"})
            continue
        statuses.append(compare_scenario(scenario, rust))
    for status in statuses:
        print(json.dumps(status, ensure_ascii=False))
    if any(status["status"] != "MATCH" for status in statuses):
        raise SystemExit(1)


if __name__ == "__main__":
    main()

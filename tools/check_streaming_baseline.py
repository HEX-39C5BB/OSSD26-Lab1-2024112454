#!/usr/bin/env python3
"""Check the Gentoo window streaming baseline against TBD-017."""

import argparse
from collections import defaultdict
from pathlib import Path
import shlex
import sys


SETTLE_LIMIT_MS = {2: 135, 4: 235}
RUNS = (1, 2, 3)
EXPANDED_RADII = (1, 4, 8, 12, 16)
GPUS = {
    "intel": {"device_type": "INTEGRATED_GPU", "vendor_id": "0x8086"},
    "nvidia": {"device_type": "DISCRETE_GPU", "vendor_id": "0x10de"},
}
ROUTE = [("outward", z) for z in (1, 2, 3)] + [("return", z) for z in (2, 1, 0)]
NS_PER_MS = 1_000_000


def read_crossings(path, radius, *, gpu):
    metadata = {
        "streaming_benchmark_scene": {
            "scenario": "flat_round_trip_v1",
            "seed": "42",
            "radius": str(radius),
            "debug_assertions": "false",
        },
        "streaming_benchmark_surface": {"width": "1920", "height": "1080"},
        "streaming_benchmark_complete": {"radius": str(radius)},
    }
    if gpu:
        metadata["streaming_benchmark_scene"]["expected_chunks"] = str((2 * radius + 1) ** 2)
        metadata["vulkan_terrain_device"] = GPUS[gpu]
    events = defaultdict(list)
    for line in path.read_text(encoding="utf-8").splitlines():
        tag, _, fields = line.partition(" ")
        prefix, _, event = tag.partition("=")
        if prefix != "event":
            continue
        if event == "client_failed":
            raise ValueError("client reported a failure")
        if event in metadata or event == "streaming_benchmark_crossing":
            events[event].append(dict(field.split("=", 1) for field in shlex.split(fields)))

    for event, expected in metadata.items():
        entries = events[event]
        if len(entries) != 1 or any(entries[0].get(key) != value for key, value in expected.items()):
            raise ValueError(f"missing or unexpected {event}")
    if gpu:
        device = events["vulkan_terrain_device"][0]
        if any(not device.get(key) for key in ("name", "api", "driver", "info")):
            raise ValueError("missing Vulkan device or driver metadata")

    crossings = events["streaming_benchmark_crossing"]
    if [(row["leg"], int(row["chunk_z"])) for row in crossings] != ROUTE:
        raise ValueError("expected three outward and three return crossings in route order")

    row_width = 2 * radius + 1
    for row, (leg, z) in zip(crossings, ROUTE):
        settled = row["settled"]
        elapsed_ns = int(row["elapsed_ns"])
        if row["chunk_x"] != "0" or settled not in ("true", "false") or elapsed_ns < 0:
            raise ValueError(f"invalid crossing fields for {leg} z={z}")
        if settled == "true":
            counts = int(row["generated_chunks"]), int(row["reactivated_chunks"])
            expected = (row_width, 0) if leg == "outward" else (0, row_width)
            if counts != expected:
                raise ValueError(f"unexpected chunk activation counts for {leg} z={z}")
        yield leg, z, settled == "true", elapsed_ns


def check_directory(directory, *, gpu):
    passed = True
    radii = EXPANDED_RADII if gpu else SETTLE_LIMIT_MS
    for radius in radii:
        limit_ms = SETTLE_LIMIT_MS.get(radius) if gpu != "nvidia" else None
        paths = (
            [directory / f"radius-{radius}.log"]
            if gpu
            else [directory / f"radius-{radius}-run-{run}.log" for run in RUNS]
        )
        completed_runs = 0
        settled_times = []
        for path in paths:
            try:
                crossings = list(read_crossings(path, radius, gpu=gpu))
            except (OSError, ValueError, KeyError) as error:
                print(f"{path} | {error}", file=sys.stderr)
                passed = False
                continue

            completed_runs += 1
            for leg, z, settled, elapsed_ns in crossings:
                if not settled:
                    print(f"{path} | {leg} z={z} did not settle", file=sys.stderr)
                    if limit_ms is not None:
                        passed = False
                    continue
                settled_times.append(elapsed_ns)
                if limit_ms is not None and elapsed_ns > limit_ms * NS_PER_MS:
                    print(
                        f"{path} | {leg} z={z} took {elapsed_ns / NS_PER_MS:.6f} ms, "
                        f"limit {limit_ms} ms",
                        file=sys.stderr,
                    )
                    passed = False

        longest = f"{max(settled_times) / NS_PER_MS:.3f}" if settled_times else "unavailable"
        limit = str(limit_ms) if limit_ms is not None else "baseline"
        print(
            f"radius={radius} runs={completed_runs}/{len(paths)} "
            f"settled={len(settled_times)}/{len(paths) * len(ROUTE)} "
            f"max_settle_ms={longest} limit_ms={limit}"
        )
    return passed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="directory containing the streaming logs")
    parser.add_argument("--expanded", action="store_true", help="check logs at radii 1, 4, 8, 12, and 16")
    parser.add_argument("--gpu", choices=GPUS, help="GPU used for expanded logs (default: intel)")
    args = parser.parse_args()
    if args.gpu and not args.expanded:
        parser.error("--gpu requires --expanded")
    gpu = (args.gpu or "intel") if args.expanded else None
    return 0 if check_directory(args.directory, gpu=gpu) else 1


if __name__ == "__main__":
    raise SystemExit(main())

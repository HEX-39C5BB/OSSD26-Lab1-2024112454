"""Run the RustCraft reference fixtures against a locally installed Java 1.20.1."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "tools/java-redstone-reference/Reference.java"
CLIENT_SHA256 = "56b71336d2b4fdffd197f56595b0da93e32a946f78f382a299b8f4b92758bb0f"
TRACE_PREFIX = "RUSTCRAFT_REFERENCE "


def classpath(prism):
    metadata = json.loads((prism / "meta/net.minecraft/1.20.1.json").read_text())
    libraries = prism / "libraries"
    client = libraries / "com/mojang/minecraft/1.20.1/minecraft-1.20.1-client.jar"
    with client.open("rb") as artifact:
        if hashlib.file_digest(artifact, "sha256").hexdigest() != CLIENT_SHA256:
            raise ValueError("expected the unmodified Java Edition 1.20.1 client jar")
    with zipfile.ZipFile(client) as archive:
        version = json.loads(archive.read("version.json"))
    if (version["id"], version["world_version"], version["java_version"]) != ("1.20.1", 3465, 17):
        raise ValueError("unexpected Minecraft build metadata")

    entries = [str(client)]
    for library in metadata["libraries"]:
        artifact = library.get("downloads", {}).get("artifact")
        if not artifact:
            continue
        group, package, version, *classifier = library["name"].split(":")
        name = "-".join([package, version, *classifier]) + ".jar"
        path = libraries / group.replace(".", "/") / package / version / name
        if path.is_file():
            entries.append(str(path))
    return ":".join(entries)


def run(args):
    cp = classpath(args.prism_root.expanduser().resolve())
    jdk = subprocess.run(
        [args.java, "-version"], check=True, capture_output=True, text=True, timeout=30
    )
    java_version = jdk.stderr.strip()
    if not java_version.startswith('openjdk version "17.') and not java_version.startswith(
        'java version "17.'
    ):
        raise ValueError(f"expected Java 17, got {java_version.splitlines()[0]}")
    target = ROOT / "target"
    target.mkdir(exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix="java-redstone-reference.", dir=target))
    print(f"Reference run: {directory.relative_to(ROOT)}", flush=True)
    log = directory / "reference.log"
    command = [args.java, "-Xmx1g"]
    if args.notifications:
        sys.path.insert(0, str(ROOT / "tools"))
        from run_dedicated_bud013 import build_notification_agent

        agent = build_notification_agent(args.java)
        command.extend(
            [
                f"-javaagent:{agent}={directory / 'notifications.jsonl'}|16,0",
                "--add-exports=java.base/jdk.internal.org.objectweb.asm=ALL-UNNAMED",
            ]
        )
    command.extend(["-cp", cp, str(SOURCE)])
    stages = ["fixtures", "qc-save", "qc-load", "chunk-residency"]
    succeeded = True
    with log.open("w") as output:
        for stage in stages:
            process = subprocess.run(
                [*command, stage], cwd=directory, stdout=output,
                stderr=subprocess.STDOUT, timeout=args.timeout,
            )
            if process.returncode:
                succeeded = False
                break

    records = []
    for line in log.read_text().splitlines():
        if TRACE_PREFIX in line:
            records.append(json.loads(line.split(TRACE_PREFIX, 1)[1]))
    succeeded &= [row["stage"] for row in records if row["record"] == "stage_complete"] == stages
    trace = directory / "reference.jsonl"
    with trace.open("w") as output:
        header = {
            "schema": 1,
            "record": "reference_run",
            "edition": "java",
            "version": "1.20.1",
            "ruleset": "java/1.20.1",
            "client_sha256": CLIENT_SHA256,
            "java": java_version.splitlines()[0],
            "packs": ["vanilla"],
        }
        output.write(json.dumps(header) + "\n")
        for record in records:
            output.write(json.dumps(record) + "\n")
        if succeeded:
            output.write(json.dumps({"record": "complete"}) + "\n")
    if not succeeded:
        raise ValueError(f"reference fixtures failed, see {log.relative_to(ROOT)}")
    print(f"{len(records)} records: {trace.relative_to(ROOT)}")
    if args.notifications:
        notifications = directory / "notifications.jsonl"
        print(f"notification trace: {notifications.relative_to(ROOT)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prism-root", type=Path, required=True)
    parser.add_argument("--java", default="java", help="Path to the Java 17 executable")
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument(
        "--notifications",
        action="store_true",
        help="attach the read-only Java notification agent to the GameTest reference run",
    )
    args = parser.parse_args()
    try:
        run(args)
    except (OSError, ValueError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()

"""Run the BUD-013 online chunk probe against a local Java 1.20.1 server.

The probe starts an offline-mode DedicatedServer from the installed client jar,
connects a minimal protocol bot, and moves that player out of and back into the
target chunk. It emits JSONL samples for the resident block-tick scenario.
"""
import argparse
import json
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from run_java_redstone_reference import classpath

ROOT = Path(__file__).resolve().parent.parent
AGENT_SOURCE = ROOT / "tools/java-redstone-reference/BudAgent.java"
AGENT_CLASSES = ROOT / "target/bud-agent-classes"
AGENT_JAR = ROOT / "target/bud-agent.jar"
NOTIFICATION_SOURCE = ROOT / "tools/java-redstone-reference/NotificationAgent.java"
NOTIFICATION_CLASSES = ROOT / "target/notification-agent-classes"
NOTIFICATION_JAR = ROOT / "target/notification-agent.jar"
REFERENCE = "Reference"
LAMP = (264, 110, 6)
POWER = (263, 110, 6)
PISTON = (264, 110, 10)
PISTON_HEAD = (265, 110, 10)
PISTON_LOAD = (266, 110, 10)
PISTON_POWER = (263, 110, 10)
SECTION_PISTON = (264, 15, 6)
SECTION_HEAD = (264, 16, 6)
SECTION_LOAD = (264, 17, 6)
SECTION_POWER = (263, 15, 6)
CHAIN_SUPPORTS = [(x, 109, 1) for x in range(264, 269)]
CHAIN_DUST = [(x, 110, 1) for x in range(264, 269)]
CHAIN_PROBES = [(x, 110, 2) for x in range(264, 269)]
CHAIN_HEADS = [(x, 110, 3) for x in range(264, 269)]
CHAIN_QC = [(x, 111, 3) for x in range(264, 269)]
CHAIN_SOURCE = (263, 110, 1)
SPAWN = (260, 110, 10)
AWAY = (0, 100, 0)


def varint(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def string(value):
    encoded = value.encode()
    return varint(len(encoded)) + encoded


def packet(payload):
    return varint(len(payload)) + payload


class Bot:
    def __init__(self, port, name=REFERENCE):
        self.socket = socket.create_connection(("127.0.0.1", port), timeout=10)
        handshake = (
            varint(0)
            + varint(763)
            + string("127.0.0.1")
            + struct.pack(">H", port)
            + varint(2)
        )
        self.socket.sendall(packet(handshake))
        # 1.20.1 Login Start carries the name followed by an optional UUID flag.
        self.socket.sendall(packet(varint(0) + string(name) + b"\x00"))
        self.reader = threading.Thread(target=self._discard, daemon=True)
        self.reader.start()

    def _discard(self):
        try:
            while self.socket.recv(4096):
                pass
        except OSError:
            pass

    def close(self):
        self.socket.close()


def build_agent(java):
    """Compile the reflection-only agent once per checkout."""
    toolchain = Path(java).resolve().parent
    javac = toolchain / "javac"
    jar = toolchain / "jar"
    if not javac.is_file() or not jar.is_file():
        raise RuntimeError("JDK javac/jar are required for the BUD agent")
    if (
        AGENT_JAR.is_file()
        and AGENT_JAR.stat().st_mtime >= AGENT_SOURCE.stat().st_mtime
    ):
        return AGENT_JAR
    shutil.rmtree(AGENT_CLASSES, ignore_errors=True)
    AGENT_CLASSES.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [str(javac), "-d", str(AGENT_CLASSES), str(AGENT_SOURCE)],
        check=True,
        capture_output=True,
        text=True,
    )
    manifest = ROOT / "target/bud-agent-manifest.mf"
    manifest.write_text(
        "Premain-Class: rustcraft.budagent.BudAgent\n"
        "Can-Redefine-Classes: false\n"
        "Can-Retransform-Classes: false\n",
        encoding="utf-8",
    )
    subprocess.run(
        [
            str(jar),
            "--create",
            "--file",
            str(AGENT_JAR),
            "--manifest",
            str(manifest),
            "-C",
            str(AGENT_CLASSES),
            ".",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return AGENT_JAR


def build_notification_agent(java):
    """Compile the read-only NC/Shape notification tracer."""
    toolchain = Path(java).resolve().parent
    javac = toolchain / "javac"
    jar = toolchain / "jar"
    if not javac.is_file() or not jar.is_file():
        raise RuntimeError("JDK javac/jar are required for the notification agent")
    if (
        NOTIFICATION_JAR.is_file()
        and NOTIFICATION_JAR.stat().st_mtime >= NOTIFICATION_SOURCE.stat().st_mtime
    ):
        return NOTIFICATION_JAR
    shutil.rmtree(NOTIFICATION_CLASSES, ignore_errors=True)
    NOTIFICATION_CLASSES.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            str(javac),
            "--add-exports",
            "java.base/jdk.internal.org.objectweb.asm=ALL-UNNAMED",
            "-d",
            str(NOTIFICATION_CLASSES),
            str(NOTIFICATION_SOURCE),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    manifest = ROOT / "target/notification-agent-manifest.mf"
    manifest.write_text(
        "Premain-Class: rustcraft.notifyagent.NotificationAgent\n"
        "Can-Redefine-Classes: false\n"
        "Can-Retransform-Classes: false\n",
        encoding="utf-8",
    )
    subprocess.run(
        [
            str(jar),
            "--create",
            "--file",
            str(NOTIFICATION_JAR),
            "--manifest",
            str(manifest),
            "-C",
            str(NOTIFICATION_CLASSES),
            ".",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return NOTIFICATION_JAR


class Server:
    def __init__(self, prism_root, java, timeout, extra_agents=()):
        self.timeout = timeout
        self.directory = Path(
            tempfile.mkdtemp(prefix="dedicated-bud013.", dir=ROOT / "target")
        )
        self.agent_path = build_agent(java)
        self.agent_log = self.directory / "agent.jsonl"
        self.log_path = self.directory / "server.log"
        (self.directory / "eula.txt").write_text("eula=true\n", encoding="utf-8")
        probe = socket.socket()
        probe.bind(("127.0.0.1", 0))
        self.port = probe.getsockname()[1]
        probe.close()
        (self.directory / "server.properties").write_text(
            "online-mode=false\n"
            "server-ip=127.0.0.1\n"
            f"server-port={self.port}\n"
            "level-type=flat\n"
            "level-seed=42\n"
            "spawn-protection=0\n"
            "view-distance=1\n"
            "simulation-distance=1\n"
            "max-players=2\n"
            "enable-command-block=true\n"
            "enforce-secure-profile=false\n"
            "player-idle-timeout=0\n"
            "generate-structures=false\n",
            encoding="utf-8",
        )
        cp = classpath(prism_root)
        self.log = self.log_path.open("w", encoding="utf-8")
        agent_args = [f"-javaagent:{self.agent_path}={self.agent_log}|16,0"]
        agent_args.extend(f"-javaagent:{agent}" for agent in extra_agents)
        self.process = subprocess.Popen(
            [
                java,
                "-Xmx1g",
                *agent_args,
                "--add-opens=java.base/java.lang=ALL-UNNAMED",
                "--add-exports=java.base/jdk.internal.org.objectweb.asm=ALL-UNNAMED",
                "-cp",
                cp,
                "net.minecraft.server.Main",
                "--nogui",
            ],
            cwd=self.directory,
            stdin=subprocess.PIPE,
            stdout=self.log,
            stderr=subprocess.STDOUT,
            text=True,
        )
        self.wait_for("Done (", 40)

    def text(self):
        try:
            return self.log_path.read_text(encoding="utf-8", errors="ignore")
        except FileNotFoundError:
            return ""

    def wait_for(self, needle, timeout=None):
        deadline = time.time() + (timeout or self.timeout)
        while time.time() < deadline:
            if needle in self.text():
                return
            time.sleep(0.1)
        raise RuntimeError(f"timed out waiting for {needle!r}; see {self.log_path}")

    def command(self, command, settle=0.2):
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()
        time.sleep(settle)

    def command_now(self, command):
        self.process.stdin.write(command + "\n")
        self.process.stdin.flush()

    def query_line(self, command):
        before = len(self.text().splitlines())
        self.command(command, settle=0.15)
        lines = self.text().splitlines()[before:]
        return lines[-1] if lines else ""

    def query(self, command):
        line = self.query_line(command)
        if "Test passed" in line:
            return True
        if "Test failed" in line:
            return False
        return None

    def game_time(self):
        before = len(self.text().splitlines())
        self.command("time query gametime", settle=0.15)
        for line in reversed(self.text().splitlines()[before:]):
            match = re.search(r"The time is (\d+)", line)
            if match:
                return int(match.group(1))
        return None

    def stop(self):
        try:
            self.command_now("stop")
        except (BrokenPipeError, OSError):
            pass
        try:
            self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.log.close()


def block_state(server, position, predicate="minecraft:redstone_lamp[lit=true]"):
    x, y, z = position
    return server.query(f"execute if block {x} {y} {z} {predicate}")


def emit(trace, record, **fields):
    row = {"record": record, **fields}
    trace.write(json.dumps(row) + "\n")
    trace.flush()
    print(json.dumps(row), flush=True)


def run_chain(server, trace):
    """Queue a five-probe Block Event cascade, unload, then reload it."""
    fixture = "DEDICATED-BUD013-CHAIN"
    for position in CHAIN_SUPPORTS:
        server.command(
            f"setblock {position[0]} {position[1]} {position[2]} smooth_stone"
        )
    for position in CHAIN_DUST:
        server.command(
            f"setblock {position[0]} {position[1]} {position[2]} redstone_wire"
        )
    for position in CHAIN_PROBES:
        server.command(
            f"setblock {position[0]} {position[1]} {position[2]} "
            "piston[facing=south]"
        )
    server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
    time.sleep(0.5)
    for position in CHAIN_QC:
        server.command(
            f"setblock {position[0]} {position[1]} {position[2]} redstone_block"
        )
    time.sleep(0.5)
    ready_extended = sum(
        block_state(server, position, "minecraft:piston[extended=true]") is True
        for position in CHAIN_PROBES
    )
    emit(
        trace,
        "sample",
        fixture=fixture,
        phase="ready",
        gt=server.game_time(),
        extended=ready_extended,
    )

    before = server.game_time()
    server.command_now(
        f"setblock {CHAIN_SOURCE[0]} {CHAIN_SOURCE[1]} {CHAIN_SOURCE[2]} "
        "redstone_block"
    )
    server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
    time.sleep(2.0)
    first = CHAIN_PROBES[0]
    away_state = server.query_line(
        f"execute if block {first[0]} {first[1]} {first[2]} minecraft:piston"
    )
    emit(
        trace,
        "sample",
        fixture=fixture,
        phase="away",
        gt=before,
        loaded="not loaded" not in away_state.lower(),
        response=away_state,
    )

    server.command_now(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
    time.sleep(0.3)
    extended = sum(
        block_state(server, position, "minecraft:piston[extended=true]") is True
        for position in CHAIN_PROBES
    )
    heads = sum(
        block_state(server, position, "minecraft:piston_head[facing=south]") is True
        for position in CHAIN_HEADS
    )
    emit(
        trace,
        "sample",
        fixture=fixture,
        phase="after_return",
        gt=server.game_time(),
        extended=extended,
        heads=heads,
    )
    time.sleep(0.4)
    settled = sum(
        block_state(server, position, "minecraft:piston[extended=true]") is True
        for position in CHAIN_PROBES
    )
    emit(
        trace,
        "sample",
        fixture=fixture,
        phase="after_return_settled",
        gt=server.game_time(),
        extended=settled,
    )
    emit(trace, "complete", fixture=fixture)


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

    server = Server(args.prism_root.expanduser(), args.java, args.timeout)
    trace_path = server.directory / "trace.jsonl"
    trace = trace_path.open("w", encoding="utf-8")
    bot = None
    try:
        emit(trace, "run", edition="java", version="1.20.1", server=server.port)
        bot = Bot(server.port)
        server.wait_for(f"{REFERENCE} joined the game", 30)
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        time.sleep(0.5)
        emit(trace, "player", name=REFERENCE, spawn=list(SPAWN))

        # Establish a live, ticking lamp in the player chunk.
        server.command(f"setblock {LAMP[0]} {LAMP[1]} {LAMP[2]} redstone_lamp")
        server.command(f"setblock {POWER[0]} {POWER[1]} {POWER[2]} redstone_block")
        before = server.game_time()
        lit = block_state(server, LAMP)
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-LAMP",
            phase="powered",
            gt=before,
            lit=lit,
        )
        if lit is not True:
            raise RuntimeError("lamp did not light in the player chunk")

        # Remove power and leave the chunk before the 4 GT off callback.
        server.command_now(f"setblock {POWER[0]} {POWER[1]} {POWER[2]} air")
        server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
        left = server.game_time()
        time.sleep(0.9)
        away = server.game_time()
        away_state = server.query_line(
            f"execute if block {LAMP[0]} {LAMP[1]} {LAMP[2]} minecraft:redstone_lamp"
        )
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-LAMP",
            phase="away",
            gt=left,
            waited_gt=None if left is None or away is None else away - left,
            loaded="not loaded" not in away_state.lower(),
            response=away_state,
        )

        server.command_now(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        time.sleep(0.15)
        returned = server.game_time()
        lit_after = block_state(server, LAMP)
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-LAMP",
            phase="after_return",
            gt=returned,
            lit=lit_after,
        )
        time.sleep(0.4)
        lit_settled = block_state(server, LAMP)
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-LAMP",
            phase="after_return_settled",
            gt=server.game_time(),
            lit=lit_settled,
        )
        emit(trace, "complete", fixture="DEDICATED-BUD013-LAMP")

        # A piston is started just before the player leaves, then checked after
        # the chunk is reloaded. This exercises the Block Event/motion lifecycle
        # rather than only a scheduled lamp tick.
        server.command(f"setblock {PISTON[0]} {PISTON[1]} {PISTON[2]} air")
        server.command(
            f"setblock {PISTON[0]} {PISTON[1]} {PISTON[2]} piston[facing=east]"
        )
        server.command(
            f"setblock {PISTON_LOAD[0]} {PISTON_LOAD[1]} {PISTON_LOAD[2]} smooth_stone"
        )
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        before = server.game_time()
        server.command_now(
            f"setblock {PISTON_POWER[0]} {PISTON_POWER[1]} {PISTON_POWER[2]} redstone_block"
        )
        server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
        time.sleep(2.0)
        away_state = server.query_line(
            f"execute if block {PISTON[0]} {PISTON[1]} {PISTON[2]} minecraft:piston"
        )
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-PISTON",
            phase="away",
            gt=before,
            loaded="not loaded" not in away_state.lower(),
            response=away_state,
        )
        server.command_now(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        time.sleep(0.2)
        extended = block_state(
            server,
            PISTON,
            "minecraft:piston[extended=true]",
        )
        head = block_state(server, PISTON_HEAD, "minecraft:piston_head[facing=east]")
        load = block_state(server, PISTON_LOAD, "minecraft:smooth_stone")
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-PISTON",
            phase="after_return",
            gt=server.game_time(),
            extended=extended,
            head=head,
            load=load,
        )
        emit(trace, "complete", fixture="DEDICATED-BUD013-PISTON")

        # The same online-unload check across a vertical Section boundary.
        server.command(
            f"setblock {SECTION_PISTON[0]} {SECTION_PISTON[1]} {SECTION_PISTON[2]} "
            "piston[facing=up]"
        )
        server.command(
            f"setblock {SECTION_LOAD[0]} {SECTION_LOAD[1]} {SECTION_LOAD[2]} smooth_stone"
        )
        server.command(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        before = server.game_time()
        server.command_now(
            f"setblock {SECTION_POWER[0]} {SECTION_POWER[1]} {SECTION_POWER[2]} redstone_block"
        )
        server.command_now(f"tp {REFERENCE} {AWAY[0]} {AWAY[1]} {AWAY[2]}")
        time.sleep(2.0)
        away_state = server.query_line(
            f"execute if block {SECTION_PISTON[0]} {SECTION_PISTON[1]} {SECTION_PISTON[2]} "
            "minecraft:piston"
        )
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-SECTION",
            phase="away",
            gt=before,
            loaded="not loaded" not in away_state.lower(),
            response=away_state,
        )
        server.command_now(f"tp {REFERENCE} {SPAWN[0]} {SPAWN[1]} {SPAWN[2]}")
        time.sleep(0.2)
        extended = block_state(
            server, SECTION_PISTON, "minecraft:piston[extended=true]"
        )
        head = block_state(
            server, SECTION_HEAD, "minecraft:piston_head[facing=up]"
        )
        load = block_state(server, SECTION_LOAD, "minecraft:smooth_stone")
        emit(
            trace,
            "sample",
            fixture="DEDICATED-BUD013-SECTION",
            phase="after_return",
            gt=server.game_time(),
            extended=extended,
            head=head,
            load=load,
        )
        emit(trace, "complete", fixture="DEDICATED-BUD013-SECTION")

        run_chain(server, trace)

    finally:
        if bot is not None:
            bot.close()
        trace.close()
        server.stop()
        print(f"trace: {trace_path.relative_to(ROOT)}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, ValueError) as error:
        sys.exit(str(error))

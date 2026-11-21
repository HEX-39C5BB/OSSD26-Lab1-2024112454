package rustcraft.budagent;

import java.io.BufferedWriter;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Read-only Java agent for the local DedicatedServer BUD-013 probe.
 *
 * Queue snapshots are submitted through MinecraftServer.execute so every read
 * happens on the server thread. The writer thread only drains completed JSON
 * lines and never touches Minecraft state. Arguments are:
 *   OUTPUT[|CHUNK_X,CHUNK_Z]
 */
public final class BudAgent {
    private static final BlockingQueue<String> LINES = new LinkedBlockingQueue<>();

    private BudAgent() {}

    public static void premain(String argument, java.lang.instrument.Instrumentation instrumentation) {
        String[] parts = argument.split("\\|", 2);
        Path output = Path.of(parts[0]);
        int[] chunk = null;
        if (parts.length == 2) {
            String[] coordinates = parts[1].split(",", 2);
            chunk = new int[] {Integer.parseInt(coordinates[0]), Integer.parseInt(coordinates[1])};
        }
        final int[] targetChunk = chunk;

        Thread writer = new Thread(() -> write(output), "rustcraft-bud-writer");
        writer.setDaemon(true);
        writer.start();

        Thread sampler = new Thread(() -> sample(targetChunk), "rustcraft-bud-sampler");
        sampler.setDaemon(true);
        sampler.start();
    }

    private static void write(Path output) {
        try (BufferedWriter writer = Files.newBufferedWriter(
                output,
                StandardCharsets.UTF_8,
                StandardOpenOption.CREATE,
                StandardOpenOption.APPEND)) {
            while (true) {
                String line = LINES.take();
                writer.write(line);
                writer.newLine();
                writer.flush();
            }
        } catch (InterruptedException ignored) {
            Thread.currentThread().interrupt();
        } catch (IOException failure) {
            System.err.println("rustcraft bud writer failed: " + failure);
        }
    }

    private static void sample(int[] targetChunk) {
        long lastScheduled = Long.MIN_VALUE;
        AtomicBoolean pending = new AtomicBoolean();
        try {
            while (true) {
                Object server = findServer();
                if (server == null) {
                    Thread.sleep(50);
                    continue;
                }
                Object level = call(server, "D");
                if (level == null) {
                    Thread.sleep(50);
                    continue;
                }
                long tick = ((Number) call(level, "V")).longValue();
                if (tick != lastScheduled && pending.compareAndSet(false, true)) {
                    lastScheduled = tick;
                    Runnable snapshot = () -> {
                        try {
                            LINES.add(snapshot(tick, level, targetChunk));
                        } catch (ReflectiveOperationException | RuntimeException failure) {
                            System.err.println("rustcraft bud snapshot failed: " + failure);
                        } finally {
                            pending.set(false);
                        }
                    };
                    execute(server, snapshot);
                }
                Thread.sleep(20);
            }
        } catch (InterruptedException ignored) {
            Thread.currentThread().interrupt();
        } catch (ReflectiveOperationException failure) {
            System.err.println("rustcraft bud sampler failed: " + failure);
        }
    }

    private static void execute(Object server, Runnable task) throws ReflectiveOperationException {
        Method execute = server.getClass().getMethod("execute", Runnable.class);
        execute.setAccessible(true);
        execute.invoke(server, task);
    }

    private static Object findServer() throws ReflectiveOperationException {
        for (Thread thread : Thread.getAllStackTraces().keySet()) {
            if (!thread.getName().equals("Server thread")) {
                continue;
            }
            Field target = Thread.class.getDeclaredField("target");
            target.setAccessible(true);
            Object server = findServer(target.get(thread), 0);
            if (server != null) {
                return server;
            }
        }
        return null;
    }

    private static Object findServer(Object value, int depth) throws ReflectiveOperationException {
        if (value == null || depth > 4) {
            return null;
        }
        if (hasMethod(value, "D")) {
            return value;
        }
        if (value instanceof java.util.concurrent.atomic.AtomicReference<?> reference) {
            Object child = findServer(reference.get(), depth + 1);
            if (child != null) {
                return child;
            }
        }
        for (Field field : value.getClass().getDeclaredFields()) {
            if (Modifier.isStatic(field.getModifiers())) {
                continue;
            }
            try {
                field.setAccessible(true);
                Object child = findServer(field.get(value), depth + 1);
                if (child != null) {
                    return child;
                }
            } catch (RuntimeException ignored) {
                // A synthetic lambda field can be inaccessible; keep looking.
            }
        }
        return null;
    }

    private static boolean hasMethod(Object value, String name) {
        for (Method method : value.getClass().getMethods()) {
            if (method.getName().equals(name) && method.getParameterCount() == 0) {
                return true;
            }
        }
        return false;
    }

    private static String snapshot(long tick, Object level, int[] targetChunk)
            throws ReflectiveOperationException {
        StringBuilder json = new StringBuilder();
        json.append("{\"gt\":").append(tick).append(",\"block_ticks\":[");
        List<Object> ticks = containers(level);
        boolean first = true;
        for (Object container : ticks) {
            Object stream = call(container, "d");
            if (!(stream instanceof java.util.stream.Stream<?> values)) {
                continue;
            }
            Object[] entries = values.toArray();
            for (Object entry : entries) {
                if (!first) {
                    json.append(',');
                }
                first = false;
                json.append("{\"block\":").append(quote(String.valueOf(call(entry, "a"))))
                        .append(",\"pos\":").append(position(call(entry, "b")))
                        .append(",\"due\":").append(number(call(entry, "c")))
                        .append(",\"priority\":").append(priority(entry))
                        .append(",\"order\":").append(number(call(entry, "e")))
                        .append('}');
            }
        }
        json.append("],\"block_events\":[");
        List<Object> events = blockEvents(level);
        first = true;
        for (Object event : events) {
            if (!first) {
                json.append(',');
            }
            first = false;
            json.append("{\"pos\":").append(position(call(event, "a")))
                    .append(",\"block\":").append(quote(String.valueOf(call(event, "b"))))
                    .append(",\"id\":").append(number(call(event, "c")))
                    .append(",\"data\":").append(number(call(event, "d")))
                    .append('}');
        }
        json.append(']');
        if (targetChunk != null) {
            json.append(",\"chunks\":[").append(chunkState(level, targetChunk[0], targetChunk[1]))
                    .append(']');
        }
        json.append('}');
        return json.toString();
    }

    private static String chunkState(Object level, int x, int z) throws ReflectiveOperationException {
        Class<?> chunkClass = Class.forName("clt");
        Object key = chunkClass.getConstructor(int.class, int.class).newInstance(x, z);
        boolean ticking = (Boolean) level.getClass()
                .getMethod("a", chunkClass)
                .invoke(level, key);
        Object keyLong = chunkClass.getMethod("a").invoke(key);
        boolean ticket = (Boolean) level.getClass()
                .getMethod("a", long.class)
                .invoke(level, keyLong);
        Object chunkSource = call(level, "k");
        Object chunk = chunkSource.getClass()
                .getMethod("a", int.class, int.class)
                .invoke(chunkSource, x, z);
        return "{\"pos\":[" + x + "," + z + "],\"ticking\":" + ticking
                + ",\"ticket\":" + ticket + ",\"resident\":" + (chunk != null) + "}";
    }

    private static List<Object> containers(Object level) throws ReflectiveOperationException {
        Field field = level.getClass().getDeclaredField("R"); // ServerLevel.blockTicks
        field.setAccessible(true);
        Object ticks = field.get(level);
        Field containers = ticks.getClass().getDeclaredField("d"); // LevelTicks.allContainers
        containers.setAccessible(true);
        Object map = containers.get(ticks);
        List<Object> values = new ArrayList<>();
        for (Object value : ((Iterable<?>) map.getClass().getMethod("values").invoke(map))) {
            values.add(value);
        }
        return values;
    }

    private static List<Object> blockEvents(Object level) throws ReflectiveOperationException {
        Field field = level.getClass().getDeclaredField("V"); // ServerLevel.blockEvents
        field.setAccessible(true);
        Object events = field.get(level);
        List<Object> values = new ArrayList<>();
        for (Object value : (Iterable<?>) events) {
            values.add(value);
        }
        return values;
    }

    private static Object call(Object target, String name) throws ReflectiveOperationException {
        Method method = target.getClass().getMethod(name);
        method.setAccessible(true);
        return method.invoke(target);
    }

    private static String priority(Object entry) throws ReflectiveOperationException {
        Object priority = call(entry, "d");
        Object value = call(priority, "a");
        return number(value);
    }

    private static String number(Object value) {
        return value instanceof Number ? value.toString() : "null";
    }

    private static String position(Object value) throws ReflectiveOperationException {
        if (value == null) {
            return "null";
        }
        return "[" + number(call(value, "u")) + "," + number(call(value, "v")) + ","
                + number(call(value, "w")) + "]";
    }

    private static String quote(String value) {
        return "\"" + value.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }
}

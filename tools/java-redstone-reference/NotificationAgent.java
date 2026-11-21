package rustcraft.notifyagent;

import java.io.BufferedWriter;
import java.io.IOException;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.security.ProtectionDomain;
import java.util.ArrayDeque;
import java.util.Deque;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

import jdk.internal.org.objectweb.asm.ClassReader;
import jdk.internal.org.objectweb.asm.ClassVisitor;
import jdk.internal.org.objectweb.asm.ClassWriter;
import jdk.internal.org.objectweb.asm.Label;
import jdk.internal.org.objectweb.asm.MethodVisitor;
import jdk.internal.org.objectweb.asm.Opcodes;
import jdk.internal.org.objectweb.asm.Type;

/**
 * Read-only notification tracer for the local Java 1.20.1 DedicatedServer.
 *
 * The agent instruments only the verified Level and BlockStateBase notification
 * entry points. It logs method enter/exit, target/source/direction, depth and
 * parent id, and never changes arguments, return values, scheduling or world
 * state. The bounded queue and dropped counter make loss visible.
 */
public final class NotificationAgent {
    private static final int BUFFER_CAPACITY = 8192;
    private static final BlockingQueue<String> LINES =
            new ArrayBlockingQueue<>(BUFFER_CAPACITY);
    private static final AtomicLong DROPPED = new AtomicLong();
    private static final AtomicInteger NEXT_ID = new AtomicInteger();
    private static final ThreadLocal<Deque<Frame>> STACK =
            ThreadLocal.withInitial(ArrayDeque::new);
    private static volatile long gameTime;
    private static volatile Object serverLevel;
    private static volatile Method tickMethod;

    private NotificationAgent() {}

    public static void premain(String argument, Instrumentation instrumentation) {
        String[] parts = argument.split("\\|", 2);
        Path output = Path.of(parts[0]);
        startWriter(output);
        instrumentation.addTransformer(new Transformer(), false);
        startClock();
    }

    private static final class Frame {
        final int id;
        final String kind;

        Frame(int id, String kind) {
            this.id = id;
            this.kind = kind;
        }
    }

    public static int enter(
            String kind,
            Object a,
            Object b,
            Object c,
            Object d,
            Object e,
            int flags) {
        int id = NEXT_ID.incrementAndGet();
        Deque<Frame> stack = STACK.get();
        Frame parent = stack.peek();
        int depth = stack.size();
        long gt = currentTick();
        StringBuilder json = new StringBuilder(256);
        json.append("{\"record\":\"notify\",\"phase\":\"enter\",\"gt\":")
                .append(gt)
                .append(",\"gt_source\":\"server_thread\"")
                .append(",\"seq\":")
                .append(id)
                .append(",\"id\":")
                .append(id)
                .append(",\"parent\":")
                .append(parent == null ? 0 : parent.id)
                .append(",\"depth\":")
                .append(depth)
                .append(",\"kind\":")
                .append(quote(kind))
                .append(",\"thread\":")
                .append(quote(Thread.currentThread().getName()))
                .append(",\"dropped\":")
                .append(DROPPED.get())
                .append(",\"flags\":")
                .append(flags)
                .append(",\"args\":[")
                .append(value(a))
                .append(',')
                .append(value(b))
                .append(',')
                .append(value(c))
                .append(',')
                .append(value(d))
                .append(',')
                .append(value(e))
                .append("]}");
        emit(json.toString());
        stack.push(new Frame(id, kind));
        return id;
    }

    public static void exit(int id) {
        Deque<Frame> stack = STACK.get();
        Frame frame = stack.peek();
        if (frame == null || frame.id != id) {
            emit("{\"record\":\"notify-error\",\"gt\":" + gameTime
                    + ",\"id\":" + id + ",\"message\":\"unbalanced exit\"}");
            return;
        }
        stack.pop();
        emit("{\"record\":\"notify\",\"phase\":\"exit\",\"gt\":" + currentTick()
                + ",\"gt_source\":\"server_thread\""
                + ",\"seq\":" + id
                + ",\"id\":" + id
                + ",\"parent\":" + (stack.peek() == null ? 0 : stack.peek().id)
                + ",\"depth\":" + stack.size()
                + ",\"kind\":" + quote(frame.kind)
                + ",\"thread\":" + quote(Thread.currentThread().getName())
                + ",\"dropped\":" + DROPPED.get() + "}");
    }

    private static void emit(String line) {
        if (!LINES.offer(line)) {
            DROPPED.incrementAndGet();
        }
    }

    private static String value(Object value) {
        if (value == null) {
            return "null";
        }
        if (value instanceof Number || value instanceof Boolean) {
            return value.toString();
        }
        String text = value.toString();
        if (value.getClass().getName().equals("gu")) {
            try {
                Method x = value.getClass().getMethod("u");
                Method y = value.getClass().getMethod("v");
                Method z = value.getClass().getMethod("w");
                return "[" + x.invoke(value) + "," + y.invoke(value) + ","
                        + z.invoke(value) + "]";
            } catch (ReflectiveOperationException ignored) {
                // Fall through to the class string; never invent coordinates.
            }
        }
        return quote(text);
    }

    private static String quote(String value) {
        StringBuilder out = new StringBuilder(value.length() + 2);
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> out.append(c);
            }
        }
        out.append('"');
        return out.toString();
    }

    private static void startWriter(Path output) {
        Thread writer = new Thread(() -> {
            long lastDropped = -1;
            try (BufferedWriter out = Files.newBufferedWriter(
                    output,
                    StandardCharsets.UTF_8,
                    StandardOpenOption.CREATE,
                    StandardOpenOption.APPEND)) {
                while (true) {
                    String line = LINES.poll(20, TimeUnit.MILLISECONDS);
                    if (line != null) {
                        out.write(line);
                        out.newLine();
                    }
                    long dropped = DROPPED.get();
                    if (dropped != lastDropped) {
                        out.write("{\"record\":\"dropped\",\"gt\":" + gameTime
                                + ",\"count\":" + dropped + "}");
                        out.newLine();
                        lastDropped = dropped;
                    }
                    out.flush();
                }
            } catch (InterruptedException ignored) {
                Thread.currentThread().interrupt();
            } catch (IOException failure) {
                System.err.println("rustcraft notification writer failed: " + failure);
            }
        }, "rustcraft-notify-writer");
        writer.setDaemon(true);
        writer.start();
    }

    private static void startClock() {
        Thread clock = new Thread(() -> {
            while (true) {
                try {
                    Object server = findServer();
                    if (server != null) {
                        Object level = call(server, "D");
                        if (level != null) {
                            serverLevel = level;
                            tickMethod = level.getClass().getMethod("V");
                            gameTime = ((Number) tickMethod.invoke(level)).longValue();
                        }
                    }
                    Thread.sleep(10);
                } catch (InterruptedException ignored) {
                    Thread.currentThread().interrupt();
                    return;
                } catch (ReflectiveOperationException | RuntimeException ignored) {
                    // Keep the clock best-effort; hook events still carry the
                    // last successfully observed GT.
                }
            }
        }, "rustcraft-notify-clock");
        clock.setDaemon(true);
        clock.start();
    }

    private static long currentTick() {
        Object level = serverLevel;
        Method method = tickMethod;
        if (level != null && method != null) {
            try {
                return ((Number) method.invoke(level)).longValue();
            } catch (ReflectiveOperationException | RuntimeException ignored) {
                // Fall back to the sampled value and keep the event visible.
            }
        }
        return gameTime;
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
            return findServer(reference.get(), depth + 1);
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
                // Synthetic lambda fields may be inaccessible.
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

    private static Object call(Object target, String name) throws ReflectiveOperationException {
        Method method = target.getClass().getMethod(name);
        method.setAccessible(true);
        return method.invoke(target);
    }

    private static final class Transformer implements ClassFileTransformer {
        @Override
        public byte[] transform(
                Module module,
                ClassLoader loader,
                String className,
                Class<?> classBeingRedefined,
                ProtectionDomain protectionDomain,
                byte[] classfileBuffer) {
            if (!className.equals("cmm") && !className.equals("dca$a")) {
                return null;
            }
            ClassReader reader = new ClassReader(classfileBuffer);
            ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_MAXS);
            ClassVisitor visitor = new ClassVisitor(Opcodes.ASM8, writer) {
                @Override
                public MethodVisitor visitMethod(
                        int access,
                        String name,
                        String descriptor,
                        String signature,
                        String[] exceptions) {
                    MethodVisitor delegate =
                            super.visitMethod(access, name, descriptor, signature, exceptions);
                    if (delegate == null) {
                        return null;
                    }
                    if (className.equals("cmm")) {
                        if (name.equals("a") && descriptor.equals("(Lgu;Lcpn;)V")) {
                            return new Hook(delegate, access, descriptor, "update_neighbors_at", 0);
                        }
                        if (name.equals("a") && descriptor.equals("(Lgu;Lcpn;Lha;)V")) {
                            return new Hook(delegate, access, descriptor, "update_neighbors_except", 1);
                        }
                        if (name.equals("a") && descriptor.equals("(Lgu;Lcpn;Lgu;)V")) {
                            return new Hook(delegate, access, descriptor, "neighbor_changed_dispatch", 2);
                        }
                        if (name.equals("a")
                                && descriptor.equals("(Lha;Ldcb;Lgu;Lgu;II)V")) {
                            return new Hook(delegate, access, descriptor, "neighbor_shape_changed", 3);
                        }
                    } else {
                        if (name.equals("a")
                                && descriptor.equals("(Lcmm;Lgu;Lcpn;Lgu;Z)V")) {
                            return new Hook(delegate, access, descriptor, "neighbor_changed", 4);
                        }
                        if (name.equals("a")
                                && descriptor.equals("(Lha;Ldcb;Lcmn;Lgu;Lgu;)Ldcb;")) {
                            return new Hook(delegate, access, descriptor, "update_shape", 5);
                        }
                    }
                    return delegate;
                }
            };
            reader.accept(visitor, 0);
            return writer.toByteArray();
        }
    }

    private static final class Hook extends MethodVisitor {
        private final int access;
        private final String descriptor;
        private final String kind;
        private final int shape;
        private final int idLocal;
        private final int exitKind;

        Hook(MethodVisitor delegate, int access, String descriptor, String kind, int shape) {
            super(Opcodes.ASM8, delegate);
            this.access = access;
            this.descriptor = descriptor;
            this.kind = kind;
            this.shape = shape;
            int local = (access & Opcodes.ACC_STATIC) == 0 ? 1 : 0;
            for (Type type : Type.getArgumentTypes(descriptor)) {
                local += type.getSize();
            }
            this.idLocal = local;
            this.exitKind = shape;
        }

        @Override
        public void visitCode() {
            super.visitCode();
            visitLdcInsn(kind);
            switch (shape) {
                case 0 -> {
                    // Level.updateNeighborsAt(BlockPos, Block)
                    visitVarInsn(Opcodes.ALOAD, 0);
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitInsn(Opcodes.ACONST_NULL);
                    visitInsn(Opcodes.ACONST_NULL);
                    visitInsn(Opcodes.ICONST_0);
                }
                case 1 -> {
                    // Level.updateNeighborsAtExceptFromFacing(BlockPos, Block, Direction)
                    visitVarInsn(Opcodes.ALOAD, 0);
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitVarInsn(Opcodes.ALOAD, 3);
                    visitInsn(Opcodes.ACONST_NULL);
                    visitInsn(Opcodes.ICONST_0);
                }
                case 2 -> {
                    // Level.neighborChanged(BlockPos, Block, BlockPos)
                    visitVarInsn(Opcodes.ALOAD, 0);
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitVarInsn(Opcodes.ALOAD, 3);
                    visitInsn(Opcodes.ACONST_NULL);
                    visitInsn(Opcodes.ICONST_0);
                }
                case 3 -> {
                    // Level.neighborShapeChanged(Direction, BlockState, BlockPos, BlockPos, int, int)
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitVarInsn(Opcodes.ALOAD, 3);
                    visitVarInsn(Opcodes.ALOAD, 4);
                    visitInsn(Opcodes.ACONST_NULL);
                    visitVarInsn(Opcodes.ILOAD, 5);
                }
                case 4 -> {
                    // BlockState.neighborChanged(Level, BlockPos, Block, BlockPos, boolean)
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitVarInsn(Opcodes.ALOAD, 3);
                    visitVarInsn(Opcodes.ALOAD, 4);
                    visitVarInsn(Opcodes.ALOAD, 0);
                    visitVarInsn(Opcodes.ILOAD, 5);
                }
                case 5 -> {
                    // BlockState.updateShape(Direction, BlockState, LevelReader, BlockPos, BlockPos)
                    visitVarInsn(Opcodes.ALOAD, 1);
                    visitVarInsn(Opcodes.ALOAD, 2);
                    visitVarInsn(Opcodes.ALOAD, 3);
                    visitVarInsn(Opcodes.ALOAD, 4);
                    visitVarInsn(Opcodes.ALOAD, 5);
                    visitInsn(Opcodes.ICONST_0);
                }
                default -> throw new IllegalStateException("unknown hook shape " + shape);
            }
            visitMethodInsn(
                    Opcodes.INVOKESTATIC,
                    "rustcraft/notifyagent/NotificationAgent",
                    "enter",
                    "(Ljava/lang/String;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;Ljava/lang/Object;I)I",
                    false);
            visitVarInsn(Opcodes.ISTORE, idLocal);
        }

        @Override
        public void visitInsn(int opcode) {
            if (opcode >= Opcodes.IRETURN && opcode <= Opcodes.RETURN) {
                visitVarInsn(Opcodes.ILOAD, idLocal);
                visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        "rustcraft/notifyagent/NotificationAgent",
                        "exit",
                        "(I)V",
                        false);
            }
            super.visitInsn(opcode);
        }

        @Override
        public void visitMaxs(int maxStack, int maxLocals) {
            super.visitMaxs(maxStack, maxLocals);
        }
    }
}

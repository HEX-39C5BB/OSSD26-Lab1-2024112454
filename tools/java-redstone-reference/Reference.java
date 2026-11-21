import com.mojang.authlib.GameProfile;
import java.nio.file.Path;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Consumer;
import net.minecraft.server.MinecraftServer;

/**
 * Observes the installed Java 1.20.1 implementation through its GameTest server.
 * Short class/member names come from this version's mappings. This launcher
 * supplies fixture inputs and calls the unmodified level's tick entry point.
 */
public class Reference {
    private static final int Y = 6;
    private final MinecraftServer server;
    private final aif level;
    private final Field blockEvents;
    private final Field blockTickContainers;
    private final Method pistonPower;

    private Reference(MinecraftServer server) throws ReflectiveOperationException {
        this.server = server;
        this.level = server.D();
        this.blockEvents = aif.class.getDeclaredField("V"); // ServerLevel.blockEvents
        this.blockEvents.setAccessible(true);
        this.blockTickContainers = efq.class.getDeclaredField("d"); // LevelTicks.allContainers
        this.blockTickContainers.setAccessible(true);
        this.pistonPower = dbu.class.getDeclaredMethod("a", cnc.class, gu.class, ha.class);
        this.pistonPower.setAccessible(true); // PistonBaseBlock.getNeighborSignal
    }

    public static void main(String[] args) {
        try {
            runPhase(args);
            System.exit(0);
        } catch (Exception failure) {
            failure.printStackTrace();
            System.exit(1);
        }
    }

    private static void runPhase(String[] args) throws Exception {
        aa.a();   // SharedConstants.tryDetectVersion
        acs.a();  // Bootstrap.bootStrap
        if (args.length != 1) {
            throw new IllegalArgumentException("expected a reference phase");
        }
        switch (args[0]) {
            case "fixtures" -> {
                runWorld("reference", Reference::runFixtures);
                runWorld("torch-reopen", Reference::prepareTorchReopen);
                runWorld("torch-reopen", Reference::observeTorchReopen);
            }
            case "chunk-residency" -> runWorld("chunk-residency", Reference::chunkResidency);
            case "qc-save" -> runWorld("qc-reopen", Reference::prepareQcReopen);
            case "qc-load" -> runWorld("qc-reopen", Reference::observeQcReopen);
            default -> throw new IllegalArgumentException("unknown reference phase: " + args[0]);
        }
        emit("{\"record\":\"stage_complete\",\"stage\":\"" + args[0] + "\"}");
    }

    private static void runWorld(String world, Consumer<Reference> fixture) throws Exception {
        try (var storage = dyy.b(Path.of("worlds")).c(world)) {
            var packs = akl.a(storage);
            // The constructor requires a batch. We drive ServerLevel directly
            // so fixture input and observation phases do not depend on GameTest callbacks.
            var test = new qi("reference", "unused", "unused", 100, 0L, true, pq::g);
            var batch = new pm("reference", List.of(test), null, null);
            try (var game = pw.a(Thread.currentThread(), storage, packs, List.of(batch), new gu(0, 4, 0))) {
                if (!game.e()) {
                    throw new IllegalStateException("GameTest world creation failed");
                }
                fixture.accept(new Reference(game));
            }
        }
    }

    private void runFixtures() {
        emit("{\"record\":\"world\",\"world\":\"reference\",\"dimension\":\"minecraft:overworld\",\"seed\":" +
                level.A() + ",\"gt\":" + level.V() + "}");
        level.a(0, 0, true); // ServerLevel.setChunkForced
        step(3);
        directPiston();
        quasiPiston();
        shapeAndNeighborChanged();
        sameGameTickTransitions();
        repeatedSourceNotices();
        shapeAndNeighborFaces();
        neighborExclusionFaces();
        cancelPendingExtension();
        quasiPistonsAcrossSeams();
        bud007Range();
        bud007Directions();
        bud007Components();
        repeaters();
        lamp();
        comparator();
        observer();
        torch();
        levers();
        buttons();
        pressurePlates();
        ejectors();
        componentBudCases();
        wireExternalPower();
        wireShapes();
        wireNotices();
        bud007Chain();
        bud007LongChain();
        bud007MultiSource();
        bud007Corner();
        bud007Junction();
        bud007Section();
        stickyPistonPulse();
        stickyQuasiPiston();
        powerQueries();
        pistonPowerQueries();
        scheduledOrder();
        neighborOrder();
    }

    private void directPiston() {
        var fixture = "BUD-001";
        var base = new gu(4, Y, 2);
        var head = new gu(5, Y, 2);
        var power = new gu(3, Y, 2);
        set(fixture, base, "piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        expect(fixture, base, "after_command", "extended=false");
        untilMovingCompletes(fixture, base, head);
    }

    private void buttons() {
        var fixture = "TIM-BUTTON-STONE";
        var button = new gu(15, 134, 6);
        var support = offset(button, 0, -1, 0);
        var directLamp = offset(button, 1, 0, 0);
        var conductedLamp = offset(support, 1, 0, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, button, "stone_button[face=floor,facing=south]");
        set(fixture, directLamp, "redstone_lamp");
        set(fixture, conductedLamp, "redstone_lamp");
        clickButton(fixture, button);
        for (var toward : ha.values()) {
            samplePower(fixture, button, toward);
        }
        observe(fixture, directLamp, "after_player_use");
        observe(fixture, conductedLamp, "after_player_use");
        step(10);
        clickButton(fixture, button);
        step(8);
        sampleScheduledBlocks(fixture, 7, button, directLamp, conductedLamp);

        fixture = "TIM-BUTTON-OAK";
        button = new gu(7, 134, 11);
        support = offset(button, 0, 1, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, button, "oak_button[face=ceiling,facing=west]");
        clickButton(fixture, button);
        for (var toward : ha.values()) {
            samplePower(fixture, button, toward);
        }
        step(28);
        sampleScheduledBlocks(fixture, 4, button);
        clickButton(fixture, button);
        set(fixture, support, "air");
        observe(fixture, button, "after_support_removed");
        observeBlockTick(fixture, button);

        fixture = "TIM-BUTTON-WALL";
        button = new gu(7, 138, 11);
        support = offset(button, -1, 0, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, button, "stone_button[face=wall,facing=east]");
        clickButton(fixture, button);
        for (var toward : ha.values()) {
            samplePower(fixture, button, toward);
        }
        step(18);
        sampleScheduledBlocks(fixture, 4, button);
    }

    private void clickButton(String fixture, gu position) {
        useControl(fixture, position, "use_button");
        observeBlockTick(fixture, position);
    }

    private void useControl(String fixture, gu position, String operation) {
        var player = new aig(server, level, new GameProfile(new UUID(0, 3), "ControlFixture"));
        var hit = new eee(new eei(position.u() + 0.5, position.v() + 0.125, position.w() + 0.5),
                ha.a("up"), position, false);
        var result = level.a_(position).a(level, player, bdw.a, hit); // BlockState.use
        row("input", fixture, position, "between_gt", "operation", operation);
        observe(fixture, position, "after_player_use_" + result);
    }

    private void levers() {
        leverCircuit("floor", "south", new gu(15, 174, 5), 0, -1);
        leverCircuit("wall", "east", new gu(15, 182, 5), -1, 0);
        leverCircuit("ceiling", "west", new gu(9, 190, 9), 0, 1);
        leverStateWrite();
        leverPlacements();
    }

    private void leverCircuit(String mount, String facing, gu lever, int supportX, int supportY) {
        var fixture = "TIM-LEVER-" + mount;
        var support = offset(lever, supportX, supportY, 0);
        var direct = offset(lever, 1, 0, 0);
        var conducted = offset(support, 0, 0, -1);
        set(fixture, support, "smooth_stone");
        set(fixture, lever, "lever[face=" + mount + ",facing=" + facing + "]");
        set(fixture, direct, "redstone_lamp");
        set(fixture, conducted, "redstone_lamp");

        useControl(fixture, lever, "use_lever");
        for (var toward : ha.values()) {
            samplePower(fixture, lever, toward);
            samplePower(fixture, support, toward);
        }
        expect(fixture, direct, "after_player_use", "lit=true");
        expect(fixture, conducted, "after_player_use", "lit=true");
        useControl(fixture, lever, "use_lever");
        observeBlockTick(fixture, direct);
        observeBlockTick(fixture, conducted);
        step(1);
        useControl(fixture, lever, "use_lever");
        sampleScheduledBlocks(fixture, 5, lever, direct, conducted);

        set(fixture, support, "air");
        expect(fixture, lever, "after_support_removed", "minecraft:air");
        sampleScheduledBlocks(fixture, 5, lever, direct, conducted);
    }

    private void leverPlacements() {
        var fixture = "TIM-LEVER-PLACEMENT";
        var player = new aig(server, level, new GameProfile(new UUID(0, 3), "ControlFixture"));
        var template = new gu(7, 202, 7);
        set(fixture, offset(template, 0, -1, 0), "smooth_stone");
        set(fixture, template, "lever");
        var block = level.a_(template).b();
        var item = new cfz(block.k());
        for (var mountedToward : ha.values()) {
            var target = offset(template, 0, 4 + 4 * mountedToward.ordinal(), 0);
            var support = target.a(mountedToward);
            set(fixture, support, "smooth_stone");
            player.a(target.u() + 0.5, target.v(), target.w() + 0.5, 0, 0);
            var hit = new eee(new eei(
                    (target.u() + support.u() + 1) * 0.5,
                    (target.v() + support.v() + 1) * 0.5,
                    (target.w() + support.w() + 1) * 0.5),
                    mountedToward.g(), support, false);
            var state = block.a(new cih(player, bdw.a, item, hit));
            if (state == null) {
                throw new IllegalStateException("original lever could not place on " + mountedToward);
            }
            row("lever_placement", fixture, target, mountedToward.toString(),
                    "state", state.toString());
        }
    }

    private void leverStateWrite() {
        var fixture = "TIM-LEVER-state-write";
        var lever = new gu(9, 198, 9);
        var support = offset(lever, 0, -1, 0);
        var direct = offset(lever, 1, 0, 0);
        var conducted = offset(support, 0, 0, -1);
        set(fixture, support, "smooth_stone");
        set(fixture, lever, "lever[face=floor,facing=south]");
        set(fixture, direct, "redstone_lamp");
        set(fixture, conducted, "redstone_lamp");

        set(fixture, lever, "lever[face=floor,facing=south,powered=true]");
        expect(fixture, direct, "after_state_write", "lit=true");
        expect(fixture, conducted, "after_state_write", "lit=false");
        power(fixture, support, ha.a("north"), 0, 0, 15);
        sampleScheduledBlocks(fixture, 2, conducted);
        notifyNeighbor(fixture, conducted, support);
        expect(fixture, conducted, "after_effective_neighbor_change", "lit=true");
    }

    private void pressurePlates() {
        int y = 142;
        for (var name : List.of("stone_pressure_plate", "oak_pressure_plate",
                "light_weighted_pressure_plate", "heavy_weighted_pressure_plate")) {
            var fixture = "TIM-PLATE-" + name;
            var plate = new gu(9, y, 9);
            y += 3;
            set(fixture, offset(plate, 0, -1, 0), "smooth_stone");
            set(fixture, plate, name);
            var actor = "summon minecraft:armor_stand " + (plate.u() + 0.5) + " " +
                    plate.v() + " " + (plate.w() + 0.5) +
                    " {NoGravity:1b,Tags:[\"rc_plate\"],UUID:[I;0,0,0,91]}";
            if (server.aC().a(server.aD(), actor) < 1) {
                throw new IllegalStateException("could not summon the plate fixture entity");
            }
            var stand = level.a(new UUID(0, 91));
            if (stand == null) {
                throw new IllegalStateException("plate fixture entity is missing");
            }
            level.a_(plate).a(level, plate, stand); // BlockState.entityInside
            row("input", fixture, plate, "between_gt", "operation", "occupy_plate");
            observe(fixture, plate, "after_entity_inside");
            observeBlockTick(fixture, plate);
            sampleScheduledBlocks(fixture, 3, plate);
            for (var toward : ha.values()) {
                samplePower(fixture, plate, toward);
            }
            // Remain on the plate through its first recheck.
            step(name.contains("weighted") ? 7 : 17);
            sampleScheduledBlocks(fixture, 1, plate);
            if (server.aC().a(server.aD(), "kill @e[type=armor_stand,tag=rc_plate]") < 1) {
                throw new IllegalStateException("could not remove the plate fixture entity");
            }
            row("input", fixture, plate, "between_gt", "operation", "leave_plate");
            step(name.contains("weighted") ? 7 : 17);
            sampleScheduledBlocks(fixture, 4, plate);
        }
    }

    private void ejectors() {
        ejectorQuasiPower("dispenser", new gu(15, 160, 6));
        ejectorQuasiPower("dropper", new gu(15, 170, 6));
        dropperTransfer();
        ejectorComparatorOutput();
    }

    private void ejectorQuasiPower(String type, gu position) {
        var fixture = "TIM-EJECTOR-QC-" + type;
        var quasi = offset(position, 0, 1, -1);
        var notice = offset(position, 0, -1, 0);
        var observer = offset(position, 0, 1, 0);
        set(fixture, position, type + "[facing=east]");
        set(fixture, observer, "observer[facing=down]");
        loadEjector(fixture, position, "{Slot:4b,id:\"minecraft:smooth_stone\",Count:2b}");
        step(6);
        set(fixture, quasi, "redstone_block");
        expect(fixture, position, "quasi_without_notice", "triggered=false");
        observeBlockTick(fixture, position);
        sampleScheduledBlocks(fixture, 2, position, observer);

        set(fixture, notice, "glass");
        expect(fixture, position, "quasi_with_notice", "triggered=true");
        observeBlockTick(fixture, position);
        observeEjectorItems(fixture, position);
        step(1);
        set(fixture, quasi, "air");
        expect(fixture, position, "quasi_lost_without_notice", "triggered=true");
        set(fixture, notice, "air");
        expect(fixture, position, "quasi_lost_with_notice", "triggered=false");
        observeBlockTick(fixture, position);
        for (int tick = 0; tick < 4; tick++) {
            sampleScheduledBlocks(fixture, 1, position, observer);
            observeEjectorItems(fixture, position);
            observeEjectedItems(fixture, position);
        }
        expectEjectorInventory(fixture, position, 1);
        // Input from the face which would be excluded by a piston.
        var front = offset(position, 1, 0, 0);
        set(fixture, front, "redstone_block");
        expect(fixture, position, "direct_from_front", "triggered=true");
        observeBlockTick(fixture, position);
        sampleScheduledBlocks(fixture, 5, position);
        expectEjectorInventory(fixture, position, 0);
    }

    private void dropperTransfer() {
        var fixture = "TIM-EJECTOR-TRANSFER";
        var source = new gu(8, 180, 6);
        var target = offset(source, 1, 0, 0);
        var power = offset(source, -1, 0, 0);
        set(fixture, source, "dropper[facing=east]");
        set(fixture, target, "dispenser[facing=east]");
        loadEjector(fixture, source, "{Slot:4b,id:\"minecraft:smooth_stone\",Count:2b}");
        loadEjector(fixture, target, "{Slot:1b,id:\"minecraft:smooth_stone\",Count:63b}");
        set(fixture, power, "redstone_block");
        sampleScheduledBlocks(fixture, 4, source);
        observeEjectorItems(fixture, source);
        observeEjectorItems(fixture, target);
        observeEjectedItems(fixture, source);
        set(fixture, power, "air");
        var full = new ArrayList<String>();
        for (int slot = 0; slot < 9; slot++) {
            full.add("{Slot:" + slot + "b,id:\"minecraft:smooth_stone\",Count:64b}");
        }
        loadEjector(fixture, target, full.toArray(String[]::new));
        set(fixture, power, "redstone_block");
        sampleScheduledBlocks(fixture, 4, source);
        expectEjectorInventory(fixture, source, 1);
        observeEjectorItems(fixture, source);
        observeEjectorItems(fixture, target);
        observeEjectedItems(fixture, source);
    }

    private void ejectorComparatorOutput() {
        var fixture = "TIM-EJECTOR-COMPARATOR";
        var ejector = new gu(8, 188, 6);
        var comparator = offset(ejector, 0, 0, 1);
        var power = offset(ejector, -1, 0, 0);
        set(fixture, ejector, "dispenser[facing=east]");
        set(fixture, offset(comparator, 0, -1, 0), "smooth_stone");
        set(fixture, comparator, "comparator[facing=north]");
        loadEjector(fixture, ejector, "{Slot:4b,id:\"minecraft:smooth_stone\",Count:42b}");
        sampleScheduledBlocks(fixture, 3, comparator);
        comparatorPower(fixture, comparator, "south");
        set(fixture, power, "redstone_block");
        sampleScheduledBlocks(fixture, 6, ejector, comparator);
        comparatorPower(fixture, comparator, "south");
        observeEjectorItems(fixture, ejector);
    }

    private void loadEjector(String fixture, gu position, String... stacks) {
        var command = "data merge block " + position.u() + " " + position.v() + " " + position.w() +
                " {Items:[" + String.join(",", stacks) + "]}";
        if (server.aC().a(server.aD(), command) < 1) {
            throw new IllegalStateException("could not stock ejector " + position);
        }
        row("input", fixture, position, "between_gt", "command", command.replace("\"", "\\\""));
    }

    private void observeEjectorItems(String fixture, gu position) {
        var container = (dae) level.c_(position);
        List<String> slots = new ArrayList<>();
        for (int slot = 0; slot < 9; slot++) {
            var stack = container.a(slot);
            if (!stack.b()) {
                slots.add("{\"slot\":" + slot + ",\"count\":" + stack.L() + ",\"item\":\"" + stack.d() + "\"}");
            }
        }
        emit("{\"record\":\"ejector_inventory\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"pos\":[" + position.u() + "," + position.v() + "," + position.w() +
                "],\"slots\":[" + String.join(",", slots) + "]}");
    }

    private void expectEjectorInventory(String fixture, gu position, int count) {
        var container = (dae) level.c_(position);
        int actual = 0;
        for (int slot = 0; slot < 9; slot++) {
            actual += container.a(slot).L();
        }
        if (actual != count) {
            throw new IllegalStateException(fixture + " GT " + level.V() + " expected " + count +
                    " items, saw " + actual);
        }
    }

    private void observeEjectedItems(String fixture, gu source) {
        List<String> items = new ArrayList<>();
        for (var entity : level.y()) {
            if (entity instanceof bvh item && item.k() <= 20) {
                // Ignore stale item entities left by earlier fixtures in the
                // same area; ejection observations only cover fresh drops.
                var pos = item.dg();
                if (Math.abs(pos.a() - source.u() - 0.5) < 3 &&
                        Math.abs(pos.b() - source.v() - 0.5) < 3 &&
                        Math.abs(pos.c() - source.w() - 0.5) < 3) {
                    items.add("{\"count\":" + item.j().L() + ",\"item\":\"" + item.j().d() +
                            "\",\"position\":[" + pos.a() + "," + pos.b() + "," + pos.c() +
                            "],\"age\":" + item.k() + "}");
                }
            }
        }
        emit("{\"record\":\"ejected_items\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"pos\":[" + source.u() + "," + source.v() + "," + source.w() +
                "],\"items\":[" + String.join(",", items) + "]}");
    }

    private void pistonPowerQueries() {
        var position = new gu(10, 24, 8);
        int[][] offsets = {
            {-1, 0, 0}, {1, 0, 0}, {0, -1, 0}, {0, 1, 0}, {0, 0, -1}, {0, 0, 1},
            {-1, 1, 0}, {1, 1, 0}, {0, 2, 0}, {0, 1, -1}, {0, 1, 1},
            {-1, -1, 0}, {1, 0, 1}, {2, 1, 0},
        };
        for (var facing : ha.values()) {
            set("PWR-PISTON-GEOMETRY", position, "piston[facing=" + facing + "]");
            for (int[] offset : offsets) {
                var source = new gu(position.u() + offset[0], position.v() + offset[1], position.w() + offset[2]);
                set("PWR-PISTON-GEOMETRY", source, "redstone_block");
                boolean powered;
                try {
                    powered = (boolean) pistonPower.invoke(level.a_(position).b(), level, position, facing);
                } catch (ReflectiveOperationException failure) {
                    throw new IllegalStateException("cannot query original piston power", failure);
                }
                emit("{\"record\":\"piston_power_query\",\"fixture\":\"PWR-PISTON-GEOMETRY\",\"gt\":" +
                        level.V() + ",\"facing\":\"" + facing + "\",\"offset\":[" + offset[0] +
                        "," + offset[1] + "," + offset[2] + "],\"powered\":" + powered + "}");
                set("PWR-PISTON-GEOMETRY", source, "air");
            }
        }
    }

    private void quasiPiston() {
        var base = new gu(4, Y, 6);
        var head = new gu(5, Y, 6);
        var power = new gu(4, Y + 1, 5);
        var neighbor = new gu(4, Y - 1, 6);
        set("BUD-003", base, "piston[facing=east]");
        step(2);
        set("BUD-003", power, "redstone_block");
        expectEventCount("BUD-003", base, "after_command", 0);
        expect("BUD-003", base, "after_command", "extended=false");
        for (int count = 0; count < 5; count++) {
            step(1);
            expect("BUD-003", base, "after_level_tick", "extended=false");
        }

        set("BUD-004", neighbor, "glass");
        expect("BUD-004", base, "after_nc", "extended=false");
        expectEventCount("BUD-004", base, "after_nc", 1);
        untilMovingCompletes("BUD-004", base, head);

        set("BUD-005", power, "air");
        expectEventCount("BUD-005", base, "after_command", 0);
        for (int count = 0; count < 5; count++) {
            step(1);
            expect("BUD-005", base, "after_level_tick", "extended=true");
        }

        set("BUD-006", neighbor, "air");
        expectEventCount("BUD-006", base, "after_nc", 1);
        step(1);
        observe("BUD-006", base, "after_level_tick");
        for (int count = 0; count < 4; count++) {
            step(1);
            observe("BUD-006", base, "after_level_tick");
        }
        expect("BUD-006", base, "settled", "extended=false");
    }

    private void cancelPendingExtension() {
        var fixture = "BUD-010";
        var base = new gu(4, Y, 10);
        var power = new gu(4, Y + 1, 9);
        var neighbor = new gu(4, Y - 1, 10);
        set(fixture, base, "piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        set(fixture, neighbor, "glass");
        expectEventCount(fixture, base, "after_nc", 1);
        set(fixture, power, "air");
        expectEventCount(fixture, base, "after_power_removed", 1);
        for (int count = 0; count < 5; count++) {
            step(1);
            expect(fixture, base, "after_level_tick", "extended=false");
            expectEventCount(fixture, base, "after_level_tick", 0);
        }
    }

    private void shapeAndNeighborChanged() {
        var fixture = "BUD-008";
        var base = new gu(7, Y, 12);
        var power = new gu(7, Y + 1, 11);
        var source = new gu(7, Y - 1, 12);
        set(fixture, base, "piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        var down = Arrays.stream(ha.values()).filter(d -> d.toString().equals("down")).findFirst().orElseThrow();
        var sourceState = level.a_(source);
        level.a(down, sourceState, base, source, 0, 512); // Level.neighborShapeChanged
        row("input", fixture, base, "between_gt", "operation", "shape_update_from_down");
        expectEventCount(fixture, base, "after_shape_update", 0);
        step(1);
        expect(fixture, base, "after_level_tick", "extended=false");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_neighbor_changed", 1);
        step(1);
        expect(fixture, base, "after_level_tick", "extended=true");
    }

    private void sameGameTickTransitions() {
        var fixture = "BUD-009";
        var base = new gu(7, Y, 7);
        var power = new gu(7, Y + 1, 6);
        var source = new gu(7, Y - 1, 7);
        set(fixture, base, "piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        notifyNeighbor("BUD-002", base, source);
        expectEventCount("BUD-002", base, "after_neighbor_changed", 1);
        set(fixture, power, "air");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_off", 1);
        set(fixture, power, "redstone_block");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_second_on", 1);
        expect(fixture, base, "after_second_on", "extended=false");
        step(1);
        expectEventCount(fixture, base, "after_level_tick", 0);
        expect(fixture, base, "after_level_tick", "extended=true");
    }

    private void repeatedSourceNotices() {
        var fixture = "BUD008-REPEAT-A";
        level.a(0, 2, true);
        observeChunkActivity(fixture, 0, 2, true);
        var base = new gu(4, Y, 40);
        var power = new gu(4, Y + 1, 39);
        var source = new gu(4, Y - 1, 40);
        set(fixture, base, "piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        set(fixture, source, "glass");
        expectEventCount(fixture, base, "after_first_notice", 1);
        set(fixture, source, "air");
        expectEventCount(fixture, base, "after_second_notice", 1);
        set(fixture, source, "glass");
        expectEventCount(fixture, base, "after_third_notice", 1);
        expect(fixture, base, "after_third_notice", "extended=false");
        step(1);
        expectEventCount(fixture, base, "after_level_tick", 0);
        expect(fixture, base, "after_level_tick", "extended=true");
    }

    private void shapeAndNeighborFaces() {
        level.a(2, 3, true);
        String[] faces = {"west", "east", "down", "up", "north", "south"};
        observeChunkActivity("BUD008-FACE-setup", 2, 3, true);
        observeChunkActivity("BUD008-FACE-setup", 3, 3, true);
        for (int i = 0; i < faces.length; i++) {
            var fixture = "BUD008-FACE-" + faces[i].toUpperCase();
            var base = new gu(34 + 3 * i, Y, 50);
            var source = offset(base,
                    faces[i].equals("west") ? -1 : faces[i].equals("east") ? 1 : 0,
                    faces[i].equals("down") ? -1 : faces[i].equals("up") ? 1 : 0,
                    faces[i].equals("north") ? -1 : faces[i].equals("south") ? 1 : 0);
            var power = offset(base, 0, 1, -1);
            set(fixture, source, "glass");
            set(fixture, base, "piston[facing=east]");
            step(2);
            set(fixture, power, "redstone_block");
            step(1);

            var shapeDirection = direction(faces[i]);
            level.a(shapeDirection, level.a_(source), base, source, 0, 512);
            row("input", fixture, base, "between_gt", "operation",
                    "shape_update_from_" + faces[i]);
            expectEventCount(fixture, base, "after_shape_update", 0);
            step(1);
            expect(fixture, base, "after_shape_only", "extended=false");
            notifyNeighbor(fixture, base, source);
            expectEventCount(fixture, base, "after_neighbor_changed", 1);
            step(1);
            expect(fixture, base, "after_neighbor_changed", "extended=true");
        }
    }

    private void neighborExclusionFaces() {
        level.a(3, 3, true);
        level.a(4, 3, true);
        String[] faces = {"west", "east", "down", "up", "north", "south"};
        observeChunkActivity("BUD008-EXCLUDE-setup", 3, 3, true);
        observeChunkActivity("BUD008-EXCLUDE-setup", 4, 3, true);
        for (int i = 0; i < faces.length; i++) {
            var fixture = "BUD008-EXCLUDE-" + faces[i].toUpperCase();
            var source = new gu(50 + 3 * i, Y, 60);
            set(fixture, source, "glass");
            var excluded = direction(faces[i]);
            level.a(source, level.a_(source).b(), excluded);
            row("input", fixture, source, "between_gt", "operation",
                    "update_neighbors_except_" + faces[i]);
        }
    }

    private void componentBudCases() {
        stickyBudShape(
                "BUDX-STICKY-008",
                new gu(12, Y, 2),
                new gu(12, Y + 1, 1),
                new gu(12, Y - 1, 2),
                new gu(13, Y, 2));
        stickyBudTransitions(
                "BUDX-STICKY-009",
                new gu(12, Y, 4),
                new gu(12, Y + 1, 3),
                new gu(12, Y - 1, 4));
        ejectorBudShape(
                "BUDX-DISPENSER-008",
                "dispenser[facing=east]",
                new gu(2, Y, 11),
                new gu(2, Y + 1, 10),
                new gu(2, Y - 1, 11));
        ejectorBudTransitions(
                "BUDX-DISPENSER-009",
                "dispenser[facing=east]",
                new gu(13, Y, 11),
                new gu(13, Y + 1, 10),
                new gu(13, Y - 1, 11));
        ejectorBudShape(
                "BUDX-DROPPER-008",
                "dropper[facing=east]",
                new gu(2, Y, 15),
                new gu(2, Y + 1, 14),
                new gu(2, Y - 1, 15));
        ejectorBudTransitions(
                "BUDX-DROPPER-009",
                "dropper[facing=east]",
                new gu(13, Y, 15),
                new gu(13, Y + 1, 14),
                new gu(13, Y - 1, 15));
    }

    private void stickyBudShape(String fixture, gu base, gu power, gu source, gu head) {
        set(fixture, base, "sticky_piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        expectEventCount(fixture, base, "after_quasi_power", 0);
        expect(fixture, base, "after_quasi_power", "extended=false");
        level.a(direction("down"), level.a_(source), base, source, 0, 512);
        row("input", fixture, base, "between_gt", "operation", "shape_update_from_down");
        expectEventCount(fixture, base, "after_shape_update", 0);
        step(1);
        expect(fixture, base, "after_level_tick", "extended=false");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_neighbor_changed", 1);
        untilMovingCompletes(fixture, base, head);
    }

    private void stickyBudTransitions(String fixture, gu base, gu power, gu source) {
        set(fixture, base, "sticky_piston[facing=east]");
        step(2);
        set(fixture, power, "redstone_block");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_neighbor_changed", 1);
        set(fixture, power, "air");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_off", 1);
        set(fixture, power, "redstone_block");
        notifyNeighbor(fixture, base, source);
        expectEventCount(fixture, base, "after_second_on", 1);
        expect(fixture, base, "after_second_on", "extended=false");
        step(1);
        expectEventCount(fixture, base, "after_level_tick", 0);
        expect(fixture, base, "after_level_tick", "extended=true");
    }

    private void ejectorBudShape(
            String fixture, String block, gu base, gu power, gu source) {
        set(fixture, base, block);
        loadEjector(fixture, base, "{Slot:4b,id:\"minecraft:smooth_stone\",Count:2b}");
        step(2);
        set(fixture, power, "redstone_block");
        expect(fixture, base, "after_quasi_power", "triggered=false");
        observeBlockTick(fixture, base);
        level.a(direction("down"), level.a_(source), base, source, 0, 512);
        row("input", fixture, base, "between_gt", "operation", "shape_update_from_down");
        expect(fixture, base, "after_shape_update", "triggered=false");
        observeBlockTick(fixture, base);
        notifyNeighbor(fixture, base, source);
        expect(fixture, base, "after_neighbor_changed", "triggered=true");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 4, base);
        observeEjectorItems(fixture, base);
        observeEjectedItems(fixture, base);
    }

    private void ejectorBudTransitions(
            String fixture, String block, gu base, gu power, gu source) {
        set(fixture, base, block);
        loadEjector(fixture, base, "{Slot:4b,id:\"minecraft:smooth_stone\",Count:2b}");
        step(2);
        set(fixture, power, "redstone_block");
        notifyNeighbor(fixture, base, source);
        expect(fixture, base, "after_neighbor_changed", "triggered=true");
        observeBlockTick(fixture, base);
        set(fixture, power, "air");
        notifyNeighbor(fixture, base, source);
        expect(fixture, base, "after_off", "triggered=false");
        observeBlockTick(fixture, base);
        set(fixture, power, "redstone_block");
        notifyNeighbor(fixture, base, source);
        expect(fixture, base, "after_second_on", "triggered=true");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 4, base);
        observeEjectorItems(fixture, base);
        observeEjectedItems(fixture, base);
    }

    private static ha direction(String name) {
        return Arrays.stream(ha.values())
                .filter(direction -> direction.toString().equals(name))
                .findFirst()
                .orElseThrow();
    }

    private void notifyNeighbor(String fixture, gu target, gu source) {
        level.a(target, level.a_(source).b(), source); // Level.neighborChanged
        row("input", fixture, target, "between_gt", "operation",
                "neighbor_changed_from_" + source.u() + "_" + source.v() + "_" + source.w());
    }

    private void quasiPistonsAcrossSeams() {
        for (var chunk : new int[][] {{-1,0}, {1,0}, {0,-1}, {0,1}}) {
            level.a(chunk[0], chunk[1], true);
        }
        step(2);
        var directions = List.of("south", "east", "north", "west", "down", "up");
        int[][] bases = {{8,16,15}, {15,16,8}, {8,16,0}, {0,16,8}, {11,16,11}, {4,15,11}};
        int[][] offsets = {{0,0,1}, {1,0,0}, {0,0,-1}, {-1,0,0}, {0,-1,0}, {0,1,0}};
        for (int i = 0; i < directions.size(); i++) {
            var facing = directions.get(i);
            var fixture = "BUD-011-" + facing;
            var base = new gu(bases[i][0], bases[i][1], bases[i][2]);
            var head = offset(base, offsets[i][0], offsets[i][1], offsets[i][2]);
            var source = offset(base, i % 2 == 0 ? 1 : 0, 1, i % 2 == 0 ? 0 : -1);
            var notice = i < 4 ? offset(base, 0, -1, 0) : offset(base, -1, 0, 0);
            set(fixture, base, "piston[facing=" + facing + "]");
            set(fixture, source, "redstone_block");
            expectEventCount(fixture, base, "after_quasi_power", 0);
            if (i == 0) {
                set("BUD-007", offset(base, 2, 0, 0), "glass");
                expectEventCount("BUD-007", base, "after_unrelated_change", 0);
            }
            step(1);
            expect(fixture, base, "after_level_tick", "extended=false");
            set(fixture, notice, "glass");
            expectEventCount(fixture, base, "after_nc", 1);
            untilMovingCompletes(fixture, base, head);
        }
    }

    private void bud007Range() {
        int[][] offsets = {
            {0, -1, 0},  // direct NC source below
            {0, 0, -1},  // adjacent north
            {0, 0, 1},   // adjacent south
            {-1, 0, 0},  // adjacent west
            {1, 0, 0},   // head/front cell
            {2, 0, 0},   // two blocks east
            {1, 0, 1},   // diagonal
            {0, 0, -2},  // two blocks north
            {0, 1, 0},   // above base
        };
        String[] names = {
            "down", "north", "south", "west", "front", "far_east", "diagonal", "far_north", "above",
        };
        for (int i = 0; i < offsets.length; i++) {
            var base = new gu(4, Y, 14 + 2 * i);
            var power = offset(base, 0, 1, -1);
            var test = offset(base, offsets[i][0], offsets[i][1], offsets[i][2]);
            var fixture = "BUD007R-" + names[i];
            set(fixture, base, "piston[facing=east]");
            step(2);
            set(fixture, power, "redstone_block");
            expectEventCount(fixture, base, "after_quasi_power", 0);
            set(fixture, test, "glass");
            observeBlockEvents(fixture, base, "after_change");
            observe(fixture, base, "after_change");
            step(1);
        }
    }

    private void bud007Directions() {
        for (int cx = 6; cx <= 8; cx++) {
            for (int cz = 6; cz <= 7; cz++) {
                level.a(cx, cz, true);
            }
        }
        step(2);
        for (int cx = 6; cx <= 8; cx++) {
            for (int cz = 6; cz <= 7; cz++) {
                observeChunkActivity("BUD007D-setup", cx, cz, true);
            }
        }
        String[] directions = {"east", "south", "west", "north"};
        int[][] facing = {{1, 0, 0}, {0, 0, 1}, {-1, 0, 0}, {0, 0, -1}};
        int[] baseX = {100, 106, 112, 118};
        String[] labels = {"down", "far_facing", "diagonal"};
        for (int d = 0; d < directions.length; d++) {
            for (int o = 0; o < labels.length; o++) {
                var base = new gu(baseX[d], Y, 100 + 5 * o);
                var power = offset(base, 0, 1, -1);
                var test = o == 0
                        ? offset(base, 0, -1, 0)
                        : o == 1
                                ? offset(base, 2 * facing[d][0], 0, 2 * facing[d][2])
                                : offset(base, 1, 0, 1);
                var fixture = "BUD007D-" + directions[d] + "-" + labels[o];
                set(fixture, base, "piston[facing=" + directions[d] + "]");
                step(2);
                set(fixture, power, "redstone_block");
                expectEventCount(fixture, base, "after_quasi_power", 0);
                set(fixture, test, "glass");
                observeBlockEvents(fixture, base, "after_change");
                observe(fixture, base, "after_change");
                step(1);
            }
        }

        level.a(5, 5, true);
        observeChunkActivity("BUD007S-setup", 5, 5, true);
        var seamBase = new gu(90, Y, 90);
        var seamPower = offset(seamBase, 0, 1, -1);
        var seamTest = offset(seamBase, 2, 0, 0);
        set("BUD007S-seam", seamBase, "piston[facing=east]");
        step(2);
        set("BUD007S-seam", seamPower, "redstone_block");
        expectEventCount("BUD007S-seam", seamBase, "after_quasi_power", 0);
        set("BUD007S-seam", seamTest, "glass");
        observeBlockEvents("BUD007S-seam", seamBase, "after_change");
        observe("BUD007S-seam", seamBase, "after_change");
    }

    private void bud007Components() {
        level.a(8, 6, true);
        level.a(9, 6, true);
        step(2);
        for (var fixture : new String[]{
                "BUD007C-STICKY-setup", "BUD007C-DISPENSER-setup", "BUD007C-DROPPER-setup"}) {
            observeChunkActivity(fixture, 8, 6, true);
            observeChunkActivity(fixture, 9, 6, true);
        }
        String[] labels = {"down", "far_facing", "diagonal"};
        int[][] offsets = {{0, -1, 0}, {2, 0, 0}, {1, 0, 1}};
        for (int i = 0; i < labels.length; i++) {
            var sticky = new gu(130, Y, 100 + 5 * i);
            var stickyPower = offset(sticky, 0, 1, -1);
            var stickyTest = offset(sticky, offsets[i][0], offsets[i][1], offsets[i][2]);
            var stickyFixture = "BUD007C-STICKY-" + labels[i];
            set(stickyFixture, sticky, "sticky_piston[facing=east]");
            step(2);
            set(stickyFixture, stickyPower, "redstone_block");
            expectEventCount(stickyFixture, sticky, "after_quasi_power", 0);
            set(stickyFixture, stickyTest, "glass");
            observeBlockEvents(stickyFixture, sticky, "after_change");
            observe(stickyFixture, sticky, "after_change");

            var dispenser = new gu(140, Y, 100 + 5 * i);
            var dispenserPower = offset(dispenser, 0, 1, -1);
            var dispenserTest = offset(dispenser, offsets[i][0], offsets[i][1], offsets[i][2]);
            var dispenserFixture = "BUD007C-DISPENSER-" + labels[i];
            set(dispenserFixture, dispenser, "dispenser[facing=east]");
            step(2);
            set(dispenserFixture, dispenserPower, "redstone_block");
            observeBlockTick(dispenserFixture, dispenser);
            set(dispenserFixture, dispenserTest, "glass");
            observeBlockTick(dispenserFixture, dispenser);
            observe(dispenserFixture, dispenser, "after_change");

            var dropper = new gu(150, Y, 100 + 5 * i);
            var dropperPower = offset(dropper, 0, 1, -1);
            var dropperTest = offset(dropper, offsets[i][0], offsets[i][1], offsets[i][2]);
            var dropperFixture = "BUD007C-DROPPER-" + labels[i];
            set(dropperFixture, dropper, "dropper[facing=east]");
            step(2);
            set(dropperFixture, dropperPower, "redstone_block");
            observeBlockTick(dropperFixture, dropper);
            set(dropperFixture, dropperTest, "glass");
            observeBlockTick(dropperFixture, dropper);
            observe(dropperFixture, dropper, "after_change");
        }
    }

    private void bud007Chain() {
        var fixture = "BUD007L-chain";
        var z = 119;
        level.a(10, 7, true);
        step(2);
        observeChunkActivity("BUD007L-setup", 10, 7, true);

        var source = new gu(170, Y, z);
        var probes = new gu[5];
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        for (int i = 0; i < probes.length; i++) {
            var x = 171 + i;
            var dust = new gu(x, Y, z);
            probes[i] = new gu(x, Y, z + 1);
            set(fixture, offset(dust, 0, -1, 0), "smooth_stone");
            placeWire(fixture, dust);
            set(fixture, probes[i], "piston[facing=south]");
        }
        step(4);
        for (var probe : probes) {
            set(fixture, offset(probe, 0, 1, 1), "redstone_block");
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, source, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        observe(fixture, source, "after_power");
        for (int i = 0; i < probes.length; i++) {
            observe(fixture, new gu(171 + i, Y, z), "after_power");
            observe(fixture, probes[i], "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private void bud007LongChain() {
        var fixture = "BUD007T-long";
        var z = 130;
        level.a(10, 8, true);
        level.a(11, 8, true);
        step(2);
        observeChunkActivity("BUD007T-long-setup", 10, 8, true);
        observeChunkActivity("BUD007T-long-setup", 11, 8, true);

        var source = new gu(170, Y, z);
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        var probes = new gu[9];
        for (int i = 0; i < probes.length; i++) {
            probes[i] = placeDustProbe(fixture, new gu(171 + i, Y, z), 0, 1, "south");
        }
        step(4);
        for (var probe : probes) {
            chargeDustProbe(fixture, probe, 0, 1);
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, source, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private void bud007MultiSource() {
        var fixture = "BUD007T-multi";
        var z = 140;
        level.a(10, 8, true);
        level.a(11, 8, true);
        step(2);
        observeChunkActivity("BUD007T-multi-setup", 10, 8, true);
        observeChunkActivity("BUD007T-multi-setup", 11, 8, true);

        var west = new gu(170, Y, z);
        var east = new gu(180, Y, z);
        set(fixture, offset(west, 0, -1, 0), "smooth_stone");
        set(fixture, offset(east, 0, -1, 0), "smooth_stone");
        var probes = new gu[9];
        for (int i = 0; i < probes.length; i++) {
            probes[i] = placeDustProbe(fixture, new gu(171 + i, Y, z), 0, 1, "south");
        }
        step(4);
        for (var probe : probes) {
            chargeDustProbe(fixture, probe, 0, 1);
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, west, "redstone_block");
        set(fixture, east, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private void bud007Corner() {
        var fixture = "BUD007T-corner";
        var z = 130;
        level.a(11, 8, true);
        step(2);
        observeChunkActivity("BUD007T-corner-setup", 11, 8, true);

        var source = new gu(180, Y, z);
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        var probes = new gu[6];
        for (int i = 0; i < 3; i++) {
            probes[i] = placeDustProbe(fixture, new gu(181 + i, Y, z), 0, 1, "south");
        }
        var corner = new gu(184, Y, z);
        set(fixture, offset(corner, 0, -1, 0), "smooth_stone");
        placeWire(fixture, corner);
        for (int i = 0; i < 3; i++) {
            probes[3 + i] = placeDustProbe(fixture, new gu(184, Y, z + 1 + i), 1, 0, "east");
        }
        step(4);
        for (int i = 0; i < 3; i++) {
            chargeDustProbe(fixture, probes[i], 0, 1);
        }
        for (int i = 0; i < 3; i++) {
            chargeDustProbe(fixture, probes[3 + i], 1, 0);
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, source, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private void bud007Junction() {
        var fixture = "BUD007T-junction";
        var z = 150;
        level.a(11, 9, true);
        step(2);
        observeChunkActivity("BUD007T-junction-setup", 11, 9, true);

        var source = new gu(180, Y, z);
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        var probes = new gu[10];
        for (int i = 0; i < 4; i++) {
            probes[i] = placeDustProbe(fixture, new gu(181 + i, Y, z), 0, 1, "south");
        }
        var junction = new gu(185, Y, z);
        set(fixture, offset(junction, 0, -1, 0), "smooth_stone");
        placeWire(fixture, junction);
        for (int i = 0; i < 3; i++) {
            probes[4 + i] = placeDustProbe(fixture, new gu(185, Y, z - 1 - i), 1, 0, "east");
        }
        for (int i = 0; i < 3; i++) {
            probes[7 + i] = placeDustProbe(fixture, new gu(185, Y, z + 1 + i), 1, 0, "east");
        }
        step(4);
        for (int i = 0; i < 4; i++) {
            chargeDustProbe(fixture, probes[i], 0, 1);
        }
        for (int i = 4; i < probes.length; i++) {
            chargeDustProbe(fixture, probes[i], 1, 0);
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, source, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private void bud007Section() {
        var fixture = "BUD007T-section";
        var z = 160;
        level.a(11, 10, true);
        level.a(12, 10, true);
        step(2);
        observeChunkActivity("BUD007T-section-setup", 11, 10, true);
        observeChunkActivity("BUD007T-section-setup", 12, 10, true);

        var source = new gu(190, 15, z);
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        var probes = new gu[6];
        for (int i = 0; i < 3; i++) {
            probes[i] = placeDustProbe(fixture, new gu(191 + i, 15, z), 0, 1, "south", false);
        }
        for (int i = 0; i < 3; i++) {
            probes[3 + i] = placeDustProbe(fixture, new gu(194 + i, 16, z), 0, 1, "south", false);
        }
        step(4);
        for (var probe : probes) {
            chargeDustProbe(fixture, probe, 0, 1);
        }
        step(2);
        observeBlockEventOrder(fixture, "before_power", probes);
        set(fixture, source, "redstone_block");
        observeBlockEventOrder(fixture, "after_power", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_power");
        }
        step(1);
        observeBlockEventOrder(fixture, "after_tick", probes);
        for (var probe : probes) {
            observe(fixture, probe, "after_tick");
        }
        step(3);
        for (var probe : probes) {
            observe(fixture, probe, "settled");
        }
    }

    private gu placeDustProbe(String fixture, gu dust, int probeX, int probeZ, String facing) {
        return placeDustProbe(fixture, dust, probeX, probeZ, facing, true);
    }

    private gu placeDustProbe(
            String fixture, gu dust, int probeX, int probeZ, String facing, boolean samplePlacement) {
        set(fixture, offset(dust, 0, -1, 0), "smooth_stone");
        placeWire(fixture, dust, samplePlacement);
        var probe = offset(dust, probeX, 0, probeZ);
        set(fixture, probe, "piston[facing=" + facing + "]");
        return probe;
    }

    private void chargeDustProbe(String fixture, gu probe, int x, int z) {
        set(fixture, offset(probe, x, 1, z), "redstone_block");
    }

    private void observeBlockEventOrder(String fixture, String phase, gu... probes) {
        List<String> events = new ArrayList<>();
        try {
            for (Object pending : (Iterable<?>) blockEvents.get(level)) {
                var event = (clr) pending;
                if (Arrays.asList(probes).contains(event.a())) {
                    events.add("{\"pos\":[" + event.a().u() + "," + event.a().v() + "," +
                            event.a().w() + "],\"block\":\"" + event.b() + "\",\"id\":" +
                            event.c() + ",\"data\":" + event.d() + "}");
                }
            }
        } catch (IllegalAccessException failure) {
            throw new IllegalStateException("cannot read pending block event order", failure);
        }
        List<String> probeRows = new ArrayList<>();
        for (var probe : probes) {
            probeRows.add("[" + probe.u() + "," + probe.v() + "," + probe.w() + "]");
        }
        emit("{\"record\":\"block_event_order\",\"fixture\":\"" + fixture + "\",\"gt\":" +
                level.V() + ",\"phase\":\"" + phase + "\",\"probes\":[" +
                String.join(",", probeRows) + "],\"pending\":[" + String.join(",", events) + "]}");
    }

    private static gu offset(gu position, int x, int y, int z) {
        return new gu(position.u() + x, position.v() + y, position.w() + z);
    }

    private void expectEventCount(String fixture, gu pos, String phase, int count) {
        var events = observeBlockEvents(fixture, pos, phase);
        if (events.size() != count) {
            throw new IllegalStateException(fixture + " GT " + level.V() + " expected " + count +
                    " pending block events, saw " + events);
        }
    }

    private List<String> observeBlockEvents(String fixture, gu pos, String phase) {
        List<String> events = new ArrayList<>();
        try {
            for (Object pending : (Iterable<?>) blockEvents.get(level)) {
                var event = (clr) pending;
                if (event.a().equals(pos)) {
                    events.add("{\"block\":\"" + event.b() + "\",\"id\":" + event.c() +
                            ",\"data\":" + event.d() + "}");
                }
            }
        } catch (IllegalAccessException failure) {
            throw new IllegalStateException("cannot read pending block events", failure);
        }
        emit("{\"record\":\"block_event_queue\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"pos\":[" + pos.u() + "," + pos.v() + "," + pos.w() + "],\"phase\":\"" + phase +
                "\",\"pending\":[" + String.join(",", events) + "]}");
        return events;
    }

    private void untilMovingCompletes(String fixture, gu base, gu head) {
        step(1);
        expect(fixture, base, "after_level_tick", "extended=true");
        expect(fixture, head, "after_level_tick", "minecraft:moving_piston");
        for (int count = 0; count < 4; count++) {
            step(1);
            observe(fixture, head, "after_level_tick");
        }
        expect(fixture, head, "settled", "minecraft:piston_head");
    }

    private void repeaters() {
        for (int delay = 1; delay <= 4; delay++) {
            var fixture = "TIM-REPEATER-" + delay;
            var base = new gu(9, Y, delay * 3);
            var support = new gu(9, Y - 1, delay * 3);
            var input = new gu(8, Y, delay * 3);
            set(fixture, support, "smooth_stone");
            set(fixture, base, "repeater[facing=west,delay=" + delay + "]");
            step(2);
            set(fixture, input, "redstone_block");
            observeBlockTick(fixture, base);
            expectAfterDelay(fixture, base, "powered", true, 2 * delay);
            // Reverse the input on the other parity of GT.
            step(1);
            set(fixture, input, "air");
            observeBlockTick(fixture, base);
            expectAfterDelay(fixture, base, "powered", false, 2 * delay);
        }
        repeaterShortPulse();
        repeaterPoweredPlacement();
        repeaterSideLock();
        repeaterFacingPriority();
        repeaterOutputNotifications();
        repeaterDueBatch();
    }

    private void repeaterShortPulse() {
        var fixture = "TIM-REPEATER-SHORT";
        var base = new gu(7, 38, 2);
        var input = offset(base, -1, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, base, "repeater[facing=west,delay=3]");
        set(fixture, input, "redstone_block");
        observeBlockTick(fixture, base);
        step(1);
        set(fixture, input, "air");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 12, base);
    }

    private void repeaterPoweredPlacement() {
        var fixture = "TIM-REPEATER-PREPOWER";
        var base = new gu(7, 38, 5);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(base, -1, 0, 0), "redstone_block");
        set(fixture, base, "repeater[facing=west,delay=4]");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base);

        fixture = "TIM-REPEATER-PLACED-BY";
        var placed = new gu(10, 38, 5);
        set(fixture, offset(placed, 0, -1, 0), "smooth_stone");
        set(fixture, offset(placed, -1, 0, 0), "redstone_block");
        set(fixture, placed, "repeater[facing=west]");
        var state = level.a_(placed);
        state.b().a(level, placed, state, null, cfz.b); // Block.setPlacedBy
        row("input", fixture, placed, "between_gt", "operation", "placed_by");
        observeBlockTick(fixture, placed);
        sampleScheduledBlocks(fixture, 3, placed);
    }

    private void repeaterSideLock() {
        var fixture = "TIM-REPEATER-LOCK";
        var base = new gu(7, 38, 9);
        var side = offset(base, 0, 0, -1);
        var sidePower = offset(side, 0, 0, -1);
        var input = offset(base, -1, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(side, 0, -1, 0), "smooth_stone");
        set(fixture, base, "repeater[facing=west,delay=2]");
        set(fixture, side, "redstone_block");
        observe(fixture, base, "after_side_power");
        set(fixture, side, "repeater[facing=north]");
        set(fixture, sidePower, "redstone_block");
        sampleScheduledBlocks(fixture, 2, side, base);

        set(fixture, input, "redstone_block");
        sampleScheduledBlocks(fixture, 5, base);
        set(fixture, sidePower, "air");
        sampleScheduledBlocks(fixture, 7, side, base);
        set(fixture, sidePower, "redstone_block");
        sampleScheduledBlocks(fixture, 2, side, base);
        set(fixture, input, "air");
        sampleScheduledBlocks(fixture, 5, base);
        set(fixture, sidePower, "air");
        sampleScheduledBlocks(fixture, 7, side, base);
    }

    private void repeaterFacingPriority() {
        for (var facing : List.of("north", "east")) {
            var fixture = "TIM-REPEATER-PRIORITY-" + facing.toUpperCase(java.util.Locale.ROOT);
            var base = new gu(7, facing.equals("north") ? 38 : 46, 14);
            var front = offset(base, 1, 0, 0);
            var input = offset(base, -1, 0, 0);
            set(fixture, offset(base, 0, -1, 0), "smooth_stone");
            set(fixture, offset(front, 0, -1, 0), "smooth_stone");
            set(fixture, base, "repeater[facing=west]");
            set(fixture, front, "repeater[facing=" + facing + "]");
            set(fixture, input, "redstone_block");
            observeBlockTick(fixture, base);
            sampleScheduledBlocks(fixture, 2, base);
            set(fixture, input, "air");
            observeBlockTick(fixture, base);
            sampleScheduledBlocks(fixture, 2, base);
        }
    }

    private void repeaterOutputNotifications() {
        var fixture = "TIM-REPEATER-NOTIFY";
        var base = new gu(7, 42, 6);
        var input = offset(base, -1, 0, 0);
        var beside = offset(base, 0, 0, 1);
        var beyondOutput = offset(base, 2, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, base, "repeater[facing=west]");
        set(fixture, beside, "piston[facing=south]");
        set(fixture, beyondOutput, "piston[facing=east]");
        set(fixture, offset(beside, 0, 1, 1), "redstone_block");
        set(fixture, offset(beyondOutput, 0, 1, 1), "redstone_block");
        set(fixture, input, "redstone_block");
        for (int tick = 0; tick < 4; tick++) {
            step(1);
            for (var position : List.of(base, beside, beyondOutput)) {
                observe(fixture, position, "after_level_tick");
            }
        }
    }

    private void repeaterDueBatch() {
        var fixture = "TIM-REPEATER-BATCH";
        var base = new gu(7, 42, 12);
        var rear = offset(base, -1, 0, 0);
        var input = offset(rear, -1, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(rear, 0, -1, 0), "smooth_stone");
        set(fixture, base, "repeater[facing=west]");
        set(fixture, rear, "repeater[facing=west]");

        // Queue the rear callback first. A temporary power block at its
        // position queues the front callback for the same deadline.
        set(fixture, input, "redstone_block");
        set(fixture, rear, "redstone_block");
        set(fixture, rear, "repeater[facing=west]");
        observeBlockTick(fixture, rear);
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 4, rear, base);
    }

    private void sampleScheduledBlocks(String fixture, int ticks, gu... positions) {
        for (int tick = 0; tick < ticks; tick++) {
            step(1);
            for (var position : positions) {
                observe(fixture, position, "after_level_tick");
                observeBlockTick(fixture, position);
            }
        }
    }

    private void observeBlockTick(String fixture, gu position) {
        try {
            var containers = (Map<?, ?>) blockTickContainers.get(level.l());
            var chunk = (efo<?>) containers.get(clt.a(position));
            List<String> entries = chunk == null ? List.of() : chunk.d()
                    .filter(tick -> tick.b().equals(position))
                    .map(tick -> "{\"block\":\"" + tick.a() + "\",\"due_gt\":" + tick.c() +
                            ",\"priority\":" + tick.d().a() + ",\"sub_tick_order\":" + tick.e() + "}")
                    .toList();
            emit("{\"record\":\"block_tick_queue\",\"fixture\":\"" + fixture + "\",\"gt\":" +
                    level.V() + ",\"pos\":[" + position.u() + "," + position.v() + "," + position.w() +
                    "],\"pending\":[" + String.join(",", entries) + "]}");
        } catch (IllegalAccessException failure) {
            throw new IllegalStateException("cannot read original block tick queue", failure);
        }
    }

    private void lamp() {
        var fixture = "TIM-LAMP";
        var base = new gu(4, 10, 2);
        var input = new gu(3, 10, 2);
        set(fixture, base, "redstone_lamp");
        set(fixture, input, "redstone_block");
        expect(fixture, base, "after_command", "lit=true");
        step(1);
        set(fixture, input, "air");
        expectAfterDelay(fixture, base, "lit", false, 4);

        var repower = "TIM-LAMP-REPOWER";
        set(repower, input, "redstone_block");
        expect(repower, base, "after_command", "lit=true");
        set(repower, input, "air");
        step(2);
        set(repower, input, "redstone_block");
        for (int elapsed = 0; elapsed < 4; elapsed++) {
            step(1);
            expect(repower, base, "after_level_tick", "lit=true");
        }
        set(repower, input, "air");
        expectAfterDelay(repower, base, "lit", false, 4);
        lampShapeNotifications();
    }

    private void lampShapeNotifications() {
        var fixture = "TIM-LAMP-SHAPE";
        var lamp = new gu(6, 154, 6);
        var piston = offset(lamp, 1, 0, 0);
        var observer = offset(lamp, 0, 1, 0);
        var direct = offset(lamp, -1, 0, 0);
        set(fixture, lamp, "redstone_lamp");
        set(fixture, piston, "piston[facing=east]");
        set(fixture, observer, "observer[facing=down]");
        step(6);
        set(fixture, offset(piston, 0, 1, -1), "redstone_block");
        set(fixture, direct, "redstone_block");
        for (var position : List.of(lamp, piston, observer)) {
            observe(fixture, position, "after_power");
        }
        sampleScheduledBlocks(fixture, 5, lamp, piston, observer);
        set(fixture, direct, "air");
        sampleScheduledBlocks(fixture, 9, lamp, piston, observer);
    }

    private void comparator() {
        var fixture = "TIM-COMPARATOR";
        var base = new gu(4, 10, 6);
        var input = new gu(3, 10, 6);
        set(fixture, new gu(4, 9, 6), "smooth_stone");
        set(fixture, base, "comparator[facing=west,mode=compare]");
        step(2);
        set(fixture, input, "redstone_block");
        observeBlockTick(fixture, base);
        expectAfterDelay(fixture, base, "powered", true, 2);
        comparatorPower(fixture, base, "east");
        set(fixture, input, "air");
        observeBlockTick(fixture, base);
        expectAfterDelay(fixture, base, "powered", false, 2);

        comparatorModes();
        comparatorThroughBlock();
        comparatorPriority();
        comparatorSidePower();
        comparatorRepeaterLock();
        comparatorOutputCircuit();
        comparatorPlacedByPlayer();
    }

    private void comparatorModes() {
        var fixture = "TIM-COMPARATOR-MODES";
        var base = new gu(7, 100, 6);
        var rear = offset(base, -1, 0, 0);
        var sideComparator = offset(base, 0, 0, -1);
        var sideInput = offset(sideComparator, 0, 0, -1);
        for (var pos : List.of(base, rear, sideComparator, sideInput)) {
            set(fixture, offset(pos, 0, -1, 0), "smooth_stone");
        }
        set(fixture, base, "comparator[facing=west]");
        set(fixture, sideComparator, "comparator[facing=north]");
        set(fixture, sideInput, "cake[bites=2]");
        set(fixture, rear, "cake[bites=1]");
        sampleScheduledBlocks(fixture, 5, base, sideComparator);
        comparatorPower(fixture, base, "east");
        clickComparator(fixture, base);
        comparatorPower(fixture, base, "east");
        sampleScheduledBlocks(fixture, 3, base);

        set(fixture, rear, "cake[bites=2]");
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        clickComparator(fixture, base);
        comparatorPower(fixture, base, "east");
        set(fixture, rear, "cake[bites=5]");
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
    }

    private void clickComparator(String fixture, gu position) {
        var player = new aig(server, level, new GameProfile(new UUID(0, 2), "ComparatorFixture"));
        var hit = new eee(new eei(position.u() + 0.5, position.v() + 0.125, position.w() + 0.5),
                ha.a("up"), position, false);
        var result = level.a_(position).a(level, player, bdw.a, hit); // BlockState.use
        row("input", fixture, position, "between_gt", "operation", "use_comparator");
        observe(fixture, position, "after_player_use_" + result);
        observeBlockTick(fixture, position);
    }

    private void comparatorThroughBlock() {
        var fixture = "TIM-COMPARATOR-THROUGH";
        var base = new gu(16, 104, 6);
        var conductor = offset(base, -1, 0, 0);
        var source = offset(conductor, -1, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(source, 0, -1, 0), "smooth_stone");
        set(fixture, conductor, "smooth_stone");
        set(fixture, base, "comparator[facing=west]");
        set(fixture, source, "cake[bites=1]");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        notifyComparatorOutput(fixture, source);
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        set(fixture, source, "cake[bites=4]");
        notifyComparatorOutput(fixture, source);
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");

        set(fixture, conductor, "glass");
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        set(fixture, conductor, "smooth_stone");
        // The lever strongly powers the conductor. Update the analog source
        // to explicitly notify the comparator through that conductor.
        set(fixture, offset(conductor, 0, 1, 0), "lever[face=floor,powered=true]");
        set(fixture, source, "cake[bites=3]");
        notifyComparatorOutput(fixture, source);
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        set(fixture, offset(conductor, 0, 1, 0), "air");
        set(fixture, source, "air");
        notifyComparatorOutput(fixture, source);
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
    }

    private void notifyComparatorOutput(String fixture, gu source) {
        level.c(source, level.a_(source).b()); // Level.updateNeighbourForOutputSignal
        row("input", fixture, source, "between_gt", "operation", "notify_comparator_output");
    }

    private void comparatorPriority() {
        var fixture = "TIM-COMPARATOR-PRIORITY";
        var base = new gu(7, 108, 6);
        var front = offset(base, 1, 0, 0);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(front, 0, -1, 0), "smooth_stone");
        set(fixture, base, "comparator[facing=west]");
        set(fixture, front, "repeater[facing=west]");
        set(fixture, offset(base, -1, 0, 0), "redstone_block");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 5, base, front);
        comparatorPower(fixture, base, "east");
    }

    private void comparatorSidePower() {
        var fixture = "TIM-COMPARATOR-SIDE";
        var base = new gu(7, 114, 6);
        var side = offset(base, 0, 0, -1);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(side, 0, -1, 0), "smooth_stone");
        set(fixture, base, "comparator[facing=west,mode=subtract]");
        set(fixture, offset(base, -1, 0, 0), "redstone_block");
        set(fixture, side, "lever[face=floor,powered=true]");
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");

        set(fixture, side, "redstone_block");
        sampleScheduledBlocks(fixture, 3, base);
        comparatorPower(fixture, base, "east");
        set(fixture, side, "redstone_wire");
        set(fixture, offset(side, 0, -1, -1), "smooth_stone");
        set(fixture, offset(side, 0, 0, -1), "redstone_wire");
        set(fixture, offset(side, 0, 0, -2), "redstone_block");
        sampleScheduledBlocks(fixture, 3, base, side);
        comparatorPower(fixture, base, "east");
    }

    private void comparatorRepeaterLock() {
        var fixture = "TIM-COMPARATOR-REPEATER-LOCK";
        var main = new gu(7, 120, 6);
        var side = offset(main, 0, 0, -1);
        var source = offset(side, 0, 0, -1);
        var rear = offset(main, -1, 0, 0);
        for (var pos : List.of(main, side, source)) {
            set(fixture, offset(pos, 0, -1, 0), "smooth_stone");
        }
        set(fixture, main, "repeater[facing=west]");
        set(fixture, side, "comparator[facing=north]");
        set(fixture, source, "cake[bites=6]");
        sampleScheduledBlocks(fixture, 3, main, side);
        set(fixture, rear, "redstone_block");
        sampleScheduledBlocks(fixture, 3, main, side);
        set(fixture, source, "air");
        sampleScheduledBlocks(fixture, 6, main, side);
    }

    private void comparatorPower(String fixture, gu pos, String output) {
        samplePower(fixture, pos, ha.a(output));
        samplePower(fixture, pos, ha.a(output).g());
    }

    private void comparatorOutputCircuit() {
        var fixture = "TIM-COMPARATOR-OUTPUT";
        var base = new gu(14, 125, 10);
        var source = offset(base, -1, 0, 0);
        var firstWire = offset(base, 1, 0, 0);
        var secondWire = offset(base, 2, 0, 0);
        var observer = offset(base, 0, 1, 0);
        for (var pos : List.of(base, source, firstWire, secondWire)) {
            set(fixture, offset(pos, 0, -1, 0), "smooth_stone");
        }
        set(fixture, base, "comparator[facing=west]");
        set(fixture, firstWire, "redstone_wire");
        set(fixture, secondWire, "redstone_wire");
        set(fixture, observer, "observer[facing=down]");
        step(6);

        set(fixture, source, "cake[bites=1]");
        sampleScheduledBlocks(fixture, 8, base, firstWire, secondWire, observer);
        comparatorPower(fixture, base, "east");
        set(fixture, source, "cake[bites=4]");
        sampleScheduledBlocks(fixture, 6, base, firstWire, secondWire, observer);
        comparatorPower(fixture, base, "east");

        set(fixture, source, "cake[bites=3]");
        observeBlockTick(fixture, base);
        set(fixture, base, "air");
        set(fixture, base, "comparator[facing=west]");
        observe(fixture, base, "after_replacement");
        observeBlockTick(fixture, base);
        comparatorPower(fixture, base, "east");
        sampleScheduledBlocks(fixture, 5, base, firstWire, secondWire, observer);
        comparatorPower(fixture, base, "east");
    }

    private void comparatorPlacedByPlayer() {
        var fixture = "TIM-COMPARATOR-PLACED-BY";
        var base = new gu(7, 128, 13);
        set(fixture, offset(base, 0, -1, 0), "smooth_stone");
        set(fixture, offset(base, -1, 0, 0), "redstone_block");
        set(fixture, base, "comparator[facing=west]");
        var state = level.a_(base);
        state.b().a(level, base, state, null, cfz.b); // Block.setPlacedBy
        row("input", fixture, base, "between_gt", "operation", "placed_comparator");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base);
    }

    private void observer() {
        var fixture = "TIM-OBSERVER";
        var base = new gu(4, 10, 10);
        var observed = new gu(3, 10, 10);
        var outputSide = new gu(5, 10, 10);
        set(fixture, base, "observer[facing=west]");
        observeBlockTick(fixture, base);
        step(6);
        set(fixture, outputSide, "smooth_stone");
        for (int i = 0; i < 3; i++) {
            step(1);
            expect(fixture, base, "after_level_tick", "powered=false");
            observeBlockTick(fixture, base);
        }
        set(fixture, observed, "glass");
        observeBlockTick(fixture, base);
        expectAfterDelay(fixture, base, "powered", true, 2);
        observeBlockTick(fixture, base);
        for (var face : ha.values()) {
            int signal = face == ha.a("east") ? 15 : 0;
            power(fixture, base, face, signal, signal, signal);
        }
        set(fixture, observed, "air");
        observeBlockTick(fixture, base);
        step(1);
        expect(fixture, base, "after_level_tick", "powered=true");
        for (int i = 0; i < 4; i++) {
            step(1);
            expect(fixture, base, "after_level_tick", "powered=false");
            observeBlockTick(fixture, base);
        }

        observerNotificationSources();
        observerChain();
        observerPistonCircuit();
        observerPoweredPlacement();
        observerDirectPlacement(base);
        observerPlacementDirections(base);
        observerConduction();
    }

    private void observerNotificationSources() {
        var fixture = "TIM-OBSERVER-NOTIFY";
        var base = new gu(10, 48, 6);
        var observed = offset(base, 0, 1, 0);
        var otherSide = offset(base, 0, -1, 0);
        set(fixture, base, "observer[facing=up]");
        set(fixture, otherSide, "redstone_lamp");
        step(4);
        notifyNeighbor(fixture, base, observed);
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base, otherSide);

        var up = ha.a("up");
        level.a(up, level.a_(observed), base, observed, 0, 512);
        row("input", fixture, base, "between_gt", "operation", "shape_update_from_up");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 6, base, otherSide);
    }

    private void observerChain() {
        var fixture = "TIM-OBSERVER-CHAIN";
        var upstream = new gu(15, 44, 10);
        var downstream = offset(upstream, 1, 0, 0);
        var changed = offset(upstream, -1, 0, 0);
        var lamp = offset(downstream, 1, 0, 0);
        set(fixture, upstream, "observer[facing=west]");
        set(fixture, downstream, "observer[facing=west]");
        set(fixture, lamp, "redstone_lamp");
        step(12);
        set(fixture, changed, "glass");
        observeBlockTick(fixture, upstream);
        observeBlockTick(fixture, downstream);
        step(1);
        set(fixture, changed, "smooth_stone");
        observeBlockTick(fixture, upstream);
        sampleScheduledBlocks(fixture, 11, upstream, downstream, lamp);
    }

    private void observerDirectPlacement(gu template) {
        var fixture = "TIM-OBSERVER-DIRECT-PLACEMENT";
        var base = new gu(11, 54, 14);
        var state = level.a_(template);
        if (!level.a(base, state, 11)) { // BlockItem.placeBlock
            throw new IllegalStateException("failed to place observer with block item flags");
        }
        state.b().a(level, base, state, null, cfz.b); // Block.setPlacedBy
        row("input", fixture, base, "between_gt", "operation", "place_observer_west");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 5, base);
    }

    private void observerPistonCircuit() {
        var fixture = "TIM-OBSERVER-CIRCUIT";
        var observer = new gu(12, 60, 6);
        var watched = offset(observer, -1, 0, 0);
        var dust = offset(observer, 1, 0, 0);
        var repeater = offset(observer, 2, 0, 0);
        var piston = offset(observer, 3, 0, 0);
        var head = offset(observer, 4, 0, 0);
        var load = offset(observer, 5, 0, 0);
        set(fixture, offset(dust, 0, -1, 0), "smooth_stone");
        set(fixture, offset(repeater, 0, -1, 0), "smooth_stone");
        set(fixture, observer, "observer[facing=west]");
        set(fixture, dust, "redstone_wire");
        set(fixture, repeater, "repeater[facing=west,delay=2]");
        set(fixture, piston, "piston[facing=east]");
        step(18);
        set(fixture, head, "smooth_stone");
        set(fixture, watched, "glass");
        observeBlockTick(fixture, observer);
        sampleScheduledBlocks(fixture, 14, observer, dust, repeater, piston, head, load);
    }

    private void observerPoweredPlacement() {
        var fixture = "TIM-OBSERVER-POWERED-PLACEMENT";
        var base = new gu(11, 44, 14);
        var target = offset(base, 0, 0, 1);
        set(fixture, base, "observer[facing=south,powered=true]");
        observe(fixture, base, "after_command");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base);

        set(fixture, target, "glass");
        sampleScheduledBlocks(fixture, 2, base);
        set(fixture, base, "air");
        set(fixture, base, "observer[facing=south]");
        observe(fixture, base, "after_replacement");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 5, base);
    }

    private void observerPlacementDirections(gu base) {
        var block = level.a_(base).b();
        for (var direction : ha.values()) {
            var hit = new eee(new eei(base.u() + 0.5, base.v() + 0.5, base.w() + 0.5),
                    direction, base, false);
            var context = new cih(level, null, bdw.a, cfz.b, hit) {
                @Override
                public ha d() {
                    return direction;
                }
            };
            var placement = block.a(context);
            row("observer_placement", "TIM-OBSERVER-DIRECTIONS", base, direction.toString(),
                    "state", placement.toString());
        }
    }

    private void observerConduction() {
        var fixture = "TIM-OBSERVER-CONDUCTOR";
        var base = new gu(11, 54, 7);
        set(fixture, base, "observer[facing=west]");
        step(6);
        set(fixture, offset(base, 0, 1, 0), "lever[face=floor,facing=east,powered=true]");
        for (var toward : ha.values()) {
            samplePower(fixture, base, toward);
        }
    }

    private void torch() {
        var fixture = "TIM-TORCH";
        var base = new gu(4, 10, 14);
        var support = offset(base, 0, -1, 0);
        var weakSource = offset(support, -1, 0, 0);
        var output = offset(base, 0, 1, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, base, "redstone_torch");
        set(fixture, output, "redstone_lamp");
        set(fixture, weakSource, "redstone_block");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base, output);
        torchPower(fixture, base);

        set(fixture, support, "redstone_block");
        observeBlockTick(fixture, base);
        step(1);
        set(fixture, support, "smooth_stone");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 3, base, output);
        set(fixture, support, "redstone_block");
        sampleScheduledBlocks(fixture, 3, base, output);
        torchPower(fixture, base);
        set(fixture, support, "smooth_stone");
        sampleScheduledBlocks(fixture, 5, base, output);

        wallTorches();
        torchOnPoweredSupport();
        torchWithStrongInput();
        torchAttachmentChoice(base);
        torchBurnout(59);
        torchBurnout(60);
        torchBurnout(61);
    }

    private void torchPower(String fixture, gu position) {
        for (var toward : ha.values()) {
            samplePower(fixture, position, toward);
        }
    }

    private void wallTorches() {
        int height = 66;
        for (var facing : List.of("north", "east", "south", "west")) {
            var fixture = "TIM-TORCH-WALL-" + facing;
            var base = new gu(16, height, 6);
            height += 3;
            var support = base.a(ha.a(facing).g()); // BlockPos.relative
            set(fixture, support, "smooth_stone");
            set(fixture, base, "redstone_wall_torch[facing=" + facing + "]");
            torchPower(fixture, base);
            set(fixture, support, "redstone_block");
            observeBlockTick(fixture, base);
            sampleScheduledBlocks(fixture, 2, base);
            torchPower(fixture, base);
            set(fixture, support, "smooth_stone");
            sampleScheduledBlocks(fixture, 2, base);
            set(fixture, support, "glass");
            observe(fixture, base, "after_glass_support");
            set(fixture, support, "air");
            observe(fixture, base, "after_support_removed");
            observeBlockTick(fixture, base);
        }
    }

    private void torchOnPoweredSupport() {
        var fixture = "TIM-TORCH-PLACEMENT";
        var base = new gu(4, 66, 11);
        var support = offset(base, 0, -1, 0);
        set(fixture, support, "redstone_block");
        set(fixture, base, "redstone_torch");
        observe(fixture, base, "after_command");
        observeBlockTick(fixture, base);
        sampleScheduledBlocks(fixture, 4, base);
        set(fixture, support, "smooth_stone");
        sampleScheduledBlocks(fixture, 4, base);
        set(fixture, support, "glass");
        observe(fixture, base, "after_glass_support");
        set(fixture, support, "air");
        observe(fixture, base, "after_support_removed");
    }

    private void torchWithStrongInput() {
        var fixture = "TIM-TORCH-STRONG";
        var torch = new gu(16, 85, 6);
        var support = offset(torch, -1, 0, 0);
        var repeater = offset(torch, -2, 0, 0);
        var power = offset(torch, -3, 0, 0);
        set(fixture, offset(repeater, 0, -1, 0), "smooth_stone");
        set(fixture, support, "smooth_stone");
        set(fixture, torch, "redstone_wall_torch[facing=east]");
        set(fixture, repeater, "repeater[facing=west]");
        set(fixture, power, "redstone_block");
        sampleScheduledBlocks(fixture, 6, repeater, torch);
        set(fixture, power, "air");
        sampleScheduledBlocks(fixture, 6, repeater, torch);
    }

    private void torchAttachmentChoice(gu sample) {
        var fixture = "TIM-TORCH-ATTACHMENT";
        var target = new gu(7, 78, 7);
        var wall = offset(target, -1, 0, 0);
        var floor = offset(target, 0, -1, 0);
        set(fixture, wall, "smooth_stone");
        set(fixture, floor, "smooth_stone");
        try {
            var player = new aig(server, level,
                    new GameProfile(new UUID(0, 1), "Fixture"));
            player.a(target.u() + 0.6, target.v() + 0.28, target.w() + 0.5, 90, 65); // Entity.moveTo
            var hit = new eee(new eei(target.u(), target.v() + 0.6, target.w() + 0.5),
                    ha.a("east"), wall, false);
            var item = level.a_(sample).b().k();
            var context = new cih(player, bdw.a, new cfz(item), hit);
            var placement = chi.class.getDeclaredMethod("c", cih.class); // StandingAndWallBlockItem.getPlacementState
            placement.setAccessible(true);
            var state = (dcb) placement.invoke(item, context);
            row("torch_attachment", fixture, target, "look_down_click_wall", "state", state.toString());
        } catch (ReflectiveOperationException failure) {
            throw new IllegalStateException("cannot query original torch attachment", failure);
        }
    }

    private void torchBurnout(int window) {
        var fixture = "TIM-TORCH-BURNOUT-" + window;
        var base = new gu(6 + window - 59, 72, 13);
        var support = offset(base, 0, -1, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, base, "redstone_torch");
        step(2);
        for (int pulse = 0; pulse < 8; pulse++) {
            set(fixture, support, "redstone_block");
            sampleScheduledBlocks(fixture, 2, base);
            var heldThroughRecovery = window == 59 && pulse == 7;
            if (!heldThroughRecovery) {
                set(fixture, support, "smooth_stone");
            }
            if (pulse < 7) {
                step(pulse == 6 ? window - 50 : 6);
                observe(fixture, base, "between_pulses");
                observeBlockTick(fixture, base);
            } else {
                sampleScheduledBlocks(fixture, 5, base);
                step(154);
                sampleScheduledBlocks(fixture, 4, base);
                if (heldThroughRecovery) {
                    set(fixture, support, "smooth_stone");
                    sampleScheduledBlocks(fixture, 3, base);
                }
            }
        }
    }

    private void chunkResidency() {
        // Retain FULL data with a level-33 region ticket while removing ticking tickets.
        var resident = aik.<Integer>a("rustcraft_reference_resident", Integer::compare);
        level.k().a(resident, new clt(16, 0), 0, 0);
        level.a(16, 0, true);
        step(3);
        var moving = new gu(260, 108, 10);
        var normal = new gu(260, 100, 2);
        var sticky = new gu(260, 104, 6);
        var unnoticed = new gu(266, 112, 6);
        var lamp = new gu(264, 116, 6);
        var observer = new gu(264, 120, 10);
        String pistons = "CHUNK-PISTON", scheduled = "CHUNK-TIM";
        for (var fixture : List.of(pistons, scheduled)) observeChunkActivity(fixture, true);
        set(pistons, moving, "piston[facing=east]");
        set(pistons, offset(moving, 1, 0, 0), "smooth_stone");
        set(pistons, offset(moving, 0, 1, -1), "redstone_block");
        notifyNeighbor(pistons, moving, offset(moving, 0, -1, 0));
        step(1);
        for (var base : List.of(normal, sticky, unnoticed)) {
            set(pistons, base, (base.equals(sticky) ? "sticky_piston" : "piston") + "[facing=east]");
            set(pistons, offset(base, 0, 1, -1), "redstone_block");
            if (!base.equals(unnoticed)) notifyNeighbor(pistons, base, offset(base, 0, -1, 0));
        }
        set(scheduled, lamp, "redstone_lamp");
        set(scheduled, offset(lamp, -1, 0, 0), "redstone_block");
        set(scheduled, offset(lamp, -1, 0, 0), "air");
        set(scheduled, observer, "observer[facing=west]");
        var bases = List.of(moving, normal, sticky, unnoticed);
        sampleResidentPistons(pistons, bases, "before_inactive");
        sampleScheduledBlocks(scheduled, 0, lamp, observer);
        level.a(16, 0, false);
        settleChunkTickets(false);
        for (var fixture : List.of(pistons, scheduled)) observeChunkActivity(fixture, false);
        for (int tick = 0; tick <= 6; tick++) {
            if (tick != 0) step(1);
            sampleResidentPistons(pistons, bases, "while_inactive");
            observe(scheduled, lamp, "while_inactive");
            observeBlockTick(scheduled, lamp);
            observe(scheduled, observer, "while_inactive");
            observeBlockTick(scheduled, observer);
        }
        level.a(16, 0, true);
        settleChunkTickets(true);
        for (var fixture : List.of(pistons, scheduled)) observeChunkActivity(fixture, true);
        for (int tick = 0; tick <= 5; tick++) {
            if (tick != 0) step(1);
            sampleResidentPistons(pistons, bases, "after_reactivate");
            observe(scheduled, lamp, "after_reactivate");
            observeBlockTick(scheduled, lamp);
            observe(scheduled, observer, "after_reactivate");
            observeBlockTick(scheduled, observer);
        }
        partialChunkBlockEvents();
    }

    private void partialChunkBlockEvents() {
        String fixture = "CHUNK-PARTIAL";
        level.a(20, 0, true);
        level.a(15, 0, true);
        settleChunkTickets(15, 0, true);
        settleChunkTickets(20, 0, true);
        observeChunkActivity(fixture, 15, 0, true);
        observeChunkActivity(fixture, 20, 0, true);

        var west = new gu(254, 100, 10);
        var east = new gu(323, 100, 10);
        for (var base : List.of(west, east)) {
            set(fixture, base, "piston[facing=east]");
            set(fixture, offset(base, 0, 1, -1), "redstone_block");
            set(fixture, offset(base, 0, -1, 0), "glass");
            expectEventCount(fixture, base, "queued", 1);
        }
        observe(fixture, west, "both_chunks_ticking");
        observe(fixture, east, "both_chunks_ticking");

        level.a(20, 0, false);
        settleChunkTickets(20, 0, false);
        observeChunkActivity(fixture, 15, 0, true);
        observeChunkActivity(fixture, 20, 0, false);
        for (int tick = 0; tick < 4; tick++) step(1);
        expect(fixture, west, "west_chunk_progress", "extended=true");
        expect(fixture, east, "east_chunk_held", "extended=false");
        expectEventCount(fixture, west, "west_chunk_queue", 0);
        expectEventCount(fixture, east, "east_chunk_queue", 1);

        level.a(20, 0, true);
        settleChunkTickets(20, 0, true);
        observeChunkActivity(fixture, 15, 0, true);
        observeChunkActivity(fixture, 20, 0, true);
        for (int tick = 0; tick < 4; tick++) step(1);
        expect(fixture, west, "both_chunks_resumed_west", "extended=true");
        expect(fixture, east, "both_chunks_resumed_east", "extended=true");
        expectEventCount(fixture, west, "settled_west_queue", 0);
        expectEventCount(fixture, east, "settled_east_queue", 0);

        partialChunkEjectors();
    }

    private void partialChunkEjectors() {
        String fixture = "CHUNK-PARTIAL-EJECTOR";
        var west = new gu(254, 140, 10);
        var east = new gu(323, 140, 10);
        for (var entry : List.of(
                Map.entry(west, "dispenser[facing=east]"),
                Map.entry(east, "dropper[facing=east]"))) {
            var position = entry.getKey();
            set(fixture, position, entry.getValue());
            loadEjector(fixture, position,
                    "{Slot:4b,id:\"minecraft:smooth_stone\",Count:2b}");
            set(fixture, offset(position, -1, 0, 0), "redstone_block");
            expect(fixture, position, "scheduled", "triggered=true");
            observeBlockTick(fixture, position);
            observeEjectorItems(fixture, position);
        }

        level.a(20, 0, false);
        settleChunkTickets(20, 0, false);
        observeChunkActivity(fixture, 15, 0, true);
        observeChunkActivity(fixture, 20, 0, false);
        expect(fixture, west, "active_chunk_suspended", "triggered=true");
        expect(fixture, east, "inactive_chunk_suspended", "triggered=true");
        observeBlockTick(fixture, west);
        observeBlockTick(fixture, east);
        observeEjectorItems(fixture, west);
        observeEjectorItems(fixture, east);

        level.a(20, 0, true);
        settleChunkTickets(20, 0, true);
        observeChunkActivity(fixture, 15, 0, true);
        observeChunkActivity(fixture, 20, 0, true);
        expect(fixture, west, "west_resumed_pending", "triggered=true");
        expect(fixture, east, "east_resumed_pending", "triggered=true");
        observeBlockTick(fixture, west);
        observeBlockTick(fixture, east);
        observeEjectorItems(fixture, west);
        observeEjectorItems(fixture, east);
    }

    private void settleChunkTickets(boolean ticking) {
        settleChunkTickets(16, 0, ticking);
    }

    private void settleChunkTickets(int x, int z, boolean ticking) {
        try {
            var update = aid.class.getDeclaredMethod("s"); // runDistanceManagerUpdates
            update.setAccessible(true);
            long deadline = System.nanoTime() + 10_000_000_000L;
            do {
                update.invoke(level.k());
                while (level.k().d()) { /* pollTask, without advancing the world clock */ }
                if (level.a(new clt(x, z).a()) == ticking && level.k().a(x, z) != null) return;
            } while (System.nanoTime() < deadline);
            var managerField = aid.class.getDeclaredField("c");
            managerField.setAccessible(true);
            var manager = managerField.get(level.k());
            var ticketsField = ahx.class.getDeclaredField("e");
            ticketsField.setAccessible(true);
            throw new IllegalStateException("chunk ticket transition did not settle: ticking=" +
                    level.a(new clt(x, z).a()) + " resident=" + (level.k().a(x, z) != null) +
                    " tickets=" + ticketsField.get(manager));
        } catch (ReflectiveOperationException failure) {
            throw new IllegalStateException("cannot settle original chunk tickets", failure);
        }
    }

    private void observeChunkActivity(String fixture, boolean ticking) {
        observeChunkActivity(fixture, 16, 0, ticking);
    }

    private void observeChunkActivity(String fixture, int x, int z, boolean ticking) {
        long key = new clt(x, z).a();
        if (level.a(key) != ticking || level.k().a(x, z) == null) {
            throw new IllegalStateException("expected a resident chunk with ticking=" + ticking);
        }
        emit("{\"record\":\"chunk_activity\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"chunk\":[" + x + "," + z + "],\"active\":" + ticking + ",\"resident\":true}");
    }

    private void sampleResidentPistons(String fixture, List<gu> bases, String phase) {
        for (var base : bases) {
            observe(fixture, base, phase);
            observe(fixture, offset(base, 1, 0, 0), phase);
            observe(fixture, offset(base, 2, 0, 0), phase);
            observeBlockEvents(fixture, base, phase);
        }
    }

    private static final gu TORCH_BURNED = new gu(3, 18, 12);
    private static final gu TORCH_HISTORY = new gu(10, 18, 12);
    private static final gu COMPARATOR_RESTART = new gu(5, 24, 6);

    private record PistonRestart(String block, String scenario, gu base) {
        String fixture() { return "RESTART-PISTON-" + scenario + "-" + block; }
        gu head() { return offset(base, 1, 0, 0); }
        gu load() { return offset(base, 2, 0, 0); }
        gu source() { return offset(base, 0, 1, -1); }
        gu notice() { return offset(base, 0, -1, 0); }
    }

    private static List<PistonRestart> pistonRestartCases() {
        var cases = new ArrayList<PistonRestart>();
        var scenarios = List.of("unnoticed-rise", "queued-extend", "moving", "moving-late",
                "moving-retract", "stale-extended", "queued-retract");
        for (var block : List.of("piston", "sticky_piston")) {
            for (int index = 0; index < scenarios.size(); index++) {
                int y = block.equals("piston") ? 15 : 31;
                cases.add(new PistonRestart(block, scenarios.get(index), new gu(15, y, -9 + 4 * index)));
            }
        }
        return cases;
    }

    private void prepareQcReopen() {
        level.a(0, 0, true);
        level.a(1, 0, true);
        level.a(0, -1, true);
        level.a(1, -1, true);
        step(3);
        var cases = pistonRestartCases();
        for (var setup : cases) {
            set(setup.fixture(), setup.base(), setup.block() + "[facing=east]");
            if (setup.scenario().startsWith("moving")) {
                set(setup.fixture(), setup.head(), "smooth_stone");
            }
            if (List.of("stale-extended", "queued-retract", "moving-retract").contains(setup.scenario())) {
                set(setup.fixture(), setup.source(), "redstone_block");
                notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
            }
        }
        step(4);
        for (var setup : cases) {
            switch (setup.scenario()) {
                case "stale-extended", "queued-retract" -> set(setup.fixture(), setup.source(), "air");
                case "moving-late" -> {
                    set(setup.fixture(), setup.source(), "redstone_block");
                    notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
                }
                default -> { }
            }
        }
        step(1);
        for (var setup : cases) {
            switch (setup.scenario()) {
                case "moving" -> {
                    set(setup.fixture(), setup.source(), "redstone_block");
                    notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
                }
                case "moving-retract" -> {
                    set(setup.fixture(), setup.source(), "air");
                    notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
                }
                default -> { }
            }
        }
        step(1);
        for (var setup : cases) {
            switch (setup.scenario()) {
                case "unnoticed-rise", "queued-extend" -> set(setup.fixture(), setup.source(), "redstone_block");
                default -> { }
            }
            if (setup.scenario().equals("queued-extend") || setup.scenario().equals("queued-retract")) {
                notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
                expectEventCount(setup.fixture(), setup.base(), "before_save", 1);
            }
            samplePistonRestart(setup, "before_save");
        }
        prepareEjectorRestart();
        if (!server.b(true, true, true)) {
            throw new IllegalStateException("original QC world did not save");
        }
    }

    private void observeQcReopen() {
        emit("{\"record\":\"world_reloaded\",\"fixture\":\"RESTART-PISTON\",\"gt\":" + level.V() + "}");
        emit("{\"record\":\"world_reloaded\",\"fixture\":\"RESTART-EJECTOR\",\"gt\":" + level.V() + "}");
        var cases = pistonRestartCases();
        var ejectors = ejectorRestartCases();
        for (var setup : cases) {
            samplePistonRestart(setup, "after_reopen");
        }
        for (var ejector : ejectors) {
            sampleEjectorRestart(ejector, "after_reopen");
        }
        for (int gt = 0; gt < 4; gt++) {
            step(1);
            for (var setup : cases) {
                samplePistonRestart(setup, "after_level_tick");
            }
            for (var ejector : ejectors) {
                sampleEjectorRestart(ejector, "after_level_tick");
            }
        }
        for (var setup : cases) {
            if (!setup.scenario().startsWith("moving")) {
                notifyNeighbor(setup.fixture(), setup.base(), setup.notice());
                samplePistonRestart(setup, "after_fresh_notice");
            }
        }
        for (var ejector : ejectors) {
            if (!ejector.pending()) {
                notifyNeighbor(ejector.fixture(), ejector.position(), ejector.notice());
                sampleEjectorRestart(ejector, "after_fresh_notice");
            }
        }
        for (int gt = 0; gt < 4; gt++) {
            step(1);
            for (var setup : cases) {
                samplePistonRestart(setup, "after_level_tick");
            }
            for (var ejector : ejectors) {
                if (!ejector.pending()) {
                    sampleEjectorRestart(ejector, "after_level_tick");
                }
            }
        }
    }

    private record EjectorRestart(String block, boolean pending, gu position) {
        String fixture() { return "RESTART-EJECTOR-" + block + (pending ? "-pending" : "-unnoticed"); }
        gu source() { return offset(position, 0, 1, -1); }
        gu notice() { return offset(position, 0, -1, 0); }
    }

    private static List<EjectorRestart> ejectorRestartCases() {
        return List.of(
                new EjectorRestart("dispenser", true, new gu(15, 53, 2)),
                new EjectorRestart("dispenser", false, new gu(15, 53, 12)),
                new EjectorRestart("dropper", true, new gu(15, 65, 2)),
                new EjectorRestart("dropper", false, new gu(15, 65, 12)));
    }

    private void prepareEjectorRestart() {
        for (var setup : ejectorRestartCases()) {
            var fixture = setup.fixture();
            set(fixture, setup.position(), setup.block() + "[facing=east]");
            loadEjector(fixture, setup.position(), "{Slot:3b,id:\"minecraft:smooth_stone\",Count:2b}");
            set(fixture, setup.source(), "redstone_block");
            if (setup.pending()) {
                notifyNeighbor(fixture, setup.position(), setup.notice());
                set(fixture, setup.source(), "air");
                notifyNeighbor(fixture, setup.position(), setup.notice());
            }
            sampleEjectorRestart(setup, "before_save");
        }
    }

    private void sampleEjectorRestart(EjectorRestart setup, String phase) {
        observe(setup.fixture(), setup.position(), phase);
        observeBlockTick(setup.fixture(), setup.position());
        observeEjectorItems(setup.fixture(), setup.position());
        observeEjectedItems(setup.fixture(), setup.position());
    }

    private void samplePistonRestart(PistonRestart setup, String phase) {
        observeBlockEvents(setup.fixture(), setup.base(), phase);
        observe(setup.fixture(), setup.base(), phase);
        observe(setup.fixture(), setup.head(), phase);
        if (setup.scenario().startsWith("moving")) {
            observe(setup.fixture(), setup.load(), phase);
            for (var position : List.of(setup.base(), setup.head(), setup.load())) {
                if (level.c_(position) instanceof dbx motion) {
                    long chunkX = Math.floorDiv(position.u(), 16) & 0xffffffffL;
                    long chunkZ = Math.floorDiv(position.w(), 16) & 0xffffffffL;
                    long chunk = chunkX | (chunkZ << 32);
                    emit("{\"record\":\"piston_progress\",\"fixture\":\"" + setup.fixture() +
                            "\",\"gt\":" + level.V() + ",\"pos\":[" + position.u() +
                            "," + position.v() + "," + position.w() + "],\"phase\":\"" + phase +
                            "\",\"base\":[" + setup.base().u() + "," + setup.base().v() + "," + setup.base().w() +
                            "],\"previous\":" + motion.a(0.0F) + ",\"current\":" + motion.a(1.0F) +
                            ",\"block_ticking\":" + level.a(chunk) + ",\"entities_loaded\":" + level.c(chunk) + "}");
                }
            }
        }
    }

    private void prepareTorchReopen() {
        var fixture = "RESTART-TORCH-BEFORE";
        level.a(0, 0, true);
        step(3);
        var comparatorFixture = "RESTART-COMPARATOR-BEFORE";
        var cake = offset(COMPARATOR_RESTART, -1, 0, 0);
        set(comparatorFixture, offset(COMPARATOR_RESTART, 0, -1, 0), "smooth_stone");
        set(comparatorFixture, offset(cake, 0, -1, 0), "smooth_stone");
        set(comparatorFixture, COMPARATOR_RESTART, "comparator[facing=west]");
        set(comparatorFixture, cake, "cake[bites=1]");
        var burnedSupport = offset(TORCH_BURNED, 0, -1, 0);
        var historySupport = offset(TORCH_HISTORY, 0, -1, 0);
        set(fixture, burnedSupport, "smooth_stone");
        set(fixture, historySupport, "smooth_stone");
        set(fixture, TORCH_BURNED, "redstone_torch");
        set(fixture, TORCH_HISTORY, "redstone_torch");
        for (int pulse = 0; pulse < 7; pulse++) {
            set(fixture, burnedSupport, "redstone_block");
            set(fixture, historySupport, "redstone_block");
            step(2);
            set(fixture, burnedSupport, "smooth_stone");
            set(fixture, historySupport, "smooth_stone");
            step(2);
        }
        set(fixture, burnedSupport, "redstone_block");
        step(2);
        set(fixture, burnedSupport, "smooth_stone");
        expect(fixture, TORCH_BURNED, "before_save", "lit=false");
        observeBlockTick(fixture, TORCH_BURNED);
        expect(fixture, TORCH_HISTORY, "before_save", "lit=true");
        observeBlockTick(fixture, TORCH_HISTORY);
        set(comparatorFixture, cake, "cake[bites=4]");
        observe(comparatorFixture, COMPARATOR_RESTART, "before_save");
        observeBlockTick(comparatorFixture, COMPARATOR_RESTART);
        comparatorPower(comparatorFixture, COMPARATOR_RESTART, "east");
        if (!server.b(true, true, true)) { // MinecraftServer.saveEverything
            throw new IllegalStateException("original torch world did not save");
        }
    }

    private void observeTorchReopen() {
        var fixture = "RESTART-TORCH-AFTER";
        emit("{\"record\":\"world_reloaded\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() + "}");
        var comparatorFixture = "RESTART-COMPARATOR-AFTER";
        emit("{\"record\":\"world_reloaded\",\"fixture\":\"" + comparatorFixture + "\",\"gt\":" + level.V() + "}");
        observe(comparatorFixture, COMPARATOR_RESTART, "after_reopen");
        observeBlockTick(comparatorFixture, COMPARATOR_RESTART);
        comparatorPower(comparatorFixture, COMPARATOR_RESTART, "east");
        expect(fixture, TORCH_BURNED, "after_reopen", "lit=false");
        observeBlockTick(fixture, TORCH_BURNED);
        expect(fixture, TORCH_HISTORY, "after_reopen", "lit=true");
        var historySupport = offset(TORCH_HISTORY, 0, -1, 0);
        set(fixture, historySupport, "redstone_block");
        step(1);
        observe(fixture, TORCH_HISTORY, "after_level_tick");
        observeBlockTick(fixture, TORCH_HISTORY);
        observe(comparatorFixture, COMPARATOR_RESTART, "after_level_tick");
        observeBlockTick(comparatorFixture, COMPARATOR_RESTART);
        comparatorPower(comparatorFixture, COMPARATOR_RESTART, "east");
        step(1);
        observe(fixture, TORCH_HISTORY, "after_level_tick");
        observeBlockTick(fixture, TORCH_HISTORY);
        observe(comparatorFixture, COMPARATOR_RESTART, "after_level_tick");
        observeBlockTick(comparatorFixture, COMPARATOR_RESTART);
        comparatorPower(comparatorFixture, COMPARATOR_RESTART, "east");
        set(fixture, historySupport, "smooth_stone");
        sampleScheduledBlocks(fixture, 2, TORCH_HISTORY);
        expect(fixture, TORCH_HISTORY, "after_next_pulse", "lit=true");
        step(153);
        sampleScheduledBlocks(fixture, 6, TORCH_BURNED);
        expect(fixture, TORCH_BURNED, "after_recovery", "lit=true");
    }

    private void stickyPistonPulse() {
        for (int duration : new int[] {0, 1, 2, 3, 5}) {
            var fixture = "TIM-STICKY-" + duration + "GT";
            var base = new gu(10, 30, duration * 2 + 2);
            var head = offset(base, 1, 0, 0);
            var pushed = offset(base, 2, 0, 0);
            var input = offset(base, -1, 0, 0);
            set(fixture, base, "sticky_piston[facing=east]");
            set(fixture, head, "smooth_stone");
            step(2);
            set(fixture, input, "redstone_block");
            expectEventCount(fixture, base, "after_power", 1);
            for (int tick = 0; tick < duration; tick++) {
                step(1);
                expect(fixture, base, "after_level_tick", "extended=true");
                observe(fixture, head, "after_level_tick");
                observe(fixture, pushed, "after_level_tick");
            }
            set(fixture, input, "air");
            expectEventCount(fixture, base, "after_power_removed", 1);
            for (int tick = 0; tick < 5; tick++) {
                step(1);
                observe(fixture, base, "after_level_tick");
                observe(fixture, head, "after_level_tick");
                observe(fixture, pushed, "after_level_tick");
            }
            expect(fixture, base, "settled", "extended=false");
            observe(fixture, head, "settled");
            observe(fixture, pushed, "settled");
        }
    }

    private void stickyQuasiPiston() {
        var fixture = "TIM-STICKY-QC";
        var base = new gu(15, 33, 7);
        var head = offset(base, 1, 0, 0);
        var pushed = offset(base, 2, 0, 0);
        var input = offset(base, 0, 1, -1);
        var notice = offset(base, 0, -1, 0);
        set(fixture, base, "sticky_piston[facing=east]");
        set(fixture, head, "smooth_stone");
        set(fixture, input, "redstone_block");
        expectEventCount(fixture, base, "after_quasi_power", 0);
        step(4);
        expect(fixture, base, "after_level_tick", "extended=false");
        set(fixture, notice, "glass");
        expectEventCount(fixture, base, "after_nc", 1);
        for (int tick = 0; tick < 4; tick++) {
            step(1);
            observe(fixture, base, "after_level_tick");
            observe(fixture, head, "after_level_tick");
            observe(fixture, pushed, "after_level_tick");
        }
        set(fixture, input, "air");
        expectEventCount(fixture, base, "after_quasi_power_removed", 0);
        step(4);
        expect(fixture, base, "after_level_tick", "extended=true");
        set(fixture, notice, "air");
        expectEventCount(fixture, base, "after_nc", 1);
        for (int tick = 0; tick < 4; tick++) {
            step(1);
            observe(fixture, base, "after_level_tick");
            observe(fixture, head, "after_level_tick");
            observe(fixture, pushed, "after_level_tick");
        }
        expect(fixture, head, "settled", "minecraft:smooth_stone");
        expect(fixture, pushed, "settled", "minecraft:air");
    }

    private void expectAfterDelay(String fixture, gu pos, String property, boolean value, int delay) {
        expect(fixture, pos, "after_command", property + "=" + !value);
        for (int elapsed = 1; elapsed <= delay; elapsed++) {
            step(1);
            expect(fixture, pos, "after_level_tick", property + "=" + (elapsed == delay ? value : !value));
        }
    }

    private void powerQueries() {
        var lever = new gu(4, 20, 2);
        var support = new gu(4, 19, 2);
        set("PWR-LEVER", support, "smooth_stone");
        set("PWR-LEVER", lever, "lever[face=floor,facing=east,powered=true]");
        for (var face : ha.values()) {
            power("PWR-LEVER", lever, face, 15, face.toString().equals("down") ? 15 : 0, 15);
            power("PWR-STRONG-CONDUCTOR", support, face, 0, 0, 15);
        }
        var strongReceiver = new gu(5, 19, 2);
        set("PWR-STRONG-CONDUCTOR", new gu(5, 18, 2), "smooth_stone");
        set("PWR-STRONG-CONDUCTOR", strongReceiver, "redstone_wire");
        expect("PWR-STRONG-CONDUCTOR", strongReceiver, "after_command", "power=15");

        var source = new gu(3, 20, 6);
        var stone = new gu(4, 20, 6);
        var glass = new gu(2, 20, 6);
        set("PWR-BLOCK", stone, "smooth_stone");
        set("PWR-BLOCK", glass, "glass");
        set("PWR-BLOCK", source, "redstone_block");
        for (var face : ha.values()) {
            power("PWR-BLOCK", source, face, 15, 0, 15);
            power("PWR-WEAK-CONDUCTOR", stone, face, 0, 0, 0);
            power("PWR-GLASS", glass, face, 0, 0, 0);
        }

        var wire1 = new gu(4, 20, 11);
        var wire2 = new gu(5, 20, 11);
        var sideSupport = new gu(6, 20, 11);
        var bottomSupport = new gu(5, 19, 11);
        var farWire = new gu(7, 20, 11);
        set("PWR-WIRE", new gu(4, 19, 11), "smooth_stone");
        set("PWR-WIRE", bottomSupport, "smooth_stone");
        set("PWR-WIRE", sideSupport, "smooth_stone");
        set("PWR-WIRE", wire1, "redstone_wire");
        set("PWR-WIRE", wire2, "redstone_wire");
        set("PWR-WIRE", new gu(7, 19, 11), "smooth_stone");
        set("PWR-WIRE", farWire, "redstone_wire");
        set("PWR-WIRE", new gu(3, 20, 11), "redstone_block");
        expect("PWR-WIRE", wire1, "after_command", "power=15");
        expect("PWR-WIRE", wire2, "after_command", "power=14");
        expect("PWR-WIRE", farWire, "after_command", "power=0");
        for (var face : ha.values()) {
            var name = face.toString();
            int alongWire = List.of("east", "west", "down").contains(name) ? 14 : 0;
            power("PWR-WIRE", wire2, face, alongWire, alongWire, alongWire);
            power("PWR-WIRE-SIDE-SUPPORT", sideSupport, face, 0, 0, 14);
            power("PWR-WIRE-BOTTOM-SUPPORT", bottomSupport, face, 0, 0, 14);
        }
    }

    private void wireExternalPower() {
        var wire = new gu(15, 236, 8);
        var above = offset(wire, 0, 1, 0);
        var below = offset(wire, 0, -1, 0);
        var neighbor = offset(wire, 1, 0, 0);
        var fixture = "TIM-WIRE-EXTERNAL-above";
        set(fixture, below, "smooth_stone");
        set(fixture, offset(neighbor, 0, -1, 0), "smooth_stone");
        set(fixture, wire, "redstone_wire");
        set(fixture, neighbor, "redstone_wire");
        set(fixture, above, "redstone_block");
        expect(fixture, wire, "after_source_placed", "power=15");
        expect(fixture, neighbor, "after_source_placed", "power=14");
        sampleScheduledBlocks(fixture, 2, wire, neighbor);
        set(fixture, above, "air");
        expect(fixture, wire, "after_source_removed", "power=0");
        expect(fixture, neighbor, "after_source_removed", "power=0");

        fixture = "TIM-WIRE-EXTERNAL-below";
        wire = new gu(9, 240, 8);
        below = offset(wire, 0, -1, 0);
        set(fixture, below, "redstone_block");
        set(fixture, wire, "redstone_wire");
        expect(fixture, wire, "after_wire_placed", "power=15");
        sampleScheduledBlocks(fixture, 2, wire);

        fixture = "TIM-WIRE-EXTERNAL-conductor";
        wire = new gu(9, 244, 8);
        var conductor = offset(wire, 1, 0, 0);
        var lever = offset(conductor, 0, 1, 0);
        set(fixture, offset(wire, 0, -1, 0), "smooth_stone");
        set(fixture, wire, "redstone_wire");
        set(fixture, conductor, "smooth_stone");
        set(fixture, lever, "lever[face=floor,facing=south]");
        useControl(fixture, lever, "use_lever");
        expect(fixture, wire, "after_conducted_power_enabled", "power=15");
        sampleScheduledBlocks(fixture, 2, wire);
        useControl(fixture, lever, "use_lever");
        expect(fixture, wire, "after_conducted_power_disabled", "power=0");

        fixture = "TIM-WIRE-EXTERNAL-no-wire-feedback";
        wire = new gu(9, 256, 8);
        conductor = offset(wire, 1, 0, 0);
        var isolated = offset(wire, 2, 0, 0);
        var power = offset(wire, -1, 0, 0);
        set(fixture, offset(wire, 0, -1, 0), "smooth_stone");
        set(fixture, offset(isolated, 0, -1, 0), "smooth_stone");
        set(fixture, conductor, "smooth_stone");
        set(fixture, wire, "redstone_wire");
        set(fixture, isolated, "redstone_wire");
        set(fixture, power, "redstone_block");
        expect(fixture, wire, "after_source_placed", "power=15");
        expect(fixture, isolated, "after_source_placed", "power=0");
        samplePower(fixture, conductor, ha.a("east"));
        set(fixture, power, "air");
        expect(fixture, wire, "after_source_removed", "power=0");
        expect(fixture, isolated, "after_source_removed", "power=0");
    }

    private void wireShapes() {
        var fixture = "SHAPE-WIRE-toggle";
        var wire = new gu(10, 260, 7);
        var support = offset(wire, 0, -1, 0);
        var above = offset(wire, 0, 1, 0);
        set(fixture, support, "smooth_stone");
        set(fixture, wire, "redstone_wire");
        set(fixture, above, "redstone_block");
        sampleWire(fixture, wire);
        useControl(fixture, wire, "use_wire");
        sampleWire(fixture, wire);
        useControl(fixture, wire, "use_wire");
        sampleWire(fixture, wire);
        set(fixture, above, "air");

        fixture = "SHAPE-WIRE-flat";
        wire = new gu(15, 269, 8);
        var east = offset(wire, 1, 0, 0);
        var south = offset(wire, 0, 0, 1);
        var north = offset(wire, 0, 0, -1);
        set(fixture, offset(wire, 0, -1, 0), "smooth_stone");
        placeWire(fixture, wire);
        sampleWire(fixture, wire);
        set(fixture, offset(east, 0, -1, 0), "smooth_stone");
        placeWire(fixture, east);
        sampleWire(fixture, wire, east);
        set(fixture, offset(south, 0, -1, 0), "smooth_stone");
        placeWire(fixture, south);
        sampleWire(fixture, wire, south);
        useControl(fixture, wire, "use_wire");
        sampleWire(fixture, wire);
        set(fixture, offset(north, 0, -1, 0), "smooth_stone");
        placeWire(fixture, north);
        sampleWire(fixture, wire);
        set(fixture, east, "air");
        sampleWire(fixture, wire);
        set(fixture, south, "air");
        set(fixture, north, "air");
        sampleWire(fixture, wire);

        for (var material : List.of("smooth_stone", "glass")) {
            fixture = "SHAPE-WIRE-step-" + material;
            wire = new gu(15, material.equals("glass") ? 287 : 277, 7);
            var raisedSupport = offset(wire, 1, 0, 0);
            var raisedWire = offset(wire, 1, 1, 0);
            var cover = offset(wire, 0, 1, 0);
            set(fixture, offset(wire, 0, -1, 0), "smooth_stone");
            set(fixture, raisedSupport, material);
            placeWire(fixture, wire);
            placeWire(fixture, raisedWire);
            set(fixture, offset(wire, -1, 0, 0), "redstone_block");
            sampleWire(fixture, wire, raisedWire);
            set(fixture, cover, "smooth_stone");
            sampleWire(fixture, wire, raisedWire);
            set(fixture, cover, "glass");
            sampleWire(fixture, wire, raisedWire);
            set(fixture, cover, "air");
            set(fixture, offset(wire, -1, 0, 0), "air");
            var raisedPower = offset(raisedWire, 1, 0, 0);
            set(fixture, raisedPower, "redstone_block");
            sampleWire(fixture, wire, raisedWire);
            set(fixture, raisedPower, "air");
            set(fixture, raisedWire, "air");
            sampleWire(fixture, wire);
        }
    }

    private void sampleWire(String fixture, gu... positions) {
        for (var position : positions) {
            observe(fixture, position, "after_input");
            for (var toward : ha.values()) {
                samplePower(fixture, position, toward);
            }
        }
    }

    private void wireNotices() {
        for (var wire : List.of(new gu(15, 296, 8), new gu(6, 304, 8), new gu(-1, 296, 8))) {
            var fixture = "TIM-WIRE-NOTIFY-" + wire.u();
            var source = offset(wire, -1, 0, 0);
            var below = offset(wire, 0, -1, 0);
            var east = offset(wire, 1, 0, 0);
            var downLamp = offset(below, 0, -1, 0);
            var eastLamp = offset(east, 1, 0, 0);
            var observer = offset(wire, 0, 1, 0);
            set(fixture, below, "smooth_stone");
            set(fixture, east, "smooth_stone");
            placeWire(fixture, wire);
            set(fixture, downLamp, "redstone_lamp");
            set(fixture, eastLamp, "redstone_lamp");
            set(fixture, observer, "observer[facing=down]");
            step(6);
            set(fixture, source, "redstone_block");
            expect(fixture, eastLamp, "after_powering_dust", "lit=true");
            expect(fixture, downLamp, "after_powering_dust", "lit=true");
            observeBlockTick(fixture, observer);
            step(5);
            set(fixture, source, "air");
            expect(fixture, eastLamp, "after_removing_dust_power", "lit=true");
            expect(fixture, downLamp, "after_removing_dust_power", "lit=true");
            observeBlockTick(fixture, observer);
            observeBlockTick(fixture, eastLamp);
            observeBlockTick(fixture, downLamp);
            sampleScheduledBlocks(fixture, 5, eastLamp, downLamp, observer);

            set(fixture, source, "redstone_block");
            step(5);
            set(fixture, wire, "air");
            observeBlockTick(fixture, observer);
            observeBlockTick(fixture, eastLamp);
            observeBlockTick(fixture, downLamp);
            sampleScheduledBlocks(fixture, 5, eastLamp, downLamp, observer);
        }
        for (var wire : List.of(new gu(8, 300, 3), new gu(24, 308, 4))) {
            var fixture = "TIM-WIRE-NOTIFY-five-" + wire.u();
            var source = wire.a(ha.a("up"));
            var supports = List.of("north", "south", "west", "east", "down");
            var lamps = new ArrayList<gu>();
            for (var direction : supports) {
                var face = ha.a(direction);
                var conductor = wire.a(face);
                var lamp = conductor.a(face);
                set(fixture, conductor, "smooth_stone");
                set(fixture, lamp, "redstone_lamp");
                lamps.add(lamp);
            }
            placeWire(fixture, wire);
            set(fixture, source, "redstone_block");
            for (var lamp : lamps) {
                expect(fixture, lamp, "after_powering_dust", "lit=true");
            }
            set(fixture, source, "air");
            for (var lamp : lamps) {
                observeBlockTick(fixture, lamp);
            }
            sampleScheduledBlocks(fixture, 5, lamps.toArray(gu[]::new));
        }
    }

    private void placeWire(String fixture, gu position) {
        placeWire(fixture, position, true);
    }

    private void placeWire(String fixture, gu position, boolean samplePlacement) {
        var player = new aig(server, level, new GameProfile(new UUID(0, 3), "WireFixture"));
        var support = offset(position, 0, -1, 0);
        var hit = new eee(new eei(position.u() + 0.5, position.v(), position.w() + 0.5),
                ha.a("up"), support, false);
        var wireBlock = level.a_(new gu(9, 240, 8)).b();
        var stack = new cfz(wireBlock.k());
        var state = wireBlock.a(new cih(player, bdw.a, stack, hit));
        if (state == null || !level.a(position, state, 11)) {
            throw new IllegalStateException("could not place wire at " + position);
        }
        wireBlock.a(level, position, state, player, stack);
        row("input", fixture, position, "between_gt", "operation", "place_wire");
        if (samplePlacement) {
            observe(fixture, position, "after_placement");
        }
    }

    private void power(String fixture, gu pos, ha toward, int normal, int direct, int emitted) {
        if (!Arrays.equals(samplePower(fixture, pos, toward), new int[] {normal, direct, emitted})) {
            throw new IllegalStateException(fixture + " toward " + toward + " differs from the expected power");
        }
    }

    private int[] samplePower(String fixture, gu pos, ha toward) {
        // SignalGetter's direction parameter points from the receiver to the source.
        var argument = toward.g();
        int own = level.a_(pos).b(level, pos, argument);
        int strong = level.a(pos, argument);
        int combined = level.c(pos, argument);
        var result = "{\"normal\":" + own + ",\"direct\":" + strong + ",\"emitted\":" + combined + "}";
        emit("{\"record\":\"power_query\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"pos\":[" + pos.u() + "," + pos.v() + "," + pos.w() + "],\"toward\":\"" + toward +
                "\",\"signal\":" + result + "}");
        return new int[] {own, strong, combined};
    }

    private void scheduledOrder() {
        var visible = new efq<String>(chunk -> true, level::ad);
        var left = new clt(0, 0);
        var right = new clt(1, 0);
        visible.a(left, new efo<>());
        visible.a(right, new efo<>());
        long gt = level.V();
        var a = new gu(4, Y, 1);
        var b = new gu(1, Y, 3);
        var c = new gu(16, Y, 4);
        var d = new gu(2, Y, 2);
        addTick(visible, "older_normal", a, gt + 1, 0, 0);
        addTick(visible, "urgent_later", b, gt + 2, -3, 1);
        addTick(visible, "other_high", c, gt + 1, -1, 2);
        addTick(visible, "other_normal", d, gt + 1, 0, 3);
        addTick(visible, "older_normal", a, gt + 1, -3, 4);
        var observed = new ArrayList<String>();
        visible.a(gt + 2, 10, (pos, kind) -> {
            observed.add(kind);
            row("sample", "ORDER-CHUNK", pos, "scheduler_drain", "executed", kind);
        });
        emit("{\"record\":\"order\",\"fixture\":\"ORDER-CHUNK\",\"drain_gt\":" + (gt + 2) +
                ",\"values\":" + array(observed) + "}");
        if (!observed.equals(List.of("other_high", "older_normal", "other_normal", "urgent_later"))) {
            throw new IllegalStateException("unexpected chunk tick order: " + observed);
        }
    }

    private void addTick(efq<String> scheduler, String kind, gu pos, long due, int priority, long order) {
        scheduler.a(new eft<>(kind, pos, due, efx.a(priority), order));
        emit("{\"record\":\"schedule_attempt\",\"fixture\":\"ORDER-CHUNK\",\"gt\":" + level.V() +
                ",\"pos\":[" + pos.u() + "," + pos.v() + "," + pos.w() +
                "],\"due_gt\":" + due + ",\"priority\":" + priority +
                ",\"sub_tick_order\":" + order + ",\"kind\":\"" + kind + "\"}");
    }

    private void neighborOrder() {
        // NeighborUpdater.UPDATE_ORDER, not the order of all nested notifications.
        var directions = Arrays.stream(dyg.a).map(Object::toString).toList();
        emit("{\"record\":\"order\",\"fixture\":\"ORD-NEIGHBOR-ARRAY\",\"values\":" + array(directions) + "}");
        if (!directions.equals(List.of("west", "east", "down", "up", "north", "south"))) {
            throw new IllegalStateException("unexpected neighbor order: " + directions);
        }
    }

    private static String array(List<String> values) {
        return "[\"" + String.join("\",\"", values) + "\"]";
    }

    private void step(int count) {
        for (int i = 0; i < count; i++) {
            // ServerLevel.tick: run Java block ticks, block events and block entities.
            level.a(() -> true);
        }
    }

    private void set(String fixture, gu pos, String block) {
        var command = "setblock " + pos.u() + " " + pos.v() + " " + pos.w() + " minecraft:" + block;
        if (server.aC().a(server.aD(), command) < 1) {
            throw new IllegalStateException("fixture command failed: " + command);
        }
        row("input", fixture, pos, "between_gt", "command", command);
    }

    private void expect(String fixture, gu pos, String phase, String token) {
        var state = observe(fixture, pos, phase);
        if (!state.contains(token)) {
            throw new IllegalStateException(fixture + " GT " + level.V() + " expected " + token + ", saw " + state);
        }
    }

    private String observe(String fixture, gu pos, String phase) {
        var state = level.a_(pos).toString();
        row("sample", fixture, pos, phase, "state", state);
        return state;
    }

    private void row(String record, String fixture, gu pos, String phase, String key, String value) {
        emit("{\"record\":\"" + record + "\",\"fixture\":\"" + fixture + "\",\"gt\":" + level.V() +
                ",\"pos\":[" + pos.u() + "," + pos.v() + "," + pos.w() + "],\"phase\":\"" + phase +
                "\",\"" + key + "\":\"" + value + "\"}");
    }

    private static void emit(String json) {
        System.out.println("RUSTCRAFT_REFERENCE " + json);
    }
}

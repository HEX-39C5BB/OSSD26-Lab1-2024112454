#![forbid(unsafe_code)]

pub mod benchmark;
mod tick_clock;

pub use tick_clock::{TickCommand, TickReply, TickStatus};

use std::cell::Cell;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rustcraft_authority::{
    Capabilities, GAME_TICK_INTERVAL, Mutation, Request, Server, SessionId, Snapshot, TickReport,
    ViewDistance,
};
use tick_clock::TickClock;

const COMMANDS_PER_TICK: usize = 64;
type Error = Box<dyn std::error::Error>;

pub enum WorldPath {
    OpenOrCreate(PathBuf),
    Existing(PathBuf),
}

impl WorldPath {
    pub fn as_path(&self) -> &Path {
        match self {
            Self::OpenOrCreate(path) | Self::Existing(path) => path,
        }
    }

    pub fn load(&self) -> io::Result<Option<(u64, Server)>> {
        match self {
            Self::OpenOrCreate(path) => rustcraft_persistence::load(path),
            Self::Existing(path) => rustcraft_persistence::load_existing(path).map(Some),
        }
    }
}

enum HostControl {
    Pause(bool),
    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    Step {
        requests: Vec<Request>,
        game_ticks: u8,
    },
    ViewDistance(ViewDistance),
    Tick(TickCommand, Sender<TickReply>),
    Stop,
}

pub struct LocalHost {
    session: SessionId,
    requests: SyncSender<Request>,
    control: Sender<HostControl>,
    pause_requested: Cell<bool>,
    paused: Arc<AtomicBool>,
    worker: JoinHandle<Server>,
    pub updates: Receiver<TickReport>,
    pub initial: Snapshot,
}

impl LocalHost {
    pub fn start(seed: u64, ticks: Option<u64>) -> Result<Self, Error> {
        let mut server = Server::bootstrap(seed)?;
        let session = server.open_local_session(Capabilities::P1_HOST);
        Self::run(server, session, ticks)
    }

    pub fn start_playable(seed: u64) -> Result<Self, Error> {
        Self::start_world(Server::playable(seed)?, None)
    }

    pub fn start_world(mut server: Server, ticks: Option<u64>) -> Result<Self, Error> {
        let session = server.open_local_operator()?;
        Self::run(server, session, ticks)
    }

    fn run(mut server: Server, session: SessionId, ticks: Option<u64>) -> Result<Self, Error> {
        let initial = server.snapshot();
        let seed = initial.seed;
        eprintln!(
            "event=world_initialized seed={seed} chunks={} mods={} tick_ms={}",
            initial.chunks.len(),
            server.locked_mods().len(),
            GAME_TICK_INTERVAL.as_millis(),
        );
        let (requests, incoming) = mpsc::sync_channel(COMMANDS_PER_TICK);
        let (publish, updates) = mpsc::sync_channel(8);
        let (control, commands) = mpsc::channel();
        let paused = Arc::new(AtomicBool::new(false));
        let worker_paused = Arc::clone(&paused);
        let worker = thread::Builder::new()
            .name("rustcraft-authority".into())
            .spawn(move || {
                let mut deadline = Instant::now() + GAME_TICK_INTERVAL;
                let mut player_deadline = deadline;
                let mut clock = TickClock::default();
                let mut remaining = ticks.unwrap_or(u64::MAX);
                let mut pause_requested = false;
                let mut paused = false;
                'worker: while remaining > 0 {
                    let command = if paused {
                        commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
                    } else {
                        commands.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    };
                    let mut requests = match command {
                        Ok(HostControl::Pause(pause)) => {
                            pause_requested = pause;
                            if paused && !pause {
                                paused = false;
                                worker_paused.store(false, Ordering::Release);
                                let now = Instant::now();
                                deadline = now + clock.interval();
                                player_deadline = now + GAME_TICK_INTERVAL;
                            }
                            continue;
                        }
                        Ok(HostControl::ViewDistance(distance)) => {
                            server.set_local_view_distance(distance);
                            continue;
                        }
                        Ok(HostControl::Tick(command, reply)) => {
                            let result = clock.command(command);
                            if result.is_ok() && command != TickCommand::Query {
                                deadline = Instant::now() + clock.interval();
                            }
                            let _ = reply.send(result);
                            continue;
                        }
                        #[cfg(any(debug_assertions, feature = "dev-tools"))]
                        Ok(HostControl::Step {
                            mut requests,
                            game_ticks,
                        }) if paused => {
                            for _ in 0..u64::from(game_ticks).min(remaining) {
                                let started = Instant::now();
                                let mut report = server.step(std::mem::take(&mut requests));
                                clock.record_tick(started.elapsed());
                                remaining -= 1;
                                server.clear_player_input();
                                report.snapshot = server.snapshot();
                                if publish.send(report).is_err() {
                                    break 'worker;
                                }
                            }
                            continue;
                        }
                        #[cfg(any(debug_assertions, feature = "dev-tools"))]
                        Ok(HostControl::Step { .. }) => continue,
                        Ok(HostControl::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => Vec::new(),
                    };
                    requests.extend(incoming.try_iter().take(COMMANDS_PER_TICK - requests.len()));
                    let start = Instant::now();
                    let slow_clock = clock.interval() >= GAME_TICK_INTERVAL;
                    let simulate_player = slow_clock || start >= player_deadline;
                    if simulate_player {
                        player_deadline = if slow_clock {
                            start + GAME_TICK_INTERVAL
                        } else {
                            (player_deadline + GAME_TICK_INTERVAL)
                                .max(start + Duration::from_millis(1))
                        };
                    }
                    let advance_world = clock.advance_world();
                    let mut report = server.step_local(requests, advance_world, simulate_player);
                    if advance_world {
                        remaining -= 1;
                        clock.record_tick(start.elapsed());
                    }
                    if pause_requested {
                        server.clear_player_input();
                        report.snapshot = server.snapshot();
                    }
                    if publish.send(report).is_err() {
                        break 'worker;
                    }
                    if pause_requested {
                        paused = true;
                        worker_paused.store(true, Ordering::Release);
                    }
                    deadline = (deadline + clock.interval()).max(Instant::now());
                }
                server.close_session(session);
                server
            })?;
        Ok(Self {
            session,
            requests,
            control,
            pause_requested: Cell::new(false),
            paused,
            worker,
            updates,
            initial,
        })
    }

    pub fn submit(&self, operation: u64, mutation: Mutation) -> io::Result<()> {
        if self.pause_requested.get() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "local world is paused",
            ));
        }
        let request = Request {
            session: self.session,
            operation,
            mutation,
        };
        self.requests
            .try_send(request)
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    io::Error::new(io::ErrorKind::WouldBlock, "local command queue is full")
                }
                mpsc::TrySendError::Disconnected(_) => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "local authority stopped")
                }
            })
    }

    pub fn set_paused(&self, paused: bool) -> io::Result<()> {
        if self.pause_requested.get() != paused {
            self.control.send(HostControl::Pause(paused)).map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "local authority stopped")
            })?;
            self.pause_requested.set(paused);
        }
        Ok(())
    }

    pub fn set_view_distance(&self, distance: ViewDistance) -> io::Result<()> {
        self.control
            .send(HostControl::ViewDistance(distance))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "local authority stopped"))
    }

    pub fn tick_command(&self, command: TickCommand) -> io::Result<Receiver<TickReply>> {
        let (reply, result) = mpsc::channel();
        self.control
            .send(HostControl::Tick(command, reply))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "local authority stopped"))?;
        Ok(result)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub fn step_paused(&self) -> io::Result<bool> {
        self.step_paused_with(Vec::new(), 1)
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    pub fn step_redstone_paused(
        &self,
        edits: Vec<(u64, rustcraft_authority::DebugInteraction)>,
    ) -> io::Result<bool> {
        self.step_paused_with(edits, rustcraft_authority::JAVA_REDSTONE_TICK_GAME_TICKS)
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    fn step_paused_with(
        &self,
        edits: Vec<(u64, rustcraft_authority::DebugInteraction)>,
        game_ticks: u8,
    ) -> io::Result<bool> {
        if !self.pause_requested.get() || !self.is_paused() {
            return Ok(false);
        }
        if edits.len() > COMMANDS_PER_TICK {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "too many planned GT interactions",
            ));
        }
        let requests = edits
            .into_iter()
            .map(|(operation, edit)| Request {
                session: self.session,
                operation,
                mutation: Mutation::DebugUseBlock(edit),
            })
            .collect();
        self.control
            .send(HostControl::Step {
                requests,
                game_ticks,
            })
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "local authority stopped"))?;
        Ok(true)
    }

    pub fn shutdown(self) -> Result<(), Error> {
        self.finish().map(|_| ())
    }

    pub fn finish(self) -> Result<Server, Error> {
        let _ = self.control.send(HostControl::Stop);
        drop(self.updates);
        self.worker
            .join()
            .map_err(|_| "authority thread panicked".into())
    }
}

pub fn log_tick(report: &TickReport) {
    let rejections = report.acks.iter().filter(|ack| ack.result.is_err()).count();
    eprintln!(
        "event=tick_committed tick={} chunks={} entities={} operations={} rejected={} due_tasks={}",
        report.snapshot.tick,
        report.snapshot.chunks.len(),
        report.snapshot.entities.len(),
        report.acks.len(),
        rejections,
        report.due_tasks.len(),
    );
    for (sequence, trace) in report.piston_events.iter().enumerate() {
        let base = trace.event.base;
        eprintln!(
            "event=piston_block_event gt={} sequence={} stage={:?} action={:?} base={},{},{} facing={} notice={:?} moving={:?} power={:?}",
            trace.gt,
            sequence,
            trace.stage,
            trace.event.action,
            base.x,
            base.y,
            base.z,
            trace.event.facing,
            trace.source,
            trace.moving_position,
            trace.power,
        );
    }
    for event in &report.task_events {
        use rustcraft_authority::TaskEvent;
        let tick = report.snapshot.tick;
        match event {
            TaskEvent::Queued { task, operation } => eprintln!(
                "event=task_queued tick={tick} id={} due_tick={} kind={}",
                task.id,
                task.due_tick,
                operation.name()
            ),
            TaskEvent::Canceled { task, operation } => eprintln!(
                "event=task_canceled tick={tick} id={} due_tick={} kind={}",
                task.id,
                task.due_tick,
                operation.name()
            ),
            TaskEvent::Due {
                task,
                operation,
                result,
            } => match result {
                Ok(effect) => eprintln!(
                    "event=task_due tick={tick} id={} due_tick={} kind={} effect={effect:?}",
                    task.id,
                    task.due_tick,
                    operation.name()
                ),
                Err(reason) => eprintln!(
                    "event=task_rejected tick={tick} id={} due_tick={} kind={} reason={reason:?}",
                    task.id,
                    task.due_tick,
                    operation.name()
                ),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustcraft_authority::{Effect, PlayerInput, ScheduledOperation};
    use rustcraft_world::BlockPos;

    #[test]
    fn delivers_local_command_ack_and_authoritative_snapshot() {
        let host = LocalHost::start(7, None).unwrap();
        let position = BlockPos::new(0, 0, 0);
        host.submit(1, Mutation::SpawnEntity { position }).unwrap();
        let start = Instant::now();
        loop {
            let report = host
                .updates
                .recv_timeout(Duration::from_secs(3).saturating_sub(start.elapsed()))
                .unwrap();
            if let Some(ack) = report.acks.first() {
                let Ok(Effect::Spawned(id)) = ack.result else {
                    panic!("spawn rejected: {ack:?}");
                };
                assert_eq!(ack.operation, 1);
                assert_eq!(report.snapshot.entities[0].id, id);
                assert_eq!(report.snapshot.entities[0].position, position);
                assert_eq!(report.snapshot.seed, 7);
                break;
            }
        }
        host.shutdown().unwrap();
    }

    #[test]
    fn tick_freeze_allows_player_frames_without_consuming_delayed_world_tasks() {
        let mut world = Server::playable(42).unwrap();
        let task = world
            .schedule_operation(8, ScheduledOperation::Event)
            .unwrap();
        let host = LocalHost::start_world(world, None).unwrap();
        let clock = |command| {
            host.tick_command(command)
                .unwrap()
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
        };
        let next = || host.updates.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(clock(TickCommand::Freeze).unwrap().frozen);
        assert_eq!(clock(TickCommand::Rate(40.0)).unwrap().rate, 40.0);
        assert!(clock(TickCommand::Rate(f64::NAN)).is_err());
        let _ = host.updates.try_iter().count();
        let frozen = next().snapshot;
        host.submit(
            1,
            Mutation::ControlPlayer(PlayerInput {
                forward: 1,
                ..PlayerInput::default()
            }),
        )
        .unwrap();
        let mut report = next();
        while report.acks.is_empty() {
            report = next();
        }
        assert_eq!(report.snapshot.tick, frozen.tick);
        assert!(report.snapshot.player.unwrap().position[2] > frozen.player.unwrap().position[2]);
        assert!(report.due_tasks.is_empty());

        let steps = (task.due_tick - frozen.tick) as u32;
        assert_eq!(clock(TickCommand::Step(steps)).unwrap().stepping, steps);
        loop {
            let report = next();
            if report.snapshot.tick == task.due_tick {
                assert_eq!(report.due_tasks, [task]);
                break;
            }
        }
        assert_eq!(next().snapshot.tick, task.due_tick);
        assert!(clock(TickCommand::StopStep).is_err());
        assert_eq!(clock(TickCommand::Sprint(3)).unwrap().sprinting, 3);
        while next().snapshot.tick < task.due_tick + 3 {}
        assert_eq!(next().snapshot.tick, task.due_tick + 3);
        assert!(!clock(TickCommand::Unfreeze).unwrap().frozen);
        let mut report = next();
        while report.snapshot.tick == task.due_tick + 3 {
            report = next();
        }
        assert_eq!(report.snapshot.tick, task.due_tick + 4);
        host.shutdown().unwrap();
    }

    #[cfg(any(debug_assertions, feature = "dev-tools"))]
    #[test]
    fn applies_debug_clicks_once_and_publishes_both_game_ticks_of_one_redstone_tick() {
        use rustcraft_authority::{BlockAction, DebugView, RulePreset};

        let preset = RulePreset::JavaFlatV1;
        let mut world = Server::playable_with_rules(42, preset.default_range(), preset).unwrap();
        let above = [BlockPos::new(3, -60, 4), BlockPos::new(4, -60, 4)];
        let lever = world.registry().block_id("rustcraft_java:lever").unwrap();
        let on = world
            .registry()
            .block_id("rustcraft_java:lever_on")
            .unwrap();
        let setup = world.open_local_session(Capabilities::P1_HOST);
        for (index, position) in above.into_iter().enumerate() {
            world.step(vec![Request {
                session: setup,
                operation: index as u64 + 1,
                mutation: Mutation::SetBlock {
                    position,
                    state: lever,
                    expected_revision: world.snapshot().chunks[&position.chunk()].revision(),
                },
            }]);
        }
        let host = LocalHost::start_world(world, None).unwrap();
        let planned = above
            .into_iter()
            .enumerate()
            .map(|(index, pos)| {
                let view = DebugView {
                    eye: [pos.x as f64 + 0.5, -57.0, pos.z as f64 + 0.5],
                    yaw: 0.0,
                    pitch: -std::f64::consts::FRAC_PI_2,
                };
                let edit = host
                    .initial
                    .plan_debug_interaction(view, BlockAction::Place)
                    .unwrap();
                assert_eq!(edit.position(), pos);
                (index as u64 + 1, edit)
            })
            .collect::<Vec<_>>();
        host.set_paused(true).unwrap();
        let mut frozen = host
            .updates
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .snapshot;
        while !host.is_paused() {
            if let Ok(report) = host.updates.recv_timeout(Duration::from_millis(100)) {
                frozen = report.snapshot;
            }
        }
        if let Some(report) = host.updates.try_iter().last() {
            frozen = report.snapshot;
        }
        if frozen.tick.is_multiple_of(2) {
            assert!(host.step_paused().unwrap());
            frozen = host
                .updates
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .snapshot;
        }
        for pos in above {
            assert_eq!(frozen.chunks[&pos.chunk()].block(pos), Some(lever));
        }
        assert!(host.step_redstone_paused(planned).unwrap());
        let stepped = host.updates.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(stepped.snapshot.tick, frozen.tick + 1);
        assert_eq!(stepped.debug_interactions.len(), 2);
        assert!(
            stepped
                .debug_interactions
                .iter()
                .all(|input| input.result.is_ok())
        );
        for pos in above {
            assert_eq!(stepped.snapshot.chunks[&pos.chunk()].block(pos), Some(on));
        }
        assert_eq!(
            stepped.snapshot.player.unwrap().position,
            frozen.player.unwrap().position
        );
        let second = host.updates.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(second.snapshot.tick, frozen.tick + 2);
        assert!(second.acks.is_empty());
        assert!(second.debug_interactions.is_empty());
        for pos in above {
            assert_eq!(second.snapshot.chunks[&pos.chunk()].block(pos), Some(on));
        }
        host.shutdown().unwrap();
    }

    #[test]
    fn freezes_ticks_at_the_pause_boundary_and_clears_held_movement() {
        let host = LocalHost::start_playable(42).unwrap();
        host.submit(
            1,
            Mutation::ControlPlayer(PlayerInput {
                forward: 1,
                ..PlayerInput::default()
            }),
        )
        .unwrap();
        loop {
            let report = host.updates.recv_timeout(Duration::from_secs(2)).unwrap();
            if report.acks.iter().any(|ack| ack.operation == 1) {
                assert!(report.snapshot.player.unwrap().position[2] > 0.5);
                break;
            }
        }

        host.set_paused(true).unwrap();
        assert_eq!(
            host.submit(2, Mutation::ControlPlayer(PlayerInput::default()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while !host.is_paused() {
            assert!(
                Instant::now() < deadline,
                "pause did not reach a tick boundary"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let frozen = host.updates.try_iter().last().unwrap().snapshot;
        assert!(matches!(
            host.updates.recv_timeout(Duration::from_millis(160)),
            Err(RecvTimeoutError::Timeout),
        ));

        #[cfg(any(debug_assertions, feature = "dev-tools"))]
        let frozen = {
            assert!(host.step_paused().unwrap());
            let stepped = host
                .updates
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .snapshot;
            assert_eq!(stepped.tick, frozen.tick + 1);
            assert_eq!(
                stepped.player.unwrap().position[2],
                frozen.player.unwrap().position[2]
            );
            assert!(host.is_paused());
            assert!(matches!(
                host.updates.recv_timeout(Duration::from_millis(160)),
                Err(RecvTimeoutError::Timeout),
            ));
            stepped
        };

        host.set_paused(false).unwrap();
        let resumed = host
            .updates
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .snapshot;
        assert_eq!(resumed.tick, frozen.tick + 1);
        assert_eq!(
            resumed.player.unwrap().position[2],
            frozen.player.unwrap().position[2]
        );
        host.shutdown().unwrap();
    }
}

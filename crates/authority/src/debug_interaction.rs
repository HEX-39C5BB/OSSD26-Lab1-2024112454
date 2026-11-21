use std::f64::consts::FRAC_PI_2;

use rustcraft_world::{BlockPos, BlockStateId};

use crate::interaction::{BlockInteractionView, apply_block_action};
use crate::player::trace_target;
use crate::{BlockAction, BlockTarget, Effect, GameMode, Rejection, Server, Snapshot, WorldSound};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugView {
    pub eye: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
}

impl DebugView {
    fn target(self, voxel: impl FnMut(BlockPos) -> Option<(bool, u64)>) -> Option<BlockTarget> {
        if !self.yaw.is_finite()
            || !(-FRAC_PI_2..=FRAC_PI_2).contains(&self.pitch)
            || self
                .eye
                .iter()
                .any(|axis| !(-30_000_000.0..30_000_000.0).contains(axis))
        {
            return None;
        }
        trace_target(self.eye, self.yaw, self.pitch, voxel)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DebugInteraction {
    view: DebugView,
    pub action: BlockAction,
    position: BlockPos,
    expected: BlockStateId,
    placement: Option<BlockPos>,
}

impl DebugInteraction {
    pub fn position(self) -> BlockPos {
        self.position
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugInteractionEvent {
    pub position: BlockPos,
    pub action: BlockAction,
    pub result: Result<Effect, Rejection>,
}

impl Snapshot {
    pub fn plan_debug_interaction(
        &self,
        view: DebugView,
        action: BlockAction,
    ) -> Option<DebugInteraction> {
        let target = view.target(|position| {
            if self
                .height
                .is_some_and(|height| !height.contains(position.y))
            {
                return None;
            }
            let chunk = self.chunks.get(&position.chunk())?;
            let state = chunk.block(position)?;
            Some((state != chunk.empty_block(), chunk.revision()))
        })?;
        let expected = self
            .chunks
            .get(&target.position.chunk())?
            .block(target.position)?;
        Some(DebugInteraction {
            view,
            action,
            position: target.position,
            expected,
            placement: target.placement.map(|placement| placement.position),
        })
    }
}

impl Server {
    pub(crate) fn run_debug_interaction(
        &mut self,
        command: DebugInteraction,
        sounds: &mut Vec<WorldSound>,
    ) -> Result<Effect, Rejection> {
        if command.action == BlockAction::Break && self.game_mode() != GameMode::Creative {
            return Err(Rejection::MissingCapability);
        }
        let target = command
            .view
            .target(|position| {
                self.dimension
                    .voxel(position)
                    .map(|(state, revision)| (!self.dimension.is_empty(state), revision))
            })
            .ok_or(Rejection::NoBlockTarget)?;
        if target.position != command.position
            || self
                .dimension
                .voxel(target.position)
                .map(|(state, _)| state)
                != Some(command.expected)
        {
            return Err(Rejection::StaleBlockTarget);
        }
        let java_rules = self.rules().is_java_flat();
        let use_only = java_rules && self.redstone_use_available(command.expected);
        if command.action == BlockAction::Place
            && !use_only
            && target.placement.map(|placement| placement.position) != command.placement
        {
            return Err(Rejection::StaleBlockTarget);
        }
        apply_block_action(
            self,
            command.action,
            target.placement,
            BlockInteractionView {
                target,
                yaw: command.view.yaw.rem_euclid(std::f64::consts::TAU),
                pitch: command.view.pitch,
                sneaking: false,
            },
            sounds,
            java_rules,
        )
    }
}

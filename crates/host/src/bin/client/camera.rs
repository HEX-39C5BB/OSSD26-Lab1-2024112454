use std::f64::consts::{FRAC_PI_2, TAU};

use rustcraft_authority::PlayerSnapshot;
use rustcraft_vulkan::Controls;
use rustcraft_world::BlockPos;

use crate::settings::ClientSettings;

#[derive(Clone, Copy)]
pub(super) struct Camera {
    pub eye: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub speed: f64,
}

impl Camera {
    pub fn at_player(player: PlayerSnapshot) -> Self {
        Self {
            eye: player.eye_position(),
            yaw: player.yaw,
            pitch: player.pitch,
            speed: 0.0,
        }
    }

    pub fn update(&mut self, controls: &Controls, settings: ClientSettings) {
        if !controls.captured {
            self.speed = 0.0;
            return;
        }
        let [yaw, pitch] = settings.look_delta(controls.mouse);
        self.yaw = (self.yaw + yaw).rem_euclid(TAU);
        self.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        let seconds = controls
            .frame_timing
            .last_interval
            .map_or(0.016, |frame| frame.as_secs_f64())
            .min(0.1);
        let (sin, cos) = self.yaw.sin_cos();
        let forward = f64::from(controls.forward);
        let strafe = f64::from(controls.strafe);
        let rise = f64::from(i8::from(controls.jump) - i8::from(controls.descend));
        let length = forward.hypot(strafe).hypot(rise);
        self.speed = if length == 0.0 { 0.0 } else { 12.0 };
        let distance = self.speed * seconds / length.max(1.0);
        self.eye[0] += (sin * forward + cos * strafe) * distance;
        self.eye[1] += rise * distance;
        self.eye[2] += (cos * forward - sin * strafe) * distance;
    }

    pub fn block(self) -> BlockPos {
        let [x, y, z] = self.eye.map(|axis| axis.floor() as i64);
        BlockPos::new(x, y, z)
    }

    pub fn direction(self) -> &'static str {
        let angle = self.yaw.rem_euclid(TAU);
        ["South +Z", "East +X", "North -Z", "West -X"][(angle / FRAC_PI_2).round() as usize % 4]
    }
}

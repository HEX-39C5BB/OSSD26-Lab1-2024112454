use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use rustcraft_authority::ViewDistance;
use serde::{Deserialize, Serialize};

pub(super) const BASE_SENSITIVITY: f64 = 0.0021;
const SCHEMA: u8 = 1;
const MIN_PERCENT: u16 = 25;
const MAX_PERCENT: u16 = 300;
const STEP: i16 = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AudioSettings {
    pub volume_percent: u8,
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            volume_percent: 70,
            muted: false,
        }
    }
}

impl AudioSettings {
    pub fn gain(self) -> f32 {
        if self.muted {
            0.0
        } else {
            f32::from(self.volume_percent) / 100.0
        }
    }

    pub fn adjust_volume(&mut self, increase: bool) {
        let delta = if increase { 10 } else { -10 };
        self.volume_percent = self.volume_percent.saturating_add_signed(delta).min(100);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ClientSettings {
    schema: u8,
    pub sensitivity_percent: u16,
    pub invert_y: bool,
    #[serde(default)]
    pub audio: AudioSettings,
    #[serde(default, with = "view_distance")]
    pub view_distance: ViewDistance,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            sensitivity_percent: 100,
            invert_y: false,
            audio: AudioSettings::default(),
            view_distance: ViewDistance::default(),
        }
    }
}

impl ClientSettings {
    pub fn look_delta(&self, [x, y]: [f64; 2]) -> [f64; 2] {
        let scale = BASE_SENSITIVITY * f64::from(self.sensitivity_percent) / 100.0;
        [
            x * scale,
            if self.invert_y { y * scale } else { -y * scale },
        ]
    }

    pub fn adjust_sensitivity(&mut self, increase: bool) {
        let delta = if increase { STEP } else { -STEP };
        self.sensitivity_percent = self
            .sensitivity_percent
            .saturating_add_signed(delta)
            .clamp(MIN_PERCENT, MAX_PERCENT);
    }

    pub fn adjust_view_distance(&mut self, increase: bool) {
        let radius = self.view_distance.radius();
        let next = if increase { radius + 1 } else { radius - 1 };
        if let Some(distance) = ViewDistance::from_radius(next) {
            self.view_distance = distance;
        }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(invalid("client settings exceed 4 KiB"));
        }
        let settings: Self = serde_json::from_slice(&bytes).map_err(invalid)?;
        if settings.schema != SCHEMA
            || !(MIN_PERCENT..=MAX_PERCENT).contains(&settings.sensitivity_percent)
            || settings.audio.volume_percent > 100
        {
            return Err(invalid("unsupported settings schema or value out of range"));
        }
        Ok(settings)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)?;
        let mut draft = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer_pretty(&mut draft, self).map_err(io::Error::other)?;
        draft.write_all(b"\n")?;
        draft.as_file().sync_all()?;
        draft.persist(path)?;
        Ok(())
    }
}

mod view_distance {
    use rustcraft_authority::ViewDistance;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S: Serializer>(
        distance: &ViewDistance,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(distance.radius())
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ViewDistance, D::Error> {
        let radius = u8::deserialize(deserializer)?;
        ViewDistance::from_radius(radius)
            .ok_or_else(|| serde::de::Error::custom("view_distance must be between 1 and 16"))
    }
}

pub(super) fn load_or_default(path: Option<&Path>) -> ClientSettings {
    path.map_or_else(ClientSettings::default, |path| {
        ClientSettings::load(path).unwrap_or_else(|error| {
            eprintln!("event=settings_load_failed path={path:?} error={error:?}");
            ClientSettings::default()
        })
    })
}

pub(super) fn default_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .map(|path| path.join(".config"))
        });
    config.map(|path| path.join("rustcraft/controls.json"))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_settings_and_inverts_only_vertical_mouse_motion() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config/controls.json");
        let mut settings = ClientSettings::load(&path).unwrap();
        let initial = settings.look_delta([1.0, 1.0]);
        settings.adjust_sensitivity(true);
        settings.invert_y = true;
        settings.save(&path).unwrap();
        let loaded = ClientSettings::load(&path).unwrap();
        assert_eq!(loaded, settings);
        assert_eq!(loaded.sensitivity_percent, 125);
        let motion = loaded.look_delta([1.0, 1.0]);
        assert!(motion[0] > initial[0] && initial[1] < 0.0);
        assert_eq!(motion[0], motion[1]);
    }

    #[test]
    fn rejects_unknown_and_out_of_range_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("controls.json");
        for contents in [
            r#"{"schema":2,"sensitivity_percent":100,"invert_y":false}"#,
            r#"{"schema":1,"sensitivity_percent":0,"invert_y":false}"#,
            r#"{"schema":1,"sensitivity_percent":100,"invert_y":false,"audio":{"volume_percent":101,"muted":false}}"#,
            r#"{"schema":1,"sensitivity_percent":100,"invert_y":false,"view_distance":0}"#,
            r#"{"schema":1,"sensitivity_percent":100,"invert_y":false,"view_distance":17}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert_eq!(
                ClientSettings::load(&path).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn persists_the_five_view_distance_samples() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("controls.json");
        let mut settings = ClientSettings::default();
        settings.adjust_view_distance(false);
        settings.adjust_view_distance(false);
        for radius in [1, 4, 8, 12, 16] {
            for _ in settings.view_distance.radius()..radius {
                settings.adjust_view_distance(true);
            }
            settings.save(&path).unwrap();
            assert_eq!(
                ClientSettings::load(&path).unwrap().view_distance.radius(),
                radius
            );
        }
        settings.adjust_view_distance(true);
        assert_eq!(settings.view_distance.radius(), 16);
    }

    #[test]
    fn loads_existing_mouse_settings_and_round_trips_sound_preferences() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("controls.json");
        fs::write(
            &path,
            r#"{"schema":1,"sensitivity_percent":125,"invert_y":true}"#,
        )
        .unwrap();
        let mut settings = ClientSettings::load(&path).unwrap();
        assert_eq!(settings.sensitivity_percent, 125);
        assert!(settings.invert_y);
        assert_eq!(settings.audio, AudioSettings::default());
        assert_eq!(settings.view_distance.radius(), 2);
        settings.audio.adjust_volume(false);
        settings.audio.muted = true;
        settings.adjust_view_distance(false);
        settings.save(&path).unwrap();
        let saved = ClientSettings::load(&path).unwrap();
        assert_eq!(saved, settings);
        assert_eq!(saved.audio.volume_percent, 60);
        assert_eq!(saved.audio.gain(), 0.0);
        assert_eq!(saved.view_distance.radius(), 1);
    }
}

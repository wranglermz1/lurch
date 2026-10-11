//! Movement sound through the game's own Miles (`crate::miles`): each command's cues and the
//! state it left, and the viewmodel's sequence events, go through the sound layer
//! (`lurch::sound::Sfx`), which names what plays; the listener follows the camera, as the
//! oracle's 0x956720 sets it from the view each frame.
//!
//! Not modelled: the oracle's per-event controllers, occlusion and DSP environments
//! (0x954c60), muting while the window is out of focus (0x959500, 0x5beb30), `AmmoClipPct`
//! (0x5bec80), and the surface a probe hits (the test map has none: every surface is
//! `default`).

use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use lurch::sim::math::V3;
use lurch::sound::{Mixer, Sfx, Surfaces};

use crate::data::Data;
use crate::miles::Miles;
use crate::sim::{FrameCues, Map, Pose as EyePose};
use crate::viewmodel::{Animate, VmEvents};
use crate::{common, Config, Frame};

/// ConVar `sound_volume` (image value).
pub const SOUND_VOLUME: f32 = 1.0;
/// The global controllers the client's sound frame sets that hold still here: the volume
/// ConVars `sound_volume_voice`, `sound_volume_music_lobby`, `sound_volume_music_game`,
/// `sound_volume_dialogue` and `sound_volume_sfx` (image values 1) times 100 (0x5beb30), and a
/// pilot's `IsTitan`, `LowHealth` and `TitanHealth` (0x5bec80). Without `SfxVolume` the
/// movement sounds play silent.
pub const CONTROLLERS: [(&CStr, f32); 8] = [
    (c"VoiceCommVolume", 100.0),
    (c"LobbyMusicVolume", 100.0),
    (c"GameMusicVolume", 100.0),
    (c"DialogueVolume", 100.0),
    (c"SfxVolume", 100.0),
    (c"IsTitan", 0.0),
    (c"LowHealth", 100.0),
    (c"TitanHealth", 100.0),
];
/// Instances kept per sound name, so a loop can be stopped.
const KEPT: usize = 4;

#[derive(Resource)]
struct Opening(Mutex<Receiver<Result<Miles, String>>>);

#[derive(Resource)]
struct Audio {
    miles: Arc<Miles>,
    sfx: Option<Sfx>,
    playing: HashMap<String, Vec<u64>>,
    speed: Option<f32>,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let Some(install) = app.world().resource::<Config>().r5r.clone() else { return };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(Miles::open(&install));
        });
        app.insert_resource(Opening(Mutex::new(rx))).add_systems(Update, (opened, play.run_if(resource_exists::<Audio>)).chain().after(Animate).in_set(Frame::View));
    }
}

fn opened(mut commands: Commands, opening: Option<Res<Opening>>) {
    let Some(opening) = opening else { return };
    let Ok(result) = opening.0.lock().unwrap().try_recv() else { return };
    commands.remove_resource::<Opening>();
    let miles = match result {
        Ok(m) => Arc::new(m),
        Err(e) => {
            warn!("no sound: {e}");
            return;
        }
    };
    info!("sound: {} events", miles.events.len());
    miles.set_master_volume(SOUND_VOLUME);
    for (name, value) in CONTROLLERS {
        miles.controller(name, value);
    }
    let service = miles.clone();
    std::thread::spawn(move || loop {
        if !service.service() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });
    commands.insert_resource(Audio { miles, sfx: None, playing: HashMap::new(), speed: None });
}

/// The test map's surfaces: all `default`.
struct DefaultSurface;

impl Surfaces for DefaultSurface {
    fn name(&self, _: u32) -> Option<&str> {
        Some("default")
    }
}

struct Out<'a> {
    miles: &'a Miles,
    playing: &'a mut HashMap<String, Vec<u64>>,
}

impl Mixer for Out<'_> {
    fn play(&mut self, name: &str, at: V3) {
        if let Some(id) = self.miles.play(name, at, 1.0) {
            let ids = self.playing.entry(name.to_string()).or_default();
            if ids.len() == KEPT {
                ids.remove(0);
            }
            ids.push(id);
        } else {
            debug!("sound {name}: no such event");
        }
    }

    fn stop(&mut self, name: &str) {
        for id in self.playing.remove(name).unwrap_or_default() {
            self.miles.stop(id);
        }
    }
}

/// AngleVectors: forward and up for (pitch, yaw, roll) in degrees.
fn forward_up(a: V3) -> (V3, V3) {
    let (sp, cp) = a[0].to_radians().sin_cos();
    let (sy, cy) = a[1].to_radians().sin_cos();
    let (sr, cr) = a[2].to_radians().sin_cos();
    ([cp * cy, cp * sy, -sp], [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp])
}

fn play(mut audio: ResMut<Audio>, data: Option<Res<Data>>, steps: Res<FrameCues>, eye: Res<EyePose>, vm: Option<ResMut<VmEvents>>, mut map: ResMut<Map>) {
    let audio = &mut *audio;
    if audio.sfx.is_none() {
        let Some(data) = data else { return };
        audio.sfx = Some(Sfx::new(data.0.sounds.clone()));
    }
    let (fwd, up) = forward_up(eye.0.angles);
    audio.miles.listener(eye.0.origin, fwd, up);
    if let Some(step) = steps.0.last() {
        let v = step.sound.velocity;
        let speed = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if audio.speed != Some(speed) {
            audio.speed = Some(speed);
            audio.miles.controller(c"PlayerSpeed", speed);
        }
    }
    let sfx = audio.sfx.as_mut().unwrap();
    let mut out = Out { miles: &audio.miles, playing: &mut audio.playing };
    for step in &steps.0 {
        for cue in &step.cues {
            sfx.cue(cue, step.sound.origin, &mut map.0, &DefaultSurface, &mut out);
        }
        sfx.after_move(&step.sound, &mut map.0, &DefaultSurface, &mut out);
    }
    for (ev, options) in vm.map(|mut v| std::mem::take(&mut v.0)).unwrap_or_default() {
        sfx.anim_event(ev, &options, eye.0.origin, &mut out);
    }
    audio.miles.submit();
}

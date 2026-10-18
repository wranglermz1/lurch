//! Movement sound: what the player hears in answer to movement, named the way the oracle
//! names it. A name reaches audio through a `Mixer`; the names themselves come from the
//! player settings (`PlayerSounds`) and the surface table (`Surfaces`), both game data that a
//! later session loads. Until then a name with an empty part is silent, as in the game.
//!
//! Only the local player's (`_1p`) sounds are modelled: the `_3p` ones are what other
//! players hear.

use crate::sim::cue::{Cue, Probe, StepSurface, PROBE_HALF_WIDTH};
use crate::sim::math::{sub, V3};
use crate::sim::{HullQuery, Tracer};

/// The player settings' sound strings (playersettings `.set`).
#[derive(Clone, Debug, Default)]
pub struct PlayerSounds {
    /// `footstep_type`: the first part of footstep, landing and jump names.
    pub footstep_type: String,
    /// `stealthSounds`: every footstep, landing and jump name gains `.stealth`.
    pub stealth: bool,
    pub stand_to_crouch: String,
    pub crouch_to_stand: String,
    /// `sound_jumpjet_slide_start_1p`, played when a slide gets its boost.
    pub slide_boost: String,
    /// `sound_freefall_{start,body,finish}_1p`.
    pub freefall: Phases,
    /// `sound_slide_prefix`: the slide loop is `<prefix>.<start|bump|stop>.<surface>_1p`.
    pub slide_prefix: String,
    /// `sound_grapple_*_1p`.
    pub grapple: GrappleSounds,
}

#[derive(Clone, Debug, Default)]
pub struct GrappleSounds {
    pub fire: String,
    pub retract: String,
    pub reset: String,
    pub traverse: String,
    pub reel_in_target: String,
    pub swing: String,
}

impl PlayerSounds {
    /// The sound strings of a player settings asset (`settings/player/mp/*.rpak`).
    pub fn from_settings(s: &crate::assets::settings::Settings) -> PlayerSounds {
        let get = |k: &str| s.str(k).unwrap_or("").to_string();
        PlayerSounds {
            footstep_type: get("footstep_type"),
            stealth: s.bool("stealthSounds").unwrap_or(false),
            stand_to_crouch: get("sound_standToCrouch_1p"),
            crouch_to_stand: get("sound_crouchToStand_1p"),
            slide_boost: get("sound_jumpjet_slide_start_1p"),
            freefall: Phases { start: get("sound_freefall_start_1p"), body: get("sound_freefall_body_1p"), finish: get("sound_freefall_finish_1p") },
            slide_prefix: get("sound_slide_prefix"),
            grapple: GrappleSounds {
                fire: get("sound_grapple_fire_1p"),
                retract: get("sound_grapple_retract_1p"),
                reset: get("sound_grapple_reset_1p"),
                traverse: get("sound_grapple_traverse_1p"),
                reel_in_target: get("sound_grapple_reel_in_target_1p"),
                swing: get("sound_grapple_swing_1p"),
            },
        }
    }
}

/// A sound in three parts: a one-shot, a loop, and a one-shot when the loop is stopped.
#[derive(Clone, Debug, Default)]
pub struct Phases {
    pub start: String,
    pub body: String,
    pub finish: String,
}

/// Surface property names by index (`surfaceproperties`), the last part of most names.
pub trait Surfaces {
    fn name(&self, surface: u32) -> Option<&str>;
}

/// Where sound names become audio. A name may loop; `stop` ends every instance of it.
pub trait Mixer {
    fn play(&mut self, name: &str, at: V3);
    fn stop(&mut self, name: &str);
}

/// ConVar `skip_sounds`: a jump just after landing would be named `.skip.` instead of `.jump.`.
pub const SKIP_SOUNDS: bool = false;
/// ConVars `freefall_sound_height` and `freefall_sound_autoplay_time`.
pub const FREEFALL_HEIGHT: f32 = 200.0;
pub const FREEFALL_AUTOPLAY: f32 = 1.0;
/// The ground probe's trace mask.
pub const PROBE_MASK: u32 = 0x640_403b;

pub fn surface_name(type_: &str, middle: &str, surface: &str, stealth: bool) -> Option<String> {
    if type_.is_empty() || surface.is_empty() {
        return None;
    }
    let name = format!("{type_}.{middle}.{surface}");
    Some(if stealth { format!("{name}.stealth") } else { name })
}

/// The slide loop: `<prefix>.start.<surface>_1p` when it begins, `bump` when the surface
/// changes under it, and `<prefix>.stop.<surface>_1p` after it is stopped.
pub fn slide_name(prefix: &str, phase: &str, surface: &str) -> Option<String> {
    (!prefix.is_empty()).then(|| format!("{prefix}.{phase}.{surface}_1p"))
}

/// What one player sounds like over time: the loops it has running, and the probes it waits on.
pub struct Sfx {
    pub sounds: PlayerSounds,
    slide: Option<(u32, String)>,
    air: Option<Phases>,
    last_vz: f32,
}

/// The per-call state the sound layer reads after movement, beyond the cues.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin: V3,
    pub hull_height: f32,
    pub velocity: V3,
    pub up: V3,
    pub on_ground: bool,
    pub sliding: bool,
    pub dead: bool,
    pub ladder: bool,
    pub curtime: f32,
    /// When the player last stood (`left_ground_time`).
    pub left_ground_time: f32,
}

/// What the server's zipline script plays for the rider (the install's `pilot/_zipline.gnut`,
/// the player's half of EmitDifferentSoundsOnEntityForPlayerAndWorld): attach as the mount
/// starts, the loop once it completes, and on letting go the loop stopped and detach.
const ZIPLINE_ATTACH: &str = "player_zipline_attach";
const ZIPLINE_LOOP: &str = "player_zipline_loop";
const ZIPLINE_DETACH: &str = "player_zipline_detach";

/// What the impact effects `grapple` and `grapple_extract` play for the local player's hook on
/// a `default` surface (the impact table the game loads at run time; every surface here is
/// `default`).
const GRAPPLE_IMPACT: &str = "Default.Grapple.Impact_1P_vs_3P";
const GRAPPLE_EXTRACT: &str = "Default.Grapple.Extract_1P_vs_3P";

/// What the local player's blow sounds like on a body (`flesh`) or anything else. The fists'
/// impact table `melee_human` is loaded at run time and not read: these are the game's sound
/// events of its names, flesh's and the `default` surface's, and an heirloom's own.
pub fn melee_impact(heirloom: Option<&crate::assets::Heirloom>, flesh: bool) -> &'static str {
    match (heirloom, flesh) {
        (Some(h), true) => h.flesh_impact,
        (Some(h), false) => h.wall_impact,
        (None, true) => "player_melee_impact_flesh_1p_vs_3p",
        (None, false) => "Player_Melee_Impact_Default_1p_vs_3p",
    }
}

/// The script callbacks the first-person proxy's mantles call that play a sound, and the sound
/// (the install's `cl_player.gnut`, SetupFirstPersonProxyEvents: EmitSoundOnEntity on the proxy).
const CALLBACK_SOUNDS: &[(&str, &str)] = &[
    ("mantle_smallmantle", "mantle_smallmantle"),
    ("mantle_mediummantle", "mantle_mediummantle"),
    ("mantle_lowmantle", "mantle_lowmantle"),
    ("mantle_extralowmantle", "mantle_extralow"),
];

impl Sfx {
    pub fn new(sounds: PlayerSounds) -> Self {
        Sfx { sounds, slide: None, air: None, last_vz: 0.0 }
    }

    fn named(&self, surfaces: &dyn Surfaces, middle: &str, surface: Option<u32>) -> Option<String> {
        let surface = surfaces.name(surface?)?;
        surface_name(&self.sounds.footstep_type, middle, surface, self.sounds.stealth)
    }

    /// Plays what a cue sounds like. Probes are answered by `world` now; in the oracle the
    /// answer is a call late, which is when the cue is raised.
    pub fn cue(&mut self, cue: &Cue, at: V3, world: &mut dyn Tracer, surfaces: &dyn Surfaces, mixer: &mut dyn Mixer) {
        let name = match *cue {
            Cue::Footstep { gait, surface, .. } => {
                let surface = match surface {
                    StepSurface::Probe(p) => probe(world, p),
                    StepSurface::Ground(s) => s,
                };
                self.named(surfaces, gait.name(), surface)
            }
            Cue::Jump { skip, surface, .. } => self.named(surfaces, if skip && SKIP_SOUNDS { "skip" } else { "jump" }, surface),
            Cue::LandSound { heavy, probe: p } => self.named(surfaces, if heavy { "heavyland" } else { "land" }, probe(world, p)),
            Cue::Crouch => Some(self.sounds.stand_to_crouch.clone()),
            Cue::Stand => Some(self.sounds.crouch_to_stand.clone()),
            Cue::SlideBoost => Some(self.sounds.slide_boost.clone()),
            Cue::ZiplineMount => Some(ZIPLINE_ATTACH.to_string()),
            Cue::ZiplineStart => Some(ZIPLINE_LOOP.to_string()),
            Cue::ZiplineStop => {
                mixer.stop(ZIPLINE_LOOP);
                Some(ZIPLINE_DETACH.to_string())
            }
            Cue::GrappleFire => Some(self.sounds.grapple.fire.clone()),
            Cue::GrappleCatch { at: p } => {
                mixer.stop(&self.sounds.grapple.retract);
                mixer.play(GRAPPLE_IMPACT, p);
                None
            }
            Cue::GrapplePull => Some(self.sounds.grapple.traverse.clone()),
            Cue::GrappleDetach { extract } => {
                if let Some(p) = extract {
                    mixer.play(GRAPPLE_EXTRACT, p);
                }
                let g = &self.sounds.grapple;
                for n in [&g.traverse, &g.swing, &g.reel_in_target] {
                    mixer.stop(n);
                }
                Some(g.retract.clone())
            }
            Cue::GrappleReset => {
                mixer.stop(&self.sounds.grapple.retract);
                Some(self.sounds.grapple.reset.clone())
            }
            Cue::Land { .. } | Cue::SprintEnd | Cue::GrappleAnim(_) => None,
        };
        if let Some(n) = name.filter(|n| !n.is_empty()) {
            mixer.play(&n, at);
        }
    }

    /// The sound a sequence event plays (the client's FireEvent, `0x5f3d50`): `AE_CL_PLAYSOUND`
    /// names it, `AE_CL_PLAYSOUND_FOR_TYPE` as `<footstep_type>.<options>`, the type of the
    /// player who owns the entity, and `AE_CL_VSCRIPT_CALLBACK` runs the script bound to its
    /// options. Ground contacts name third-person sounds only.
    pub fn anim_event(&self, event: u16, options: &str, at: V3, mixer: &mut dyn Mixer) {
        let name = match event {
            crate::anim::AE_CL_PLAYSOUND => options.to_string(),
            crate::anim::AE_CL_PLAYSOUND_FOR_TYPE => format!("{}.{options}", self.sounds.footstep_type),
            crate::anim::AE_CL_VSCRIPT_CALLBACK => match CALLBACK_SOUNDS.iter().find(|(c, _)| *c == options) {
                Some((_, sound)) => sound.to_string(),
                None => return,
            },
            _ => return,
        };
        if !options.is_empty() {
            mixer.play(&name, at);
        }
    }

    /// The freefall loop and the slide loop, run once per movement call after the move.
    pub fn after_move(&mut self, f: &Frame, world: &mut dyn Tracer, surfaces: &dyn Surfaces, mixer: &mut dyn Mixer) {
        self.freefall(f, world, mixer);
        self.slide(f, world, surfaces, mixer);
        self.last_vz = f.velocity[2];
    }

    fn stop_air(&mut self, mixer: &mut dyn Mixer, at: V3) {
        if let Some(p) = self.air.take() {
            mixer.stop(&p.body);
            if !p.finish.is_empty() {
                mixer.play(&p.finish, at);
            }
        }
    }

    /// Falling with nothing within `FREEFALL_HEIGHT` below, or still falling a second after
    /// leaving the ground, starts the freefall loop; standing again ends it.
    fn freefall(&mut self, f: &Frame, world: &mut dyn Tracer, mixer: &mut dyn Mixer) {
        if f.on_ground {
            if crate::sim::math::is_world_up(f.up) {
                self.stop_air(mixer, f.origin);
            }
            return;
        }
        if f.ladder || f.velocity[2] >= 0.0 || self.air.is_some() {
            return;
        }
        let start = if self.last_vz < 0.0 {
            FREEFALL_AUTOPLAY < f.curtime - f.left_ground_time
        } else {
            let end = sub(f.origin, [0.0, 0.0, FREEFALL_HEIGHT]);
            world.hull(&HullQuery { start: f.origin, end, mins: [0.0; 3], maxs: [0.0; 3], mask: PROBE_MASK }).fraction == 1.0
        };
        if start {
            let p = self.sounds.freefall.clone();
            for n in [&p.start, &p.body].into_iter().filter(|n| !n.is_empty()) {
                mixer.play(n, f.origin);
            }
            self.air = Some(p);
        }
    }

    fn slide(&mut self, f: &Frame, world: &mut dyn Tracer, surfaces: &dyn Surfaces, mixer: &mut dyn Mixer) {
        let sliding = f.sliding && f.on_ground && !f.dead;
        if !sliding {
            if let Some((surface, name)) = self.slide.take() {
                mixer.stop(&name);
                if let Some(n) = slide_name(&self.sounds.slide_prefix, "stop", surfaces.name(surface).unwrap_or("")) {
                    mixer.play(&n, f.origin);
                }
            }
            return;
        }
        let here = probe(world, Probe::below(f.origin, [0.0, 0.0, 1.0], f.hull_height));
        let Some(surface) = here else { return };
        let phase = match &self.slide {
            Some((s, _)) if *s == surface => return,
            Some((_, name)) => {
                mixer.stop(name);
                "bump"
            }
            None => "start",
        };
        let name = slide_name(&self.sounds.slide_prefix, phase, surfaces.name(surface).unwrap_or("")).unwrap_or_default();
        if !name.is_empty() {
            mixer.play(&name, f.origin);
        }
        self.slide = Some((surface, name));
    }
}

/// The surface a ground probe lands on.
pub fn probe(world: &mut dyn Tracer, p: Probe) -> Option<u32> {
    let h = PROBE_HALF_WIDTH;
    let t = world.hull(&HullQuery { start: p.start, end: p.end, mins: [-h; 3], maxs: [h; 3], mask: PROBE_MASK });
    (t.fraction < 1.0 && !t.startsolid).then_some(t.surface)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::cue::{Gait, LandHeight};
    use crate::sim::Trace;

    struct Floor(u32);
    impl Tracer for Floor {
        fn hull(&mut self, q: &HullQuery) -> Trace {
            let z = 0.0 - q.mins[2];
            let fraction = if q.end[2] < z && q.start[2] >= z { (q.start[2] - z) / (q.start[2] - q.end[2]) } else { 1.0 };
            Trace { fraction, surface: self.0, entity: 1, ..Default::default() }
        }
    }
    struct Names;
    impl Surfaces for Names {
        fn name(&self, s: u32) -> Option<&str> {
            ["concrete", "metal"].get(s as usize).copied()
        }
    }
    #[derive(Default)]
    struct Log(Vec<String>);
    impl Mixer for Log {
        fn play(&mut self, name: &str, _: V3) {
            self.0.push(name.to_string());
        }
        fn stop(&mut self, name: &str) {
            self.0.push(format!("stop {name}"));
        }
    }

    fn sfx() -> Sfx {
        Sfx::new(PlayerSounds {
            footstep_type: "pilot".into(),
            stand_to_crouch: "crouch_1p".into(),
            freefall: Phases { start: "fall_start".into(), body: "fall_body".into(), finish: "fall_end".into() },
            slide_prefix: "slide".into(),
            ..Default::default()
        })
    }

    #[test]
    fn names_follow_the_oracles_formats() {
        let (mut s, mut log, mut w) = (sfx(), Log::default(), Floor(1));
        let probe = Probe { start: [0.0, 0.0, 60.0], end: [0.0, 0.0, -200.0] };
        s.cue(&Cue::Footstep { gait: Gait::Sprint, surface: StepSurface::Probe(probe), side: true }, [0.0; 3], &mut w, &Names, &mut log);
        s.cue(&Cue::Jump { wall: false, skip: true, surface: Some(0) }, [0.0; 3], &mut w, &Names, &mut log);
        s.cue(&Cue::LandSound { heavy: true, probe }, [0.0; 3], &mut w, &Names, &mut log);
        s.cue(&Cue::Land { speed: 300.0, height: LandHeight::Low }, [0.0; 3], &mut w, &Names, &mut log);
        s.cue(&Cue::Crouch, [0.0; 3], &mut w, &Names, &mut log);
        s.cue(&Cue::Stand, [0.0; 3], &mut w, &Names, &mut log);
        s.sounds.stealth = true;
        s.cue(&Cue::Footstep { gait: Gait::Crouch, surface: StepSurface::Ground(Some(0)), side: false }, [0.0; 3], &mut w, &Names, &mut log);
        s.anim_event(crate::anim::AE_CL_PLAYSOUND_FOR_TYPE, "effort_land_1p", [0.0; 3], &mut log);
        s.anim_event(crate::anim::AE_CL_PLAYSOUND, "KnuckleCrack", [0.0; 3], &mut log);
        s.anim_event(crate::anim::AE_GROUND_CONTACT_L, "LandSweetener_Low", [0.0; 3], &mut log);
        assert_eq!(
            log.0,
            ["pilot.sprint.metal", "pilot.jump.concrete", "pilot.heavyland.metal", "crouch_1p", "pilot.crouch.concrete.stealth", "pilot.effort_land_1p", "KnuckleCrack"]
        );
    }

    fn frame(z: f32, vz: f32, on_ground: bool, sliding: bool, t: f32) -> Frame {
        Frame { origin: [0.0, 0.0, z], hull_height: 72.0, velocity: [0.0, 0.0, vz], up: [0.0, 0.0, 1.0], on_ground, sliding, dead: false, ladder: false, curtime: t, left_ground_time: 0.0 }
    }

    #[test]
    fn freefall_starts_high_up_or_after_a_second_and_ends_on_landing() {
        let (mut s, mut log, mut w) = (sfx(), Log::default(), Floor(0));
        s.after_move(&frame(100.0, 10.0, false, false, 0.1), &mut w, &Names, &mut log);
        s.after_move(&frame(100.0, -10.0, false, false, 0.2), &mut w, &Names, &mut log);
        assert!(log.0.is_empty(), "ground within 200 units: {:?}", log.0);
        s.after_move(&frame(90.0, -20.0, false, false, 1.5), &mut w, &Names, &mut log);
        s.after_move(&frame(0.0, 0.0, true, false, 1.6), &mut w, &Names, &mut log);
        assert_eq!(log.0, ["fall_start", "fall_body", "stop fall_body", "fall_end"]);
        let mut log = Log::default();
        s.after_move(&frame(500.0, 10.0, false, false, 2.0), &mut w, &Names, &mut log);
        s.after_move(&frame(500.0, -1.0, false, false, 2.1), &mut w, &Names, &mut log);
        assert_eq!(log.0, ["fall_start", "fall_body"]);
    }

    #[test]
    fn slide_loop_starts_bumps_and_stops() {
        let (mut s, mut log, mut w) = (sfx(), Log::default(), Floor(0));
        s.after_move(&frame(0.0, 0.0, true, true, 0.0), &mut w, &Names, &mut log);
        s.after_move(&frame(0.0, 0.0, true, true, 0.1), &mut w, &Names, &mut log);
        w.0 = 1;
        s.after_move(&frame(0.0, 0.0, true, true, 0.2), &mut w, &Names, &mut log);
        s.after_move(&frame(0.0, 0.0, true, false, 0.3), &mut w, &Names, &mut log);
        assert_eq!(log.0, ["slide.start.concrete_1p", "stop slide.start.concrete_1p", "slide.bump.metal_1p", "stop slide.bump.metal_1p", "slide.stop.metal_1p"]);
    }
}

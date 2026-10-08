//! Replay recordings and print what they sound and look like: every cue movement raises, the
//! sound it names, and each change of body and viewmodel activity. Names that game data will
//! fill in are shown as `<footstep_type>`, `<surface N>` and the settings field's name.
//! Ground probes are answered by the arena's floor.

use lurch::anim::body::{Body, BodySettings, Frame};
use lurch::anim::viewmodel::Viewmodel;
use lurch::anim::{NoAnims, PlayerEvent};
use lurch::sim::cue::Cue;
use lurch::sim::math::V3;
use lurch::sound::{Mixer, Phases, PlayerSounds, Sfx, Surfaces};

const ARENA_FLOOR: f32 = -2688.0;

struct Placeholder(Vec<String>);

impl Surfaces for Placeholder {
    fn name(&self, surface: u32) -> Option<&str> {
        Some(self.0.get(surface as usize).map_or("<surface>", String::as_str))
    }
}

struct Print<'a>(&'a mut Vec<String>);

impl Mixer for Print<'_> {
    fn play(&mut self, name: &str, _: V3) {
        self.0.push(format!("play {name}"));
    }
    fn stop(&mut self, name: &str) {
        self.0.push(format!("stop {name}"));
    }
}

fn main() {
    let surfaces = Placeholder((0..256).map(|i| format!("<surface {i}>")).collect());
    let sounds = PlayerSounds {
        footstep_type: "<footstep_type>".into(),
        stealth: false,
        stand_to_crouch: "sound_standToCrouch_1p".into(),
        crouch_to_stand: "sound_crouchToStand_1p".into(),
        slide_boost: "sound_jumpjet_slide_start_1p".into(),
        freefall: Phases {
            start: "sound_freefall_start_1p".into(),
            body: "sound_freefall_body_1p".into(),
            finish: "sound_freefall_finish_1p".into(),
        },
        slide_prefix: "<sound_slide_prefix>".into(),
    };
    for path in std::env::args().skip(1) {
        println!("{path}");
        let (mut sfx, mut body, mut vm) = (Sfx::new(sounds.clone()), Body::new(BodySettings::default()), Viewmodel::default());
        let mut world = lurch::world::World::floor(ARENA_FLOOR);
        let (mut was_body, mut was_vm) = (None, None);
        for rec in lurch::ltr::open(&path).unwrap() {
            let rec = rec.unwrap();
            let (step, p, mv) = lurch::replay::run(&rec, None);
            let s = lurch::oracle::settings(&rec.settings);
            let g = lurch::oracle::globals(&rec.globals, p.lagged);
            let mut out = Vec::new();
            for cue in &step.cues {
                out.push(format!("{cue:?}"));
                sfx.cue(cue, mv.origin, &mut world, &surfaces, &mut Print(&mut out));
                if let Some(ev) = PlayerEvent::of(cue) {
                    body.event(ev, g.curtime);
                    vm.event(ev, &NoAnims);
                }
                if let Cue::Land { speed, .. } = cue {
                    body.landed(*speed);
                }
            }
            let frame = lurch::sound::Frame {
                origin: mv.origin,
                hull_height: 72.0,
                velocity: mv.velocity,
                up: p.up,
                on_ground: p.ground.is_some(),
                sliding: p.sliding,
                dead: p.dead,
                ladder: p.movetype == 9,
                curtime: g.curtime,
                left_ground_time: p.left_ground_time,
            };
            sfx.after_move(&frame, &mut world, &surfaces, &mut Print(&mut out));
            let f = Frame { p: &p, s: &s, velocity: mv.velocity, eye: mv.angles, curtime: g.curtime, frametime: g.frametime, traversal_type: 0 };
            body.update(&f, &NoAnims);
            vm.update(p.sprinting, false, g.curtime, g.frametime, &NoAnims);
            if body.activity != was_body {
                was_body = body.activity;
                out.push(format!("body {}", body.activity.and_then(|a| a.name()).unwrap_or("?")));
            }
            if vm.main != was_vm {
                was_vm = vm.main;
                out.push(format!("viewmodel {}", vm.main.and_then(|a| a.name()).unwrap_or("?")));
            }
            for line in out {
                println!("  {:x} {line}", rec.seq);
            }
        }
    }
}

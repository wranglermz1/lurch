//! Replay recordings and print what they sound and look like: every cue movement raises, the
//! sound it names, and each change of body activity.
//! `cues [--r5r INSTALL [--legend NAME]] RECORDING...`: with an install, the legend's own sound
//! names, the fists' viewmodel and the arms' first-person proxy, their sequences and the
//! sounds those play; without,
//! `<footstep_type>` and the settings fields' names. Surfaces are shown as `<surface N>`, and
//! ground probes are answered by the arena's floor.

use lurch::anim::body::{Body, BodySettings, Frame};
use lurch::anim::model::ModelAnims;
use lurch::anim::proxy::Proxy;
use lurch::anim::viewmodel::{Viewmodel, WeaponTimes};
use lurch::anim::{NoAnims, PlayerEvent};
use lurch::sim::cue::Cue;
use lurch::sim::math::V3;
use lurch::sound::{GrappleSounds, Mixer, Phases, PlayerSounds, Sfx, Surfaces};

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
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut flag = |name: &str| args.iter().position(|a| a == name).map(|i| args.drain(i..i + 2).nth(1).expect("flag without a value"));
    let (install, legend) = (flag("--r5r"), flag("--legend").unwrap_or("wraith".into()));
    let surfaces = Placeholder((0..256).map(|i| format!("<surface {i}>")).collect());
    let mut sounds = PlayerSounds {
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
        grapple: GrappleSounds {
            fire: "sound_grapple_fire_1p".into(),
            retract: "sound_grapple_retract_1p".into(),
            reset: "sound_grapple_reset_1p".into(),
            traverse: "sound_grapple_traverse_1p".into(),
            reel_in_target: "sound_grapple_reel_in_target_1p".into(),
            swing: "sound_grapple_swing_1p".into(),
        },
    };
    let mut fists = None;
    if let Some(dir) = install {
        let l = lurch::assets::loadout(std::path::Path::new(&dir), &legend).unwrap();
        sounds = PlayerSounds::from_settings(&l.legend);
        let class = l.legend.str("classActivityModifier").filter(|m| !m.is_empty()).map(str::to_string);
        let vm = Viewmodel::new(class.into_iter().collect(), WeaponTimes::from_script(|k| l.weapon_script.get(k)?.parse().ok()));
        fists = Some((ModelAnims::new(&l.weapon), vm, ModelAnims::new(&l.arms)));
    }
    for path in args {
        println!("{path}");
        let (mut sfx, mut body) = (Sfx::new(sounds.clone()), Body::new(BodySettings::default()));
        let mut fists = fists.as_ref().map(|(set, vm, arms)| (set, vm.clone(), arms, Proxy::default()));
        let mut world = lurch::world::World::floor(ARENA_FLOOR);
        let (mut was_body, mut was_vm, mut was_proxy) = (None, None, None);
        for (i, rec) in lurch::ltr::open(&path).unwrap().enumerate() {
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
            if body.activity != was_body {
                was_body = body.activity;
                out.push(format!("body {}", body.activity.and_then(|a| a.name()).unwrap_or("?")));
            }
            if let Some((set, vm, arms, proxy)) = &mut fists {
                if i == 0 {
                    vm.deploy(set, p.sprinting, g.curtime);
                }
                let traversing = p.movetype == lurch::sim::mantle::MOVETYPE_TRAVERSE;
                let was = proxy.visible;
                if (traversing || p.climb_arms) && !was {
                    vm.holster(set, g.curtime);
                }
                vm.command(set, &step.cues, p.sprinting, g.curtime);
                for (ev, options) in vm.frame(set, g.curtime + g.frametime).1 {
                    sfx.anim_event(ev, &options, mv.origin, &mut Print(&mut out));
                }
                let now = (vm.activity, vm.main.seq);
                if was_vm != Some(now) {
                    was_vm = Some(now);
                    let label = vm.main.seq.map_or("?", |q| set.seqs[q.0 as usize].label.as_str());
                    out.push(format!("viewmodel {} ({label})", vm.activity.and_then(|a| a.name()).unwrap_or("?")));
                }
                let traversal = traversing.then_some(lurch::anim::proxy::Traversal {
                    ty: p.traversal_type,
                    cycle: p.traversal_cycle,
                    gun_up: p.traversal_gun != 0,
                    start_time: p.traversal_start_time,
                    ledge: p.traversal_ref,
                    forward: p.traversal_forward,
                    mid: p.traversal_mid,
                    end: p.traversal_end,
                });
                proxy.command(arms, traversal, !vm.hidden(g.curtime), s.mantle.jumpoff_duration, g.curtime);
                let rising = lurch::oracle::move_in(&rec).velocity[2] > f32::EPSILON;
                proxy.climb(arms, p.climb_arms.then_some(lurch::anim::proxy::Climb { normal: p.up, rising }), g.curtime);
                if was && !proxy.visible {
                    vm.unholster(set, g.curtime);
                }
                for (ev, options) in proxy.frame(arms, g.curtime + g.frametime).1 {
                    sfx.anim_event(ev, &options, mv.origin, &mut Print(&mut out));
                }
                let now = proxy.main.seq.filter(|_| proxy.visible);
                if was_proxy != Some(now) {
                    was_proxy = Some(now);
                    out.push(format!("proxy {}", now.map_or("hidden", |q| arms.seqs[q.0 as usize].label.as_str())));
                }
            }
            for line in out {
                println!("  {:x} {line}", rec.seq);
            }
        }
    }
}

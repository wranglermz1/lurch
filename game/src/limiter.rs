use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::{Monitor, PrimaryMonitor};

use crate::{common, config, Config};

/// Sleeping wakes late by up to this much, so the last stretch is spun.
const SPIN: Duration = Duration::from_micros(500);

/// Holds frames to a period: each `wait` returns no earlier than one period after the last.
#[derive(Clone, Debug)]
pub struct Limiter {
    pub period: Duration,
    next: Instant,
}

impl Limiter {
    pub fn new(fps: u32) -> Self {
        Limiter { period: Duration::from_secs_f64(1.0 / fps as f64), next: Instant::now() }
    }

    pub fn wait(&mut self) {
        let now = Instant::now();
        if self.next > now + SPIN {
            std::thread::sleep(self.next - now - SPIN);
        }
        while Instant::now() < self.next {
            std::hint::spin_loop();
        }
        self.next = (self.next + self.period).max(Instant::now());
    }
}

#[derive(Resource)]
struct FrameLimiter(Limiter, FpsCap);

/// The cap in frames a second, 0 for none, as `f32` bits: the main world sets it and the
/// render world waits on it.
#[derive(Resource, Clone, Default)]
pub struct FpsCap(Arc<AtomicU32>);

impl FpsCap {
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn set(&self, fps: f32) {
        self.0.store(fps.to_bits(), Ordering::Relaxed);
    }
}

fn limit(mut l: ResMut<FrameLimiter>) {
    let fps = l.1.get();
    if fps.is_nan() || fps <= 0.0 {
        return;
    }
    let period = Duration::from_secs_f64(1.0 / fps as f64);
    if l.0.period != period {
        l.0.period = period;
    }
    l.0.wait();
}

/// The engine's frame cap (`0x2970e0`) for `fps_max`, on a desktop refreshing `refresh` times
/// a second (whole), vsync on or off: -1 is the refresh (59 and 60 as 59.94), at least 144
/// without vsync; 0 or less is 300; at most 10000. Not modelled: the path a fullscreen window
/// with vsync takes (`fps_max_vsync`, `fps_max_use_refresh`), and the adaptive vsync modes'
/// 5% more.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // the engine's compare: a NaN cap is 300
pub fn engine_cap(fps_max: f32, refresh: Option<u32>, vsync: bool) -> f32 {
    let mut f = fps_max;
    if f == -1.0 {
        f = refresh.unwrap_or(60) as f32;
        if f == 59.0 || f == 60.0 {
            f = 59.94;
        }
        if !vsync {
            f = if f > 144.0 { f } else { 144.0 };
        }
    }
    if !(0.0 < f) {
        f = 300.0;
    }
    (10000.0f64).min(f as f64) as f32
}

/// The cap each frame: the engine's for the `fps_max` setting when lurch acts on it, else
/// `--fps-max` (0 none).
fn set_cap(cap: Res<FpsCap>, cfg: Res<Config>, monitors: Query<&Monitor, With<PrimaryMonitor>>) {
    let fps = match cfg.fps_max_setting {
        Some(v) => engine_cap(v, monitors.single().ok().and_then(|m| m.refresh_rate_millihertz).map(|mhz| mhz / 1000), cfg.vsync),
        None => cfg.fps_max as f32,
    };
    cap.set(fps);
}

/// The fps cap. Windowed without pipelining it waits in the render sub-app after present,
/// so the sleep counts in the next frame's delta and none sits between input and present.
/// Off in script mode.
pub struct LimiterPlugin;

impl Plugin for LimiterPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        let cfg = config(app);
        if cfg.scripted {
            return;
        }
        let cap = FpsCap::default();
        cap.set(cfg.fps_max as f32);
        let limiter = FrameLimiter(Limiter::new(1), cap.clone());
        app.insert_resource(cap).add_systems(First, set_cap);
        match app.get_sub_app_mut(RenderApp).filter(|_| !cfg.pipelined) {
            Some(render) => {
                render.insert_resource(limiter).add_systems(Render, limit.in_set(RenderSystems::Cleanup));
            }
            None => {
                app.insert_resource(limiter).add_systems(Last, limit);
            }
        }
    }
}

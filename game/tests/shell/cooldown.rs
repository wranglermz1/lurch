use lurch::sim::cue::Cue;
use lurch::sim::pilot::Pilot;
use lurch_game::cooldown::{GrappleClip, CLIP_SIZE};

/// A catch with no travel costs the drain's base of 100 rounds once the script's next pass
/// (0.1 s on) finds the grapple over, and 10 s at 10 a second bring 100 back.
#[test]
fn a_short_grapple_costs_the_base_and_refills_at_the_rate() {
    let mut c = GrappleClip::default();
    let mut pilot = Pilot::new([0.0; 3]);
    pilot.player.grappling = true;
    pilot.cues.push(Cue::GrappleCatch { at: [0.0; 3] });
    let dt = 0.004;
    let mut now = 10.0;
    c.think(now, dt, &pilot);
    assert_eq!(c.clip, 0);
    pilot.cues.clear();
    pilot.player.grappling = false;
    now += 0.1;
    c.think(now, dt, &pilot);
    assert_eq!(c.clip, CLIP_SIZE - 100);
    for _ in 0..2500 {
        now += dt;
        c.think(now, dt, &pilot);
    }
    assert!((CLIP_SIZE - 1..=CLIP_SIZE).contains(&c.clip), "{}", c.clip);
}

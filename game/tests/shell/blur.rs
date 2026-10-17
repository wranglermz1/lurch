use lurch_game::blur::gaussian_taps;

/// `dev/blurfilter` names a kernel 25 texels wide in 13 fetches, which the generator gives;
/// the table caps it at 18: the centre and five taps a side, the last on its own texel, the
/// weights over both sides summing to one.
#[test]
fn the_screen_blur_kernel_is_the_capped_gaussian() {
    assert_eq!(gaussian_taps(25, 19).len() * 2 - 1, 13);
    let taps = gaussian_taps(18, 19);
    assert_eq!(taps.len(), 6);
    assert_eq!(taps[0][0], 0.0);
    assert_eq!(taps[5][0], 9.0);
    for (i, t) in taps[1..5].iter().enumerate() {
        assert!(t[0] > (2 * i + 1) as f32 && t[0] < (2 * i + 2) as f32, "{t:?}");
    }
    let sum = taps[0][1] + 2.0 * taps[1..].iter().map(|t| t[1]).sum::<f32>();
    assert!((sum - 1.0).abs() < 1e-6, "{sum}");
}

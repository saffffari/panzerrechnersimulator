use std::fs;

#[test]
fn campaign_take_can_select_a_different_seed_layout_and_geometry() {
    let source = fs::read_to_string("src/showreel.rs").expect("read showreel source");

    for control in [
        "KESSEL_SHOWREEL_FINAL_SEED",
        "KESSEL_SHOWREEL_FINAL_LAYOUT",
        "KESSEL_SHOWREEL_FINAL_GEOMETRY_M",
    ] {
        assert!(source.contains(control), "missing take control {control}");
    }
    assert!(source.contains("final_layout: configured_showreel_layout(defaults.final_layout)?"));
    assert!(source.contains("CampaignFinalLayout::Ring"));
    assert!(source.contains("stage_showreel_t34_wave(spec.count, spec.seed, spec.ring_m)"));
    assert!(source.contains("let config = CampaignWaveConfig::from_environment()?;"));
}

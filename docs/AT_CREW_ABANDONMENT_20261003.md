# Destroyed AT gun crew abandonment

An actual first `wreck_at_gun` transition now tells only surviving assigned original and
recruited crew to flee from that weapon's fixed position. It neither invents casualties nor
reads a hidden enemy's transform. Wrecked guns no longer own crew; fleeing riflemen cannot
be recruited to another gun. Repeated wreck notifications do not reset the retreat.

The shared Soldier `begin_flee_from` / `is_fleeing` / `advance_flee` API takes priority in
passive, scripted and mixed combat authority. Retreat uses one bounded translation per
update and existing terrain/door/world movement resolution in the mixed route, with firing
suppressed. Dead actors remain dead and stationary. It currently persists until scenario
reset; it is not a morale/recovery simulation. Physical wounds/crawling are owned separately.

Source test: `destroyed_post_retreat_overrides_patrol_and_never_revives_dead_crew`.
Build and visual acceptance are coordinated by the root agent; neither has been claimed here.

//! Neuroevolution policy net — the M1 seam from docs/NEUROEVOLUTION.md, at pipeline-smoke-test
//! scale. A tiny fixed-topology MLP (`OBS → tanh(HID) → tanh(ACT)`) drives enemy maneuver +
//! fire-discipline; the gun lay, stealth gates, reload and ballistics all stay with the sim (the
//! net proposes, the rules dispose). Grows into `src/ai/{net,obs,roles,evolve}.rs` as M2-M4 land.
//!
//! Determinism contract (same as the rest of the game): NO `rand` — every "random" number is a
//! counter-seeded `hash01`, and mutation draws are Box-Muller over two hash01 samples. A genome is
//! fully reproduced by `(base_seed, generation, parent, child)`.

use std::io::{Read, Write};

/// The July executor width (`KSL1`): the AI Tiger, the platoon wingmen and the old T-34 policy.
/// `Game::tiger_executor_obs` and `platoon::wingman_policy_obs` fill exactly this.
pub const OBS: usize = 14;
/// Hidden width of a July net.
pub const HID: usize = 16;
/// The T-34 executor width (`KSL2`, 2026-11-02): the 14 July inputs unchanged in slots 0-13, then
/// the crew's view of the ground, its memory of the Tiger, its mates and its orders — see
/// `Game::executor_obs`.
pub const T34_OBS: usize = 31;
/// Hidden width of a T-34 net: the July net's 16 units, then 11 more for the new inputs.
pub const T34_HID: usize = 27;
/// Actions: [drive, steer, fire(>0)].
pub const ACT: usize = 3;
/// Total genome length of a July-width net: two dense layers + biases.
pub const GENOME: usize = genome_len(OBS, HID);
/// Total genome length of a T-34 net.
pub const T34_GENOME: usize = genome_len(T34_OBS, T34_HID);

/// Genome length of a `[obs] → tanh(hid) → tanh(ACT)` net.
pub const fn genome_len(obs: usize, hid: usize) -> usize { obs * hid + hid + hid * ACT + ACT }

/// Weight-artifact header magic + version: "KSL1" is the 14x16 July format, "KSL2" the 31x27
/// T-34 format. Each magic admits exactly one shape; anything else is refused at load.
const MAGIC: [u8; 4] = *b"KSL1";
const MAGIC_T34: [u8; 4] = *b"KSL2";

/// A `[obs] → tanh(hid) → tanh(ACT)` MLP over a flat weight vector (layer-major, weights then
/// biases). Pure, no-alloc hot path aside from the output array.
#[derive(Clone)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Mlp {
    pub w: Vec<f32>, // len == genome_len(obs, hid)
    /// Input width: `OBS` (KSL1) or `T34_OBS` (KSL2).
    #[serde(default = "july_width")]
    pub obs: usize,
    /// Hidden width: `HID` (KSL1) or `T34_HID` (KSL2).
    #[serde(default = "july_hidden")]
    pub hid: usize,
}

fn july_width() -> usize { OBS }
fn july_hidden() -> usize { HID }

impl Mlp {
    /// A net over a flat genome; the shape is read from its length (`GENOME` or `T34_GENOME`).
    /// Any other length is a programming error.
    pub fn new(w: Vec<f32>) -> Self {
        let (obs, hid) = match w.len() {
            GENOME => (OBS, HID),
            T34_GENOME => (T34_OBS, T34_HID),
            n => panic!("genome length {n} is neither {GENOME} (KSL1, {OBS}x{HID}) nor {T34_GENOME} (KSL2, {T34_OBS}x{T34_HID})"),
        };
        Mlp { w, obs, hid }
    }

    /// A July net as a T-34 net: its 16 units keep their weights over the first 14 inputs, every
    /// new input column and the 8 new units get zero weight, so the result answers every
    /// observation exactly as the July net did on its first 14 slots. How a July policy seeds the
    /// T-34 retraining without losing its manner.
    pub fn widened(&self) -> Self {
        assert_eq!((self.obs, self.hid), (OBS, HID), "only a July net widens");
        let (o, h) = (T34_OBS, T34_HID);
        let mut w = vec![0.0; T34_GENOME];
        for j in 0..HID {
            w[j * o..j * o + OBS].copy_from_slice(&self.w[j * OBS..(j + 1) * OBS]);
            w[o * h + j] = self.w[OBS * HID + j];
            for k in 0..ACT {
                w[o * h + h + k * h + j] = self.w[OBS * HID + HID + k * HID + j];
            }
        }
        for k in 0..ACT {
            w[o * h + h + h * ACT + k] = self.w[OBS * HID + HID + HID * ACT + k];
        }
        Mlp::new(w)
    }

    /// Generation zero of the T-34 retraining: the July net (`widened`) plus four hand-wired
    /// reflex units over the new inputs, in the spirit of `tiger_doctrine_seed`: one competent
    /// ancestor for evolution to tune, not a script that overrides it. Each reflex's output bias
    /// is offset by its value on open ground with the Tiger in sight, so there the seed answers
    /// exactly as the July net did:
    /// - unit 16, room ahead: under ~12 m of room ahead it throttles back (reverses if it must);
    /// - unit 23, room astern: under ~12 m of room astern it pushes the throttle forward;
    /// - unit 17, the open side: steers toward the freer of the two front bearings;
    /// - units 18 and 22, stalled: after ~1.5 s of driving without moving, drive toward whichever
    ///   end has more room and turn;
    /// - units 19-21, blind with a memory: steer toward the remembered Tiger and drive on it,
    ///   firmly enough to override the July units' blind reverse (which otherwise see-saws the
    ///   throttle about zero);
    /// - units 24-26, blind with orders: steer toward the post or the beat's corner, drive on it
    ///   when it lies ahead.
    pub fn t34_reflex_seed(july: &Mlp) -> Self {
        let mut net = july.widened();
        let (o, h) = (T34_OBS, T34_HID);
        let bias_in = 11; // slot 11 is the constant 1
        let hb = o * h;
        let ow = hb + h;
        let ob = ow + h * ACT;
        let w = &mut net.w;
        let mut unit = |j: usize, inputs: &[(usize, f32)], bias: f32| {
            for &(i, v) in inputs { w[j * o + i] = v; }
            w[hb + j] = bias;
        };
        unit(16, &[(15, -4.0), (bias_in, 2.0)], 0.0); // >0 when the room ahead < 12 m
        unit(23, &[(17, -4.0), (bias_in, 2.0)], 0.0); // >0 when the room astern < 12 m
        unit(17, &[(16, 3.0), (14, -3.0)], 0.0); // + when the right front is the freer
        unit(18, &[(19, 5.0), (15, 3.0), (17, -3.0)], -5.0); // stalled: tanh(3 (ahead - astern))
        unit(22, &[(19, 5.0), (15, -3.0), (17, 3.0)], -5.0); // stalled: tanh(3 (astern - ahead))
        unit(19, &[(20, 3.0), (9, 4.0)], 4.0); // blind: 3 sin(bearing); in sight: saturated +1
        unit(20, &[(20, 3.0), (9, -4.0)], -4.0); // blind: 3 sin(bearing); in sight: saturated -1
        unit(21, &[(23, 2.0), (9, -4.0)], -4.0); // blind with a memory: +; otherwise -1
        unit(24, &[(27, 3.0), (9, 4.0)], 4.0); // blind: 3 sin(bearing to orders); in sight: +1
        unit(25, &[(27, 3.0), (9, -4.0)], -4.0); // blind: 3 sin(bearing to orders); in sight: -1
        unit(26, &[(28, 2.0), (9, -4.0)], -4.0); // blind, orders ahead: +; otherwise <= 0
        let mut out = |k: usize, j: usize, v: f32, baseline: f32| {
            w[ow + k * h + j] = v;
            w[ob + k] -= v * baseline;
        };
        let open = |a: f32| a.tanh();
        out(0, 16, -1.2, open(-2.0));
        out(0, 23, 1.2, open(-2.0));
        out(1, 17, 0.6, 0.0);
        out(0, 18, 1.0, open(-5.0));
        out(0, 22, -1.0, open(-5.0));
        out(1, 18, 0.5, open(-5.0));
        out(1, 22, 0.5, open(-5.0));
        out(1, 19, 0.8, open(8.0));
        out(1, 20, 0.8, open(-8.0));
        out(0, 21, 1.2, open(2.0 - 8.0));
        out(1, 24, 0.6, open(8.0));
        out(1, 25, 0.6, open(-8.0));
        out(0, 26, 0.9, open(-8.0));
        net
    }

    /// Deterministic gaussian init from a seed (Box-Muller over hash01).
    pub fn seeded(seed: u32, scale: f32) -> Self {
        let w = (0..GENOME).map(|i| gauss(seed, i as u32) * scale).collect();
        Mlp::new(w)
    }

    /// A deterministic, non-random starting doctrine for the AI Tiger.
    ///
    /// The network is still evolved — this merely gives generation zero one competent ancestor
    /// instead of asking pure noise to discover "turn toward the threat, close to a useful range,
    /// then pull the trigger" all at once. `Game::tiger_executor_obs` defines the matching inputs:
    /// range is slot 2, relative bearing sine is slot 3, and slot 13 is the constant bias.
    pub fn tiger_doctrine_seed() -> Self {
        let mut w = vec![0.0; GENOME];
        let hidden_bias = OBS * HID;
        let output_weights = OBS * HID + HID;
        let output_bias = output_weights + HID * ACT;

        // Hidden 0: range controller. Positive outside ~132 m, negative inside it.
        w[2] = 4.2;
        w[hidden_bias] = -1.85;
        // Hidden 1: turn toward the nearest T-34.
        w[OBS + 3] = 3.0;
        // Hidden 2: a permanently willing trigger. Travel lock, breech, crew cycle and gun lay
        // remain hard simulation gates, so this cannot cheat the physical tank.
        w[hidden_bias + 2] = 2.4;

        w[output_weights] = 1.15; // drive <- range controller
        w[output_weights + HID + 1] = 1.25; // steer <- relative bearing
        w[output_weights + HID * 2 + 2] = 1.6; // fire <- trigger intent
                                               // Keep the otherwise unused output biases explicit for layout readability.
        w[output_bias] = 0.0;
        w[output_bias + 1] = 0.0;
        w[output_bias + 2] = 0.0;
        Mlp::new(w)
    }

    /// One decision. `obs` may be wider than the net (a July T-34 net reads the first 14 slots of
    /// the 31-wide T-34 observation, which are the July inputs unchanged); narrower panics.
    pub fn forward(&self, obs: &[f32]) -> [f32; ACT] {
        let (n, hid) = (self.obs, self.hid);
        assert!(obs.len() >= n, "a {n}-input policy was handed a {}-wide observation", obs.len());
        let mut h = [0.0f32; T34_HID];
        let (wi, bi) = (0, n * hid);
        for j in 0..hid {
            let mut a = self.w[bi + j];
            for i in 0..n {
                a += self.w[wi + j * n + i] * obs[i];
            }
            h[j] = a.tanh();
        }
        let (wo, bo) = (n * hid + hid, n * hid + hid + hid * ACT);
        let mut out = [0.0f32; ACT];
        for k in 0..ACT {
            let mut a = self.w[bo + k];
            for j in 0..hid {
                a += self.w[wo + k * hid + j] * h[j];
            }
            out[k] = a.tanh();
        }
        out
    }

    /// Write the versioned weight artifact: magic, dims, genome id, fitness, raw f32 LE weights.
    pub fn save(&self, path: &str, genome_id: u32, fitness: f32) -> anyhow::Result<()> {
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::File::create(path)?;
        f.write_all(if self.obs == OBS { &MAGIC } else { &MAGIC_T34 })?;
        for v in [self.obs as u32, self.hid as u32, ACT as u32, genome_id] {
            f.write_all(&v.to_le_bytes())?;
        }
        f.write_all(&fitness.to_le_bytes())?;
        for v in &self.w {
            f.write_all(&v.to_le_bytes())?;
        }
        Ok(())
    }

    /// Load a July-width (`KSL1`, 14-input) artifact: the AI Tiger, the platoon wingmen. A `KSL2`
    /// T-34 net is refused here: its inputs mean nothing to those observations.
    pub fn load(path: &str) -> anyhow::Result<Mlp> {
        let net = Self::load_any(path)?;
        anyhow::ensure!(net.obs == OBS,
            "{path} is a {}-input T-34 policy (KSL2); this seat reads the {OBS}-input KSL1 observation", net.obs);
        Ok(net)
    }

    /// Load a T-34 policy: a `KSL2` net over the 31-wide observation, or a July `KSL1` net, which
    /// reads the first 14 slots (the July inputs, unchanged) and drives exactly as it did.
    pub fn load_t34(path: &str) -> anyhow::Result<Mlp> { Self::load_any(path) }

    /// Load a weight artifact of either format; errors loudly on a bad magic or on dims that do not
    /// match the magic's one shape (the runtime falls back to scripted).
    fn load_any(path: &str) -> anyhow::Result<Mlp> {
        // Runtime weights are shipped resources, but `resource::open` also passes absolute paths
        // through unchanged so trainer verification and user-selected artifacts keep working.
        let mut f = crate::resource::open(path)?;
        let mut head = [0u8; 4 + 4 * 4 + 4];
        f.read_exact(&mut head)?;
        let (width, hid) = if head[0..4] == MAGIC { (OBS, HID) } else if head[0..4] == MAGIC_T34 { (T34_OBS, T34_HID) } else {
            anyhow::bail!("bad weight-artifact magic in {path}: {:?} (KSL1 or KSL2 expected)", &head[0..4]);
        };
        let dim = |o: usize| u32::from_le_bytes(head[o..o + 4].try_into().unwrap()) as usize;
        anyhow::ensure!(
            (dim(4), dim(8), dim(12)) == (width, hid, ACT),
            "weight-artifact {path}: {} dims {}x{}x{} != the format's {width}x{hid}x{ACT}; refusing a mismatched genome",
            String::from_utf8_lossy(&head[0..4]),
            dim(4),
            dim(8),
            dim(12)
        );
        let mut bytes = vec![0u8; genome_len(width, hid) * 4];
        f.read_exact(&mut bytes)?;
        let w = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Ok(Mlp::new(w))
    }
}

/// The project's counter hash (game.rs/city.rs pattern), local so `ai` stays self-contained.
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(2_654_435_761);
    x ^= x >> 15;
    x = x.wrapping_mul(2_246_822_519);
    x ^= x >> 13;
    ((x >> 8) & 0xffff) as f32 / 65536.0
}

/// Deterministic N(0,1) draw for (seed, index) — Box-Muller over two hash01 samples.
pub fn gauss(seed: u32, i: u32) -> f32 {
    let u1 = hash01(seed.wrapping_mul(2_246_822_519) ^ i.wrapping_mul(0x9E37_79B9)).max(1e-6);
    let u2 = hash01(seed.wrapping_mul(2_654_435_761) ^ i.wrapping_mul(0x85EB_CA77));
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Mutate a parent genome into a child: per-weight gaussian steps, fixed sigma (the smoke-test
/// simplification of the doc's self-adaptive per-weight sigma — see the `sa_*` family below).
pub fn mutate(parent: &[f32], seed: u32, sigma: f32) -> Vec<f32> {
    parent
        .iter()
        .enumerate()
        .map(|(i, &w)| (w + gauss(seed, i as u32) * sigma).clamp(-4.0, 4.0))
        .collect()
}

// ── Self-adaptive ES (the doc §4 design): each weight carries its OWN step size, mutated
// multiplicatively before it is applied — the strategy parameters evolve with the solution.
// Extended genome layout: [weights; GENOME] ++ [sigmas; GENOME].

/// Extended genome length (weights + per-weight sigmas) of a July-width net.
pub const SA_GENOME: usize = GENOME * 2;
/// Learning rate for the log-sigma walk — the classic 1/sqrt(2·sqrt(n)).
const SA_TAU: f32 = 0.17;

/// Fresh extended genome: seeded gaussian weights + uniform initial sigma.
pub fn sa_seeded(seed: u32, scale: f32, sigma0: f32) -> Vec<f32> {
    sa_seeded_len(GENOME, seed, scale, sigma0)
}

/// `sa_seeded` for a genome of any length (the T-34 net's `T34_GENOME`).
pub fn sa_seeded_len(len: usize, seed: u32, scale: f32, sigma0: f32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..len).map(|i| gauss(seed, i as u32) * scale).collect();
    v.extend(std::iter::repeat(sigma0).take(len));
    v
}

/// Self-adaptive mutation: sigma_i *= exp(tau·N), then w_i += sigma_i'·N. Two independent draws
/// per weight; deterministic in (seed, i).
pub fn sa_mutate(parent: &[f32], seed: u32) -> Vec<f32> {
    assert!(parent.len() == SA_GENOME || parent.len() == 2 * T34_GENOME);
    let n = parent.len() / 2;
    let mut child = vec![0.0f32; parent.len()];
    for i in 0..n {
        let s =
            (parent[n + i] * (SA_TAU * gauss(seed, (2 * i) as u32)).exp()).clamp(0.005, 1.0);
        child[n + i] = s;
        child[i] = (parent[i] + s * gauss(seed, (2 * i + 1) as u32)).clamp(-4.0, 4.0);
    }
    child
}

/// The network weights of an extended genome (what ships; sigmas are trainer-only).
pub fn sa_weights(v: &[f32]) -> Vec<f32> {
    v[..v.len() / 2].to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_is_deterministic_and_bounded() {
        let net = Mlp::seeded(42, 0.5);
        let obs = [0.3f32; OBS];
        let a = net.forward(&obs);
        let b = net.forward(&obs);
        assert_eq!(a, b);
        assert!(a.iter().all(|v| v.abs() <= 1.0));
    }

    #[test]
    fn save_load_roundtrip_bitexact() {
        let net = Mlp::seeded(7, 0.5);
        let path = std::env::temp_dir().join("kessel_ai_roundtrip.bin");
        let path = path.to_str().unwrap();
        net.save(path, 7, 1.25).unwrap();
        let back = Mlp::load(path).unwrap();
        assert_eq!(net.w, back.w);
    }

    #[test]
    fn sa_mutation_reproducible_and_sane() {
        let p = sa_seeded(3, 0.5, 0.15);
        let a = sa_mutate(&p, 42);
        assert_eq!(a, sa_mutate(&p, 42));
        assert_ne!(a, sa_mutate(&p, 43));
        assert!(a[GENOME..].iter().all(|&s| (0.005..=1.0).contains(&s)));
        let _ = Mlp::new(sa_weights(&a)); // strips to a loadable net
    }

    #[test]
    fn widened_net_answers_exactly_as_the_july_net() {
        let july = Mlp::seeded(11, 0.7);
        let wide = july.widened();
        assert_eq!(wide.obs, T34_OBS);
        let mut obs = [0.0f32; T34_OBS];
        for (i, v) in obs.iter_mut().enumerate() { *v = ((i as f32) * 0.37).sin(); }
        assert_eq!(july.forward(&obs[..OBS]), wide.forward(&obs));
        // The new inputs carry no weight until evolution gives them some.
        obs[OBS..].iter_mut().for_each(|v| *v = 0.9);
        assert_eq!(july.forward(&obs[..OBS]), wide.forward(&obs));
    }

    #[test]
    fn formats_refuse_mismatched_genomes() {
        let dir = std::env::temp_dir();
        let wide_path = dir.join("kessel_ai_ksl2.bin");
        let wide_path = wide_path.to_str().unwrap();
        let wide = Mlp::seeded(5, 0.5).widened();
        wide.save(wide_path, 1, 0.0).unwrap();
        assert_eq!(Mlp::load_t34(wide_path).unwrap().w, wide.w);
        let refused = Mlp::load(wide_path).err().expect("a KSL2 net must not load into a 14-input seat");
        assert!(refused.to_string().contains("KSL2"), "{refused}");
        // A KSL1 header claiming the T-34 width is refused, not misread.
        let mut bytes = std::fs::read(wide_path).unwrap();
        bytes[0..4].copy_from_slice(b"KSL1");
        let forged = dir.join("kessel_ai_forged.bin");
        std::fs::write(&forged, &bytes).unwrap();
        let refused = Mlp::load_t34(forged.to_str().unwrap()).err().expect("dims must match the magic");
        assert!(refused.to_string().contains("mismatched genome"), "{refused}");
    }

    #[test]
    #[should_panic(expected = "31-input policy was handed a 14-wide observation")]
    fn a_wide_net_refuses_a_narrow_observation() {
        Mlp::seeded(3, 0.5).widened().forward(&[0.0; OBS]);
    }

    /// Open ground, the Tiger in sight: the reflex seed answers as the July net. A wall ahead
    /// throttles it back and turns it to the open side; stalled, it reverses; blind with a memory
    /// of the Tiger off to the right, it steers right and drives.
    #[test]
    fn reflex_seed_keeps_the_july_manner_in_the_open_and_avoids_walls() {
        let july = Mlp::seeded(21, 0.15); // unsaturated, so the reflexes show in the outputs
        let seed = Mlp::t34_reflex_seed(&july);
        let mut open = [0.0f32; T34_OBS];
        open[2] = 0.5; open[3] = 0.3; open[4] = 0.95; open[8] = 1.0; open[9] = 1.0; open[11] = 1.0;
        open[14] = 1.0; open[15] = 1.0; open[16] = 1.0; open[17] = 1.0; open[20] = 0.3; open[21] = 0.95;
        open[22] = 0.5; open[23] = 1.0;
        let (a, b) = (july.forward(&open[..OBS]), seed.forward(&open));
        for k in 0..ACT { assert!((a[k] - b[k]).abs() < 1e-3, "{a:?} vs {b:?}"); }
        let mut wall = open;
        wall[15] = 0.0; wall[14] = 0.0; wall[16] = 0.75;
        let w = seed.forward(&wall);
        assert!(w[0] < b[0] - 0.3 && w[1] > b[1] + 0.2, "wall {w:?} vs open {b:?}");
        // Stalled against something astern while reversing: drive out forward.
        let mut stalled = open;
        stalled[19] = 1.0; stalled[17] = 0.0;
        assert!(seed.forward(&stalled)[0] > b[0] + 0.3);
        // Stalled nose-first: back out.
        stalled[17] = 1.0; stalled[15] = 0.0;
        assert!(seed.forward(&stalled)[0] < b[0] - 0.3);
        let mut blind = [0.0f32; T34_OBS];
        blind[8] = -1.0; blind[9] = -1.0; blind[11] = 1.0;
        blind[14] = 1.0; blind[15] = 1.0; blind[16] = 1.0; blind[17] = 1.0; blind[23] = -1.0;
        let lost = seed.forward(&blind);
        blind[20] = 0.7; blind[21] = 0.7; blind[22] = 0.6; blind[23] = 1.0;
        let cue = seed.forward(&blind);
        assert!(cue[1] > lost[1] + 0.3 && cue[0] > lost[0] + 0.3, "cue {cue:?} vs none {lost:?}");
        // Blind, no memory, orders ahead and to the right: steer right, drive.
        blind[20] = 0.0; blind[21] = 0.0; blind[22] = 0.0; blind[23] = -1.0;
        blind[27] = 0.6; blind[28] = 0.8; blind[29] = 0.4; blind[30] = -1.0;
        let orders = seed.forward(&blind);
        assert!(orders[1] > lost[1] + 0.3 && orders[0] > lost[0] + 0.2, "orders {orders:?} vs none {lost:?}");
    }

    #[test]
    fn mutation_is_reproducible() {
        let p = Mlp::seeded(1, 0.5).w;
        assert_eq!(mutate(&p, 99, 0.15), mutate(&p, 99, 0.15));
        assert_ne!(mutate(&p, 99, 0.15), mutate(&p, 100, 0.15));
    }

    #[test]
    fn tiger_doctrine_closes_turns_and_fires() {
        let net = Mlp::tiger_doctrine_seed();
        let mut obs = [0.0; OBS];
        obs[2] = 0.8; // far
        obs[3] = 0.5; // target to the right
        obs[13] = 1.0;
        let far = net.forward(&obs);
        assert!(far[0] > 0.5);
        assert!(far[1] > 0.5);
        assert!(far[2] > 0.5);

        obs[2] = 0.2; // too close: back away
        let close = net.forward(&obs);
        assert!(close[0] < -0.5);
    }
}

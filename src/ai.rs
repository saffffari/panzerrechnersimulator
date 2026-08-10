//! Neuroevolution policy net — the M1 seam from docs/NEUROEVOLUTION.md, at pipeline-smoke-test
//! scale. A tiny fixed-topology MLP (`OBS → tanh(HID) → tanh(ACT)`) drives enemy maneuver +
//! fire-discipline; the gun lay, stealth gates, reload and ballistics all stay with the sim (the
//! net proposes, the rules dispose). Grows into `src/ai/{net,obs,roles,evolve}.rs` as M2-M4 land.
//!
//! Determinism contract (same as the rest of the game): NO `rand` — every "random" number is a
//! counter-seeded `hash01`, and mutation draws are Box-Muller over two hash01 samples. A genome is
//! fully reproduced by `(base_seed, generation, parent, child)`.

use std::io::{Read, Write};

/// Executor observation width — see `Game::executor_obs` for the layout.
pub const OBS: usize = 14;
/// Hidden width.
pub const HID: usize = 16;
/// Actions: [drive, steer, fire(>0)].
pub const ACT: usize = 3;
/// Total genome length: two dense layers + biases.
pub const GENOME: usize = OBS * HID + HID + HID * ACT + ACT;

/// Weight-artifact header magic + version ("KSL" + v1).
const MAGIC: [u8; 4] = *b"KSL1";

/// A `[OBS] → tanh(HID) → tanh(ACT)` MLP over a flat weight vector (layer-major, weights then
/// biases). Pure, no-alloc hot path aside from the output array.
#[derive(Clone)]
pub struct Mlp {
    pub w: Vec<f32>, // len == GENOME
}

impl Mlp {
    pub fn new(w: Vec<f32>) -> Self {
        assert_eq!(w.len(), GENOME, "genome length mismatch");
        Mlp { w }
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

    pub fn forward(&self, obs: &[f32; OBS]) -> [f32; ACT] {
        let mut h = [0.0f32; HID];
        let (wi, bi) = (0, OBS * HID);
        for j in 0..HID {
            let mut a = self.w[bi + j];
            for i in 0..OBS {
                a += self.w[wi + j * OBS + i] * obs[i];
            }
            h[j] = a.tanh();
        }
        let (wo, bo) = (OBS * HID + HID, OBS * HID + HID + HID * ACT);
        let mut out = [0.0f32; ACT];
        for k in 0..ACT {
            let mut a = self.w[bo + k];
            for j in 0..HID {
                a += self.w[wo + k * HID + j] * h[j];
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
        f.write_all(&MAGIC)?;
        for v in [OBS as u32, HID as u32, ACT as u32, genome_id] {
            f.write_all(&v.to_le_bytes())?;
        }
        f.write_all(&fitness.to_le_bytes())?;
        for v in &self.w {
            f.write_all(&v.to_le_bytes())?;
        }
        Ok(())
    }

    /// Load a weight artifact; errors on magic/dim mismatch (the runtime falls back to scripted).
    pub fn load(path: &str) -> anyhow::Result<Mlp> {
        let mut f = std::fs::File::open(path)?;
        let mut head = [0u8; 4 + 4 * 4 + 4];
        f.read_exact(&mut head)?;
        anyhow::ensure!(head[0..4] == MAGIC, "bad weight-artifact magic in {path}");
        let dim = |o: usize| u32::from_le_bytes(head[o..o + 4].try_into().unwrap()) as usize;
        anyhow::ensure!(
            (dim(4), dim(8), dim(12)) == (OBS, HID, ACT),
            "weight-artifact dims {}x{}x{} != compiled {OBS}x{HID}x{ACT}",
            dim(4),
            dim(8),
            dim(12)
        );
        let mut bytes = vec![0u8; GENOME * 4];
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

/// Extended genome length (weights + per-weight sigmas).
pub const SA_GENOME: usize = GENOME * 2;
/// Learning rate for the log-sigma walk — the classic 1/sqrt(2·sqrt(n)).
const SA_TAU: f32 = 0.17;

/// Fresh extended genome: seeded gaussian weights + uniform initial sigma.
pub fn sa_seeded(seed: u32, scale: f32, sigma0: f32) -> Vec<f32> {
    let mut v: Vec<f32> = (0..GENOME).map(|i| gauss(seed, i as u32) * scale).collect();
    v.extend(std::iter::repeat(sigma0).take(GENOME));
    v
}

/// Self-adaptive mutation: sigma_i *= exp(tau·N), then w_i += sigma_i'·N. Two independent draws
/// per weight; deterministic in (seed, i).
pub fn sa_mutate(parent: &[f32], seed: u32) -> Vec<f32> {
    assert_eq!(parent.len(), SA_GENOME);
    let mut child = vec![0.0f32; SA_GENOME];
    for i in 0..GENOME {
        let s =
            (parent[GENOME + i] * (SA_TAU * gauss(seed, (2 * i) as u32)).exp()).clamp(0.005, 1.0);
        child[GENOME + i] = s;
        child[i] = (parent[i] + s * gauss(seed, (2 * i + 1) as u32)).clamp(-4.0, 4.0);
    }
    child
}

/// The network weights of an extended genome (what ships; sigmas are trainer-only).
pub fn sa_weights(v: &[f32]) -> Vec<f32> {
    v[..GENOME].to_vec()
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

//! Read-only checkpoint transfer evaluation under the currently compiled combat rules.
//! This never calls mutation, curriculum initialization, candidate saving, or promotion.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvaluationSeedSet {
    /// New declared seed namespace, separate from the originating run's validation/test.
    Transfer,
    /// Exact old sealed-test replay; these seeds are no longer unseen after that run.
    SealedReplay,
}
#[derive(Clone, Debug)]
pub struct EvaluationConfig {
    pub output: PathBuf,
    pub candidate: PathBuf,
    pub baseline: PathBuf,
    pub original_run: PathBuf,
    pub seed_set: EvaluationSeedSet,
}

pub(super) fn checkpoint_hashes(directory: &Path) -> anyhow::Result<serde_json::Value> {
    let mut result = serde_json::Map::new();
    for name in [
        "german_rifle.ksl2",
        "soviet_rifle.ksl2",
        "tank_soft_targets.json",
    ] {
        let bytes = std::fs::read(directory.join(name))?;
        let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ *b as u64).wrapping_mul(0x100000001b3)
        });
        result.insert(
            name.into(),
            serde_json::json!({"bytes":bytes.len(),"fnv1a64":format!("{hash:016x}")}),
        );
    }
    Ok(result.into())
}
pub(super) fn load_checkpoints(directory: &Path) -> anyhow::Result<Policies> {
    let load_foot = |name: &str, role| -> anyhow::Result<FootPolicy> {
        let path = directory.join(name);
        let path = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF8 checkpoint path"))?;
        Ok(FootPolicy::load(path, role)?.0)
    };
    Ok(Policies {
        german: Some(load_foot("german_rifle.ksl2", PolicyRole::GermanRifle)?),
        soviet: Some(load_foot("soviet_rifle.ksl2", PolicyRole::SovietRifle)?),
        tank: Some(tank_ai::Policy::load(
            &directory.join("tank_soft_targets.json"),
        )?),
    })
}
fn evaluation_seeds(run: &serde_json::Value, set: EvaluationSeedSet) -> anyhow::Result<Vec<u64>> {
    let read_seeds = |field: &str| -> anyhow::Result<Vec<u64>> {
        run[field]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing {field}"))?
            .iter()
            .map(|s| {
                s.as_u64()
                    .ok_or_else(|| anyhow::anyhow!("invalid {field} seed"))
            })
            .collect()
    };
    let sealed = read_seeds("sealed_test_seeds")?;
    anyhow::ensure!(
        !sealed.is_empty() && sealed.len() <= 1024,
        "sealed seed count must be 1..1024"
    );
    if set == EvaluationSeedSet::SealedReplay {
        return Ok(sealed);
    }
    let validation = read_seeds("validation_seeds")?;
    let seed = run["seed"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("missing original seed"))?;
    let selected: Vec<_> = (0..sealed.len())
        .map(|i| tank_ai::hash(seed ^ 0x5452414e53464552 ^ i as u64))
        .collect();
    anyhow::ensure!(
        selected
            .iter()
            .all(|s| !sealed.contains(s) && !validation.contains(s)),
        "transfer seeds overlap prior held-out sets"
    );
    Ok(selected)
}

pub fn evaluate_checkpoints(config: EvaluationConfig) -> anyhow::Result<serde_json::Value> {
    let candidate_path = std::fs::canonicalize(&config.candidate)?;
    let baseline_path = std::fs::canonicalize(&config.baseline)?;
    anyhow::ensure!(
        !config.output.exists(),
        "evaluation output already exists; preserve it and choose a new directory"
    );
    let output_parent = std::fs::canonicalize(
        config
            .output
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    anyhow::ensure!(
        !output_parent.starts_with(&candidate_path) && !output_parent.starts_with(&baseline_path),
        "evaluation output cannot be inside either checkpoint directory"
    );
    let run_bytes = std::fs::read(&config.original_run)?;
    let run: serde_json::Value = serde_json::from_slice(&run_bytes)?;
    anyhow::ensure!(
        run["schema"] == "kessel.combat_training_run.v1",
        "unsupported original run schema"
    );
    let seconds = run["seconds"]
        .as_f64()
        .ok_or_else(|| anyhow::anyhow!("missing episode duration"))? as f32;
    anyhow::ensure!(
        seconds.is_finite() && (5.0..=180.0).contains(&seconds),
        "episode seconds must be 5..180"
    );
    let seeds = evaluation_seeds(&run, config.seed_set)?;
    let candidate_hashes = checkpoint_hashes(&candidate_path)?;
    let baseline_hashes = checkpoint_hashes(&baseline_path)?;
    let candidate = load_checkpoints(&candidate_path)?;
    let baseline = load_checkpoints(&baseline_path)?;
    let opponents = vec![
        Policies {
            german: None,
            soviet: None,
            tank: None,
        },
        baseline.clone(),
    ];
    let start = Instant::now();
    let mut tracks = Vec::new();
    for track in [Track::German, Track::Soviet, Track::Tank] {
        let (candidate_score, candidate_metrics) =
            evaluate(&candidate, track, &seeds, seconds, &opponents);
        let (baseline_score, baseline_metrics) =
            evaluate(&baseline, track, &seeds, seconds, &opponents);
        tracks.push(serde_json::json!({"track":track.label(),"candidate_score":candidate_score,"baseline_score":baseline_score,
            "candidate":candidate_metrics.iter().map(|m|m.json()).collect::<Vec<_>>(),
            "baseline":baseline_metrics.iter().map(|m|m.json()).collect::<Vec<_>>()}));
    }
    anyhow::ensure!(
        candidate_hashes == checkpoint_hashes(&candidate_path)?
            && baseline_hashes == checkpoint_hashes(&baseline_path)?,
        "input checkpoint changed during evaluation"
    );
    anyhow::ensure!(
        run_bytes == std::fs::read(&config.original_run)?,
        "original run receipt changed during evaluation"
    );
    let report = serde_json::json!({
        "schema":"kessel.combat_checkpoint_evaluation.v1","backend":"single_cpu_shared_combat","elapsed_seconds":start.elapsed().as_secs_f32(),
        "promoted_to_shipping":false,"weights_modified":false,"current_executable":std::env::current_exe()?,
        "source_hashes":source_hashes(),"original_source_hashes":run["source_hashes"],"original_run":std::fs::canonicalize(&config.original_run)?,
        "candidate_directory":candidate_path,"baseline_directory":baseline_path,"candidate_hashes":candidate_hashes,"baseline_hashes":baseline_hashes,
        "seconds":seconds,"seed_set":if config.seed_set==EvaluationSeedSet::Transfer {"transfer_0x5452414e53464552"}else{"previous_sealed_test_replay"},"seeds":seeds,
        "episode_context":seeds.iter().enumerate().map(|(i,seed)|serde_json::json!({"seed":seed,"opponent":if i%2==0{"hand_coded_doctrine"}else{"fixed_initial_policies"},"tank_gun":if seed&1==0{"F34_OF350_review_candidate"}else{"M10T_OF530_legacy"}})).collect::<Vec<_>>(),
        "limitations":["read-only CPU transfer evaluation; no learning or checkpoint selection","same fixed opponent set as original evaluation; no full live campaign validation","flat procedural cover arena; actual runtime collision and target perception require separate checks","current compiled stance/gun rules may differ from original checkpoint training","original sealed replay is repeated evidence, not a new unseen test"],
        "tracks":tracks
    });
    std::fs::create_dir(&config.output)?;
    std::fs::write(
        config.output.join("evaluation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transfer_seeds_are_deterministic_and_distinct_from_validation_and_old_test() {
        let run = serde_json::json!({"seed":20260905u64,"validation_seeds":[1,2,3],"sealed_test_seeds":[4,5,6,7]});
        let a = evaluation_seeds(&run, EvaluationSeedSet::Transfer).unwrap();
        assert_eq!(
            a,
            evaluation_seeds(&run, EvaluationSeedSet::Transfer).unwrap()
        );
        assert!(a.iter().all(|s| ![1, 2, 3, 4, 5, 6, 7].contains(s)));
        assert_eq!(
            evaluation_seeds(&run, EvaluationSeedSet::SealedReplay).unwrap(),
            [4, 5, 6, 7]
        );
    }
    #[test]
    fn evaluation_uses_exact_checkpoint_inputs_and_never_overwrites_evidence() {
        let dir = std::env::temp_dir().join(format!("kessel-transfer-eval-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir(&dir).unwrap();
        let policies = Policies {
            german: Some(
                FootPolicy::new(PolicyRole::GermanRifle, vec![0.0; foot_ai::WEIGHT_COUNT]).unwrap(),
            ),
            soviet: Some(
                FootPolicy::new(PolicyRole::SovietRifle, vec![0.0; foot_ai::WEIGHT_COUNT]).unwrap(),
            ),
            tank: Some(tank_ai::Policy::doctrine()),
        };
        let input = dir.join("weights");
        save_candidate(&input, &policies, 0, [0.0; 3]).unwrap();
        let hashes = checkpoint_hashes(&input).unwrap();
        let run = dir.join("run.json");
        std::fs::write(&run, serde_json::json!({"schema":"kessel.combat_training_run.v1","seed":42,"seconds":5.0,"validation_seeds":[1,2],"sealed_test_seeds":[3,4]}).to_string()).unwrap();
        let config = EvaluationConfig {
            output: dir.join("result"),
            candidate: input.clone(),
            baseline: input.clone(),
            original_run: run,
            seed_set: EvaluationSeedSet::Transfer,
        };
        let report = evaluate_checkpoints(config.clone()).unwrap();
        assert_eq!(report["promoted_to_shipping"], false);
        for track in report["tracks"].as_array().unwrap() {
            assert_eq!(track["candidate"], track["baseline"]);
        }
        assert_eq!(hashes, checkpoint_hashes(&input).unwrap());
        let receipt = std::fs::read(config.output.join("evaluation.json")).unwrap();
        assert!(evaluate_checkpoints(config.clone()).is_err());
        assert_eq!(
            receipt,
            std::fs::read(config.output.join("evaluation.json")).unwrap()
        );
        let nested = EvaluationConfig {
            output: input.join("must_not_write"),
            ..config
        };
        assert!(evaluate_checkpoints(nested).is_err());
        assert!(!input.join("must_not_write").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

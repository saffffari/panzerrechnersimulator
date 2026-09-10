//! Explicit imitation initialization from real shared-combat observations. This is not the
//! competitive optimizer: its saved candidates still need independent combat evolution/exams.
use super::*;

fn reservoir_push<T>(samples:&mut Vec<T>,seen:&mut u64,capacity:usize,seed:u64,item:T) {
    *seen+=1;
    if samples.len()<capacity {samples.push(item);return;}
    let choice=(tank_ai::hash(seed^*seen)%*seen) as usize;
    if choice<capacity {samples[choice]=item;}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn competitive_training_preserves_nonempty_output_even_without_a_run_receipt() {
        let dir=std::env::temp_dir().join(format!("kessel-training-preserve-{}-{}",
            std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(dir.join("candidate")).unwrap();
        let file=dir.join("candidate/german_rifle.ksl2");
        std::fs::write(&file,b"protected checkpoint").unwrap();
        let result=train(Config{output:dir.clone(),initial:None,generations:1,population:4,
            episodes:2,seconds:5.0,workers:1,minutes:1.0,seed:1});
        assert!(result.is_err());
        assert_eq!(std::fs::read(&file).unwrap(),b"protected checkpoint");
        assert!(!dir.join("run.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn capped_curriculum_retains_all_actor_lanes_deterministically() {
        let mut samples=Vec::new();let mut seen=0;
        let mut same=Vec::new();let mut same_seen=0;
        for i in 0..80_000 {
            reservoir_push(&mut samples,&mut seen,4096,20260905,i%4);
            reservoir_push(&mut same,&mut same_seen,4096,20260905,i%4);
        }
        assert_eq!(samples,same);assert_eq!(seen,80_000);assert_eq!(samples.len(),4096);
        for actor in 0..4 {
            let count=samples.iter().filter(|id|**id==actor).count();
            assert!((750..=1300).contains(&count),"actor{actor} retained {count}");
        }
    }
    #[test]
    fn collecting_decision_observations_does_not_change_combat_or_perception() {
        let policies=Policies{german:None,soviet:None,tank:None};
        let mut ordinary=Episode::new(94,false);
        let mut recorded=ordinary.clone();
        let mut count=0;
        for _ in 0..120 {
            ordinary.step(&policies,0.05);
            recorded.step_observed(&policies,0.05,|obs| {
                count+=1;
                assert!(obs.values().iter().all(|v|v.is_finite()));
                assert!(matches!(obs.role(),PolicyRole::GermanRifle|PolicyRole::SovietRifle));
            });
            assert_eq!(ordinary.metrics.json(),recorded.metrics.json());
            for (a,b) in ordinary.actors.iter().zip(&recorded.actors) {
                assert_eq!(a.pos,b.pos); assert_eq!(a.hp,b.hp);
            }
        }
        assert!(count>0);
    }
}

pub struct BootstrapConfig {
    pub output: PathBuf,
    pub seed: u64,
    pub rounds: usize,
    pub episodes: usize,
    pub epochs: usize,
}

fn exam(p: &Policies, seeds: &[u64]) -> serde_json::Value {
    let mut result = Vec::new();
    for track in [Track::German, Track::Soviet] {
        let mut paired = p.clone();
        if track == Track::German { paired.soviet = None; } else { paired.german = None; }
        let metrics: Vec<_> = seeds.iter().map(|seed| run_episode(*seed,45.0,&paired,false)).collect();
        let team = if track == Track::German {Team::German} else {Team::Soviet};
        result.push(serde_json::json!({"track":track.label(),"opponent":"scripted combat doctrine",
            "score":metrics.iter().map(|m|m.score(track)).sum::<f32>()/metrics.len() as f32,
            "wins":metrics.iter().filter(|m|m.winner==Some(team)).count(),
            "losses":metrics.iter().filter(|m|m.winner.is_some()&&m.winner!=Some(team)).count(),
            "episodes":metrics.iter().map(|m|m.json()).collect::<Vec<_>>() }));
    }
    result.into()
}

pub fn bootstrap_combat(config: BootstrapConfig) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(!config.output.exists(),"bootstrap output already exists; preserve previous run");
    anyhow::ensure!((1..=12).contains(&config.rounds) && (2..=256).contains(&config.episodes)
        && (1..=100).contains(&config.epochs),"invalid bootstrap workload");
    std::fs::create_dir_all(&config.output)?;
    let mut policies=Policies {german:Some(seed_policy(PolicyRole::GermanRifle)),
        soviet:Some(seed_policy(PolicyRole::SovietRifle)),tank:Some(tank_ai::Policy::doctrine())};
    let baseline=policies.clone();
    let teacher=Policies {german:None,soviet:None,tank:None};
    let validation:Vec<_>=(0..32).map(|i|tank_ai::hash(config.seed^0x42535456414c^i)).collect();
    let sealed:Vec<_>=(0..64).map(|i|tank_ai::hash(config.seed^0x42535454455354^i)).collect();
    let mut history=Vec::new();
    let mut samples:[Vec<(foot_ai::FootObservation,FootAction)>;2]=[Vec::new(),Vec::new()];
    let mut seen=[0u64;2];
    let start=Instant::now();
    save_candidate(&config.output.join("initial"),&baseline,0,[0.0;3])?;
    for round in 0..config.rounds {
        for episode in 0..config.episodes {
            let seed=tank_ai::hash(config.seed^0x425354545241494e^((round as u64)<<32)^episode as u64);
            let mut ep=Episode::new(seed,false);
            // Teacher and actual student trajectories expose different failure states. Labels
            // always see exactly the actor's ordinary observation, never hidden enemy state.
            let mut controller=if episode%3==0 {teacher.clone()} else {policies.clone()};
            if episode%3==1 {controller.soviet=None;} else if episode%3==2 {controller.german=None;}
            for _ in 0..600 {
                ep.step_observed(&controller,0.05,|obs| {
                    let i=usize::from(obs.role()==PolicyRole::SovietRifle);
                    reservoir_push(&mut samples[i],&mut seen[i],60_000,
                        config.seed^0x5245534552564f49^i as u64,(obs,foot_ai::micro_expert(&obs)));
                });
                if ep.finished() {break;}
            }
        }
        let german_loss=policies.german.as_mut().unwrap().fit_combat_observations(&samples[0],config.epochs,0.025)?;
        let soviet_loss=policies.soviet.as_mut().unwrap().fit_combat_observations(&samples[1],config.epochs,0.025)?;
        let check=exam(&policies,&validation);
        history.push(serde_json::json!({"round":round+1,"samples":[samples[0].len(),samples[1].len()],"observations_seen":seen,
            "imitation_mse":[german_loss,soviet_loss],"validation_combat":check}));
        save_candidate(&config.output.join(format!("round-{:02}",round+1)),&policies,(round+1)as u32,[0.0;3])?;
        eprintln!("[combat-bootstrap] round{} observations{:?} imitation_mse{:.5}/{:.5}",round+1,
            [samples[0].len(),samples[1].len()],german_loss,soviet_loss);
    }
    // The final round is fixed by configuration, not selected against the sealed bank.
    save_candidate(&config.output.join("candidate"),&policies,config.rounds as u32,[0.0;3])?;
    let report=serde_json::json!({"schema":"kessel.combat-imitation-bootstrap.v1",
        "seed":config.seed,"rounds":config.rounds,"episodes_per_round":config.episodes,"epochs":config.epochs,
        "source_hashes":source_hashes(),"validation_seeds":validation,"sealed_test_seeds":sealed,
        "training_seed_namespace":"0x425354545241494e","sampling":"seeded per-faction reservoir; capacity60000 each","history":history,
        "baseline_sealed":exam(&baseline,&sealed),"candidate_sealed":exam(&policies,&sealed),
        "elapsed_seconds":start.elapsed().as_secs_f64(),"promoted_to_shipping":false,
        "limits":["imitation preparation only; not competitive combat training",
        "flat bounded arena; live Game transfer still required","tank weights retain explicit doctrine unchanged",
        "checkpoint fitness header is zero because imitation loss is not combat fitness"]});
    std::fs::write(config.output.join("bootstrap.json"),serde_json::to_vec_pretty(&report)?)?;
    Ok(report)
}

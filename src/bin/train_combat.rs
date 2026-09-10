//! Renderer-free competitive combat trainer. Example:
//! train_combat artifacts/astra-combat-training-20260905-smoke --minutes 1 --generations 6 --population 8 --episodes 2 --seconds 10 --workers 8
#[path = "../anti_infantry_ai.rs"]
mod anti_infantry_ai;
#[path = "../ballistics.rs"]
mod ballistics;
#[path = "../combat_training.rs"]
mod combat_training;
#[path = "../foot_ai.rs"]
mod foot_ai;
#[path = "../infantry_combat.rs"]
mod infantry_combat;
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str)==Some("bootstrap") {
        anyhow::ensure!(args.len()>=2 && (args.len()-2)%2==0,
            "usage: train_combat bootstrap OUTPUT [--seed N --rounds N --episodes N --epochs N]");
        let mut config=combat_training::BootstrapConfig {output:args[1].clone().into(),
            seed:20260905,rounds:4,episodes:32,epochs:12};
        for pair in args[2..].chunks_exact(2) {
            match pair[0].as_str() {
                "--seed"=>config.seed=pair[1].parse()?,
                "--rounds"=>config.rounds=pair[1].parse()?,
                "--episodes"=>config.episodes=pair[1].parse()?,
                "--epochs"=>config.epochs=pair[1].parse()?,
                other=>anyhow::bail!("unknown bootstrap option {other}")
            }
        }
        let report=combat_training::bootstrap_combat(config)?;
        println!("{}",serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("evaluate") {
        anyhow::ensure!(args.len() >= 2 && (args.len() - 2) % 2 == 0,
            "usage: train_combat evaluate OUTPUT --candidate DIR --baseline DIR --run RUN_JSON [--seed-set transfer|sealed-replay]");
        let mut candidate = None;
        let mut baseline = None;
        let mut original_run = None;
        let mut seed_set = combat_training::EvaluationSeedSet::Transfer;
        for pair in args[2..].chunks_exact(2) {
            match pair[0].as_str() {
                "--candidate" => candidate = Some(pair[1].clone().into()),
                "--baseline" => baseline = Some(pair[1].clone().into()),
                "--run" => original_run = Some(pair[1].clone().into()),
                "--seed-set" => {
                    seed_set = match pair[1].as_str() {
                        "transfer" => combat_training::EvaluationSeedSet::Transfer,
                        "sealed-replay" => combat_training::EvaluationSeedSet::SealedReplay,
                        value => anyhow::bail!("unsupported evaluation seed set {value}"),
                    }
                }
                other => anyhow::bail!("unknown evaluation option {other}"),
            }
        }
        let report = combat_training::evaluate_checkpoints(combat_training::EvaluationConfig {
            output: args[1].clone().into(),
            candidate: candidate.ok_or_else(|| anyhow::anyhow!("--candidate DIR required"))?,
            baseline: baseline.ok_or_else(|| anyhow::anyhow!("--baseline DIR required"))?,
            original_run: original_run.ok_or_else(|| anyhow::anyhow!("--run RUN_JSON required"))?,
            seed_set,
        })?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }
    anyhow::ensure!(!args.is_empty(),"usage: train_combat OUTPUT --minutes N --generations N --population N --episodes N --seconds N --workers N --seed N");
    let mut config = combat_training::Config {
        output: args[0].clone().into(),
        initial: None,
        generations: 10000,
        population: 32,
        episodes: 8,
        seconds: 45.0,
        workers: 8,
        minutes: 480.0,
        seed: 20260905,
    };
    anyhow::ensure!((args.len() - 1) % 2 == 0, "options require values");
    for pair in args[1..].chunks_exact(2) {
        match pair[0].as_str() {
            "--initial" => config.initial=Some(pair[1].clone().into()),
            "--minutes" => config.minutes = pair[1].parse()?,
            "--generations" => config.generations = pair[1].parse()?,
            "--population" => config.population = pair[1].parse()?,
            "--episodes" => config.episodes = pair[1].parse()?,
            "--seconds" => config.seconds = pair[1].parse()?,
            "--workers" => config.workers = pair[1].parse()?,
            "--seed" => config.seed = pair[1].parse()?,
            other => anyhow::bail!("unknown option {other}"),
        }
    }
    let report = combat_training::train(config)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

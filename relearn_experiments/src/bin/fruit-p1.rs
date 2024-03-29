use rand::SeedableRng;
use relearn::agents::{ActorMode, Agent, BuildAgent};
use relearn::envs::{
    BuildEnv, Environment, FirstPlayerView, FruitGame, VisibleStepLimit, WithVisibleStepLimit,
};
use relearn::logging::DisplayLogger;
use relearn::simulation::{train_parallel, SimSeed, StepsIter, TrainParallelConfig};
use relearn::torch::{
    agents::{critics::ValuesOptConfig, policies::PpoConfig, ActorCriticConfig},
    modules::GruMlpConfig,
};
use relearn::Prng;
use tch::Device;

fn main() {
    let env_config = WithVisibleStepLimit::new(
        FirstPlayerView::new(FruitGame::<5, 5, 5, 5>::default()),
        VisibleStepLimit::new(50),
    );

    let agent_config: ActorCriticConfig<PpoConfig<GruMlpConfig>, ValuesOptConfig<GruMlpConfig>> =
        ActorCriticConfig {
            device: Device::cuda_if_available(),
            ..Default::default()
        };
    let training_config = TrainParallelConfig {
        num_periods: 200,
        num_threads: num_cpus::get(),
        min_worker_steps: 10,
    };
    let mut rng = Prng::seed_from_u64(0);
    let env = env_config.build_env(&mut rng).unwrap();
    let mut agent = agent_config.build_agent(&env, &mut rng).unwrap();
    let mut logger: DisplayLogger = DisplayLogger::default();

    {
        let summary = env
            .run(&agent.actor(ActorMode::Evaluation), SimSeed::Root(0), ())
            .take(10_000)
            .summarize();
        println!("Initial Stats\n{}", summary);
    }

    train_parallel(
        &mut agent,
        &env,
        &training_config,
        &mut Prng::from_rng(&mut rng).unwrap(),
        &mut rng,
        &mut logger,
    );

    let summary = env
        .run(&agent.actor(ActorMode::Evaluation), SimSeed::Root(0), ())
        .take(10_000)
        .summarize();
    println!("\nFinal Stats\n{}", summary);
}

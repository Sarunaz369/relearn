use clap::{crate_authors, crate_description, crate_version, Clap};
use rust_rl::loggers::CLILogger;
use rust_rl::simulator::{AgentDef, EnvDef};
use std::convert::From;
use std::error::Error;
use std::time::Duration;

#[derive(Clap, Debug)]
#[clap(
    version = crate_version!(),
    author = crate_authors!(),
    about = crate_description!(),
)]
pub struct Opts {
    #[clap(long, default_value = "1")]
    /// Random seed for the experiment
    pub seed: u64,
    // Environment args
    #[clap(arg_enum)]
    /// Environment name
    environment: Env,

    #[clap(long, default_value = "2")]
    /// Number of arms for some bandit environments
    num_arms: usize,

    // Agent args
    #[clap(arg_enum)]
    /// Agent name
    agent: Agent,

    #[clap(long, default_value = "0.2")]
    /// Agent exploration rate
    exploration_rate: f32,

    // Experiment args
    #[clap(long)]
    /// Maximum number of experiment steps
    max_steps: Option<u64>,
}

#[derive(Clap, Debug)]
pub enum Env {
    SimpleBernoulliBandit,
    BernoulliBandit,
}

impl From<&Opts> for EnvDef {
    fn from(opts: &Opts) -> Self {
        match opts.environment {
            Env::SimpleBernoulliBandit => EnvDef::SimpleBernoulliBandit,
            Env::BernoulliBandit => EnvDef::BernoulliBandit {
                num_arms: opts.num_arms,
            },
        }
    }
}

#[derive(Clap, Debug)]
pub enum Agent {
    Random,
    TabularQLearning,
}

impl From<&Opts> for AgentDef {
    fn from(opts: &Opts) -> Self {
        match opts.agent {
            Agent::Random => AgentDef::Random,
            Agent::TabularQLearning => AgentDef::TabularQLearning {
                exploration_rate: opts.exploration_rate,
            },
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let opts: Opts = Opts::parse();
    println!("{:?}", opts);
    let env_def = EnvDef::from(&opts);
    println!("Environment: {:?}", env_def);
    let agent_def = AgentDef::from(&opts);
    println!("Agent: {:?}", agent_def);

    let logger = CLILogger::new(Duration::from_millis(1000), true);
    let mut simulator = env_def.make_simulator(agent_def, opts.seed, logger)?;
    simulator.run(opts.max_steps);
    Ok(())
}

use super::{Actor, Agent, BuildAgent, BuildAgentError, OffPolicyAgent, SetActorMode, Step};
use crate::envs::EnvStructure;
use crate::logging::TimeSeriesLogger;
use crate::spaces::SampleSpace;
use rand::rngs::StdRng;
use rand::SeedableRng;
use std::fmt;

/// Configuration setttings for a random agent.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RandomAgentConfig;

impl RandomAgentConfig {
    pub const fn new() -> Self {
        Self
    }
}

impl<E> BuildAgent<E> for RandomAgentConfig
where
    E: EnvStructure + ?Sized,
    <E as EnvStructure>::ActionSpace: SampleSpace,
{
    type Agent = RandomAgent<E::ActionSpace>;

    fn build_agent(&self, env: &E, seed: u64) -> Result<Self::Agent, BuildAgentError> {
        Ok(RandomAgent::new(env.action_space(), seed))
    }
}

/// An agent that always acts randomly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomAgent<AS> {
    action_space: AS,
    rng: StdRng,
}

impl<AS> RandomAgent<AS> {
    pub fn new(action_space: AS, seed: u64) -> Self {
        Self {
            action_space,
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

impl<O, AS: SampleSpace> Actor<O, AS::Element> for RandomAgent<AS> {
    fn act(&mut self, _observation: &O, _new_episode: bool) -> AS::Element {
        self.action_space.sample(&mut self.rng)
    }
}

impl<O, AS: SampleSpace> Agent<O, AS::Element> for RandomAgent<AS> {
    fn update(&mut self, _step: Step<O, AS::Element>, _logger: &mut dyn TimeSeriesLogger) {}
}

impl<AS> OffPolicyAgent for RandomAgent<AS> {}

impl<AS: fmt::Display> fmt::Display for RandomAgent<AS> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RandomAgent({})", self.action_space)
    }
}

impl<AS> SetActorMode for RandomAgent<AS> {}

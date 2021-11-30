//! Meta agents
use super::{Actor, Agent, BuildAgent, BuildAgentError, FullStep, SetActorMode};
use crate::envs::{
    EnvStructure, InnerEnvStructure, MetaObservation, MetaObservationSpace, StoredEnvStructure,
    Successor,
};
use crate::logging::TimeSeriesLogger;
use crate::spaces::{NonEmptySpace, Space};
use rand::{rngs::StdRng, Rng, SeedableRng};

/// Configuration for a [`ResettingMetaAgent`].
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ResettingMetaAgentConfig<T> {
    pub agent_config: T,
}

impl<T> ResettingMetaAgentConfig<T> {
    pub const fn new(agent_config: T) -> Self {
        Self { agent_config }
    }
}

impl<T, OS, AS> BuildAgent<MetaObservationSpace<OS, AS>, AS> for ResettingMetaAgentConfig<T>
where
    T: BuildAgent<OS, AS> + Clone,
    OS: Space + Clone,
    AS: NonEmptySpace + Clone,
{
    type Agent = ResettingMetaAgent<T, OS, AS>;

    fn build_agent(
        &self,
        env: &dyn EnvStructure<ObservationSpace = MetaObservationSpace<OS, AS>, ActionSpace = AS>,
        seed: u64,
    ) -> Result<Self::Agent, BuildAgentError> {
        let inner_env_structure = StoredEnvStructure::from(&InnerEnvStructure::new(env));
        ResettingMetaAgent::new(self.agent_config.clone(), inner_env_structure, seed)
    }
}

/// Lifts a regular agent to act on a meta environment by resetting between each trial.
pub struct ResettingMetaAgent<AC, OS, AS>
where
    AC: BuildAgent<OS, AS>,
    OS: Space + Clone,
    AS: Space + Clone,
{
    inner_agent_config: AC,
    inner_env_structure: StoredEnvStructure<OS, AS>,
    rng: StdRng,
    agent: AC::Agent,
    prev_observation: Option<OS::Element>,
    prev_episode_done: bool,
}

impl<AC, OS, AS> ResettingMetaAgent<AC, OS, AS>
where
    AC: BuildAgent<OS, AS>,
    OS: Space + Clone,
    AS: Space + Clone,
{
    /// Initialize a new resetting meta agent.
    ///
    /// # Args
    /// * `inner_agent_config` - A builder for the inner agent.
    /// * `inner_env_structure` - The inner environment structure.
    /// * `seed` - Seeds the internal pseudo random state.
    pub fn new(
        inner_agent_config: AC,
        inner_env_structure: StoredEnvStructure<OS, AS>,
        seed: u64,
    ) -> Result<Self, BuildAgentError> {
        let mut rng = StdRng::seed_from_u64(seed);
        let agent = inner_agent_config.build_agent(&inner_env_structure, rng.gen())?;
        Ok(Self {
            inner_agent_config,
            inner_env_structure,
            rng,
            agent,
            prev_observation: None,
            prev_episode_done: true,
        })
    }
}

impl<AC, OS, AS> Actor<<MetaObservationSpace<OS, AS> as Space>::Element, AS::Element>
    for ResettingMetaAgent<AC, OS, AS>
where
    AC: BuildAgent<OS, AS>,
    OS: Space + Clone,
    AS: NonEmptySpace + Clone,
{
    fn act(
        &mut self,
        obs: &MetaObservation<OS::Element, AS::Element>,
        new_episode: bool,
    ) -> AS::Element {
        if new_episode {
            // Reset the agent
            self.agent = self
                .inner_agent_config
                .build_agent(&self.inner_env_structure, self.rng.gen())
                .expect("Failed to build inner agent");
            self.prev_observation = None;
            self.prev_episode_done = true;
        } else if let Some(ref step_obs) = &obs.prev_step {
            // Update the agent based on the most recent step result
            // Only relevant if the agent has not been reset.
            let step_next = match (obs.inner_observation.as_ref().cloned(), obs.episode_done) {
                (Some(obs), false) => Successor::Continue(obs),
                (Some(obs), true) => Successor::Interrupt(obs),
                (None, true) => Successor::Terminate,
                (None, false) => panic!("must provide an observation if the episode continues"),
            };

            let step = FullStep {
                observation: self.prev_observation.take().expect(
                    "Meta observation follows a previous step but no previous observation stored",
                ),
                action: step_obs.action.clone(),
                reward: step_obs.reward,
                next: step_next,
            };
            self.agent.update(step, &mut ());
        }

        let action = if let Some(ref inner_observation) = &obs.inner_observation {
            self.agent.act(inner_observation, self.prev_episode_done)
        } else {
            // If there is no inner observation then the current state is terminal
            // and the inner episode is done so whatever this action is, it will be ignored.
            assert!(
                obs.episode_done,
                "Expecting episode_done if inner_observation is None"
            );
            self.inner_env_structure.action_space.some_element()
        };

        self.prev_observation = obs.inner_observation.as_ref().cloned();
        self.prev_episode_done = obs.episode_done;

        action
    }
}

impl<AC, OS, AS> Agent<<MetaObservationSpace<OS, AS> as Space>::Element, AS::Element>
    for ResettingMetaAgent<AC, OS, AS>
where
    AC: BuildAgent<OS, AS>,
    OS: Space + Clone,
    AS: NonEmptySpace + Clone,
{
    fn update(
        &mut self,
        _step: FullStep<<MetaObservationSpace<OS, AS> as Space>::Element, AS::Element>,
        _logger: &mut dyn TimeSeriesLogger,
    ) {
        // Does not learn on a meta level
    }
}

/// Never learns on a meta level. Always acts like "Release" mode.
impl<AC, OS, AS> SetActorMode for ResettingMetaAgent<AC, OS, AS>
where
    AC: BuildAgent<OS, AS>,
    OS: Space + Clone,
    AS: Space + Clone,
{
}

#[cfg(test)]
mod resetting_meta {
    use super::super::{ActorMode, UCB1AgentConfig};
    use super::*;
    use crate::envs::{MetaPomdp, OneHotBandits, PomdpEnv};
    use crate::simulation;
    use crate::simulation::hooks::{RewardStatistics, StepLimit};

    #[test]
    fn ucb_one_hot_bandits() {
        let config = ResettingMetaAgentConfig::new(UCB1AgentConfig::default());
        let num_arms = 3;
        let num_episodes_per_trial = 20;
        let mut env = PomdpEnv::new(
            MetaPomdp::new(OneHotBandits::new(num_arms), num_episodes_per_trial),
            0,
        );
        let mut agent = config.build_agent(&env, 0).unwrap();

        let mut hooks = (RewardStatistics::new(), StepLimit::new(1000));
        agent.set_actor_mode(ActorMode::Release);
        simulation::run_actor(&mut env, &mut agent, &mut hooks, &mut ());

        assert!(hooks.0.mean_episode_reward() > 0.7 * (num_episodes_per_trial - num_arms) as f64);
    }
}

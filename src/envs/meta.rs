//! Meta reinforcement learning environment.
use super::{
    BuildEnv, BuildEnvDist, BuildEnvError, BuildPomdp, BuildPomdpDist, EnvDistribution,
    EnvStructure, Environment, Pomdp, PomdpDistribution,
};
use crate::logging::Logger;
use crate::spaces::{BooleanSpace, IntervalSpace, OptionSpace, Space, TupleSpace2, TupleSpace3};
use rand::rngs::StdRng;
use rand::SeedableRng;

// # MetaPomdp

/// A meta reinforcement learning environment that treats RL itself as an environment.
///
/// An episode in this meta environment is called a "Trial" and consists of
/// several episodes from the inner environment.
/// A new inner environment with a different structure seed is sampled for each Trial.
/// A meta episode ends when a fixed number of inner episodes have been completed.
///
/// The step metadata from the inner environment are embedded as observations.
///
/// # Observations
/// An observation is a tuple consisting of:
/// * `inner_observation` - The current inner observation on which the next step will act.
///                             Is `None` if this is a terminal state,
///                             in which case `episode_done` must be `True`.
/// * `step_action`       - The action selected by the agent on the previous step.
///                             Is `None` if this is the first step of an inner episode.
/// * `step_reward`       - The reward earned on the previous step.
///                             Is `None` if this is the first step of an inner episode.
/// * `episode_done`      - Whether the previous step ended the inner episode.
///
/// # Actions
/// The action space is the same as the action space of the inner environments.
/// Actions are forwarded to the inner environment except when the current state is the last state
/// of the inner episode (`episode_done == true`).
/// In that case, the provided action is ignored and the next state will be the start of a new
/// inner episode.
///
/// # Rewards
/// The reward is the same as the inner episode reward.
///
/// # States
/// The state ([`MetaState`]) consists of an inner environment instance,
/// an inner environment state, an episode index within the trial, and details of the most recent
/// inner step within the episode.
///
/// # Related
/// See [`MetaEnv`] for meta [`Environment`] instead of meta [`Pomdp`].
///
/// # Reference
/// This meta environment design is roughly consistent with the structure used in the paper
/// "[RL^2: Fast Reinforcement Learning via Slow Reinforcement Learning][rl2]" by Duan et al.
///
/// [rl2]: https://arxiv.org/pdf/1611.02779
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetaPomdp<E> {
    /// Environment distribution from which each trial's episode is sampled.
    pub env_distribution: E,
    /// Number of inner episodes per trial.
    pub episodes_per_trial: usize,
}

impl<E> MetaPomdp<E> {
    pub const fn new(env_distribution: E, episodes_per_trial: usize) -> Self {
        Self {
            env_distribution,
            episodes_per_trial,
        }
    }
}

impl<E: Default> Default for MetaPomdp<E> {
    fn default() -> Self {
        Self {
            env_distribution: E::default(),
            episodes_per_trial: 10,
        }
    }
}

impl<EB> BuildPomdp for MetaPomdp<EB>
where
    EB: BuildPomdpDist,
    EB::Action: Copy,
{
    type State = <Self::Pomdp as Pomdp>::State;
    type Observation = <Self::Pomdp as Pomdp>::Observation;
    type Action = <Self::Pomdp as Pomdp>::Action;
    type ObservationSpace = <Self::Pomdp as EnvStructure>::ObservationSpace;
    type ActionSpace = <Self::Pomdp as EnvStructure>::ActionSpace;
    type Pomdp = MetaPomdp<EB::PomdpDistribution>;

    fn build_pomdp(&self) -> Result<Self::Pomdp, BuildEnvError> {
        Ok(MetaPomdp::new(
            self.env_distribution.build_pomdp_dist(),
            self.episodes_per_trial,
        ))
    }
}

impl<E> EnvStructure for MetaPomdp<E>
where
    E: EnvStructure,
{
    type ObservationSpace = MetaObservationSpace<E::ObservationSpace, E::ActionSpace>;
    type ActionSpace = E::ActionSpace;

    fn observation_space(&self) -> Self::ObservationSpace {
        meta_observation_space(&self.env_distribution)
    }
    fn action_space(&self) -> Self::ActionSpace {
        self.env_distribution.action_space()
    }
    fn reward_range(&self) -> (f64, f64) {
        self.env_distribution.reward_range()
    }
    fn discount_factor(&self) -> f64 {
        self.env_distribution.discount_factor()
    }
}

impl<E> Pomdp for MetaPomdp<E>
where
    E: PomdpDistribution,
    <E::Pomdp as Pomdp>::Action: Copy,
{
    type State = MetaState<E::Pomdp>;
    type Observation =
        MetaObservation<<E::Pomdp as Pomdp>::Observation, <E::Pomdp as Pomdp>::Action>;
    type Action = <E::Pomdp as Pomdp>::Action;

    fn initial_state(&self, rng: &mut StdRng) -> Self::State {
        // Sample a new inner environment.
        let inner_env = self.env_distribution.sample_pomdp(rng);
        let inner_state = inner_env.initial_state(rng);
        MetaState {
            inner_env,
            inner_state: Some(inner_state),
            episode_index: 0,
            prev_step_info: None,
            inner_episode_done: false,
        }
    }

    fn observe(&self, state: &Self::State, rng: &mut StdRng) -> Self::Observation {
        let inner_observation = state
            .inner_state
            .as_ref()
            .map(|s| state.inner_env.observe(s, rng));
        let step_info = state.prev_step_info;
        let episode_done = state.inner_episode_done;
        (inner_observation, step_info, episode_done)
    }

    fn step(
        &self,
        state: Self::State,
        action: &Self::Action,
        rng: &mut StdRng,
        logger: &mut dyn Logger,
    ) -> (Option<Self::State>, f64, bool) {
        if state.inner_episode_done {
            // Ignore the action and start a new inner episode
            let inner_state = state.inner_env.initial_state(rng);
            let state = MetaState {
                inner_env: state.inner_env,
                inner_state: Some(inner_state),
                episode_index: state.episode_index,
                prev_step_info: None,
                inner_episode_done: false,
            };
            (Some(state), 0.0, false)
        } else {
            // Take a step in the inner episode
            let prev_inner_state = state
                .inner_state
                .expect("inner env has terminal state without ending the episode");
            let (inner_state, inner_step_reward, inner_episode_done) =
                state.inner_env.step(prev_inner_state, action, rng, logger);

            let mut episode_index = state.episode_index;
            let mut trial_done = false;
            if inner_episode_done {
                episode_index += 1;
                if episode_index >= self.episodes_per_trial {
                    // Completed the last inner episode of the trial.
                    // This is treated as an abrupt cut-off of a theorically infinite sequence of
                    // inner episodes so the meta state is not treated as terminal.
                    trial_done = true;
                }
            }

            let state = MetaState {
                inner_env: state.inner_env,
                inner_state,
                episode_index,
                prev_step_info: Some((*action, inner_step_reward)),
                inner_episode_done,
            };
            (Some(state), inner_step_reward, trial_done)
        }
    }
}

// # MetaEnv

/// Configuration for [`MetaEnv`]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetaEnvConfig<EB> {
    /// Environment distribution configuration
    pub env_dist_config: EB,
    /// Number of episodes per trial
    pub num_episodes_per_trial: usize,
}

impl<EB> MetaEnvConfig<EB> {
    pub const fn new(env_dist_config: EB, num_episodes_per_trial: usize) -> Self {
        Self {
            env_dist_config,
            num_episodes_per_trial,
        }
    }
}

impl<EB: Default> Default for MetaEnvConfig<EB> {
    fn default() -> Self {
        Self {
            env_dist_config: EB::default(),
            num_episodes_per_trial: 10,
        }
    }
}

impl<EB> BuildEnv for MetaEnvConfig<EB>
where
    EB: BuildEnvDist,
    EB::Action: Copy,
{
    type Observation = <Self::ObservationSpace as Space>::Element;
    type Action = <Self::ActionSpace as Space>::Element;
    type ObservationSpace = <Self::Environment as EnvStructure>::ObservationSpace;
    type ActionSpace = <Self::Environment as EnvStructure>::ActionSpace;
    type Environment = MetaEnv<EB::EnvDistribution>;

    fn build_env(&self, seed: u64) -> Result<Self::Environment, BuildEnvError> {
        Ok(MetaEnv::new(
            self.env_dist_config.build_env_dist(),
            self.num_episodes_per_trial,
            seed,
        ))
    }
}

/// A meta reinforcement learning environment with internal state.
///
/// See [`MetaPomdp`] for more information.
/// This is implemented in the same way but for [`Environment`] instead of [`Pomdp`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEnv<E: EnvDistribution> {
    /// Environment distribution from which each trial's episode is sampled.
    pub env_distribution: E,
    /// Number of inner episodes per trial.
    pub episodes_per_trial: usize,

    // State
    /// An instance of the inner environment.
    ///
    /// Is `None` between trials when a call to `reset()` is expected.
    env: Option<E::Environment>,
    /// The inner episode index within the current trial.
    episode_index: usize,
    /// Whether the previous step ended the inner episode.
    inner_episode_done: bool,
    /// Random state for sampling new inner environments.
    rng: StdRng,
}

impl<E: EnvDistribution> MetaEnv<E> {
    pub fn new(env_distribution: E, episodes_per_trial: usize, seed: u64) -> Self {
        Self {
            env_distribution,
            episodes_per_trial,
            env: None,
            episode_index: 0,
            inner_episode_done: false,
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

impl<E: EnvDistribution + EnvStructure> EnvStructure for MetaEnv<E> {
    type ObservationSpace = MetaObservationSpace<E::ObservationSpace, E::ActionSpace>;
    type ActionSpace = E::ActionSpace;

    fn observation_space(&self) -> Self::ObservationSpace {
        meta_observation_space(&self.env_distribution)
    }
    fn action_space(&self) -> Self::ActionSpace {
        self.env_distribution.action_space()
    }
    fn reward_range(&self) -> (f64, f64) {
        self.env_distribution.reward_range()
    }
    fn discount_factor(&self) -> f64 {
        self.env_distribution.discount_factor()
    }
}

impl<E> Environment for MetaEnv<E>
where
    E: EnvDistribution,
    <E::Environment as Environment>::Action: Copy,
{
    type Observation = MetaObservation<
        <E::Environment as Environment>::Observation,
        <E::Environment as Environment>::Action,
    >;
    type Action = <E::Environment as Environment>::Action;

    fn step(
        &mut self,
        action: &Self::Action,
        logger: &mut dyn Logger,
    ) -> (Option<Self::Observation>, f64, bool) {
        let env = self.env.as_mut().expect("Must call reset() first");
        if self.inner_episode_done {
            // Ignore the action and start a new inner episode
            let inner_observation = env.reset();
            self.inner_episode_done = false;
            let observation = (Some(inner_observation), None, false);
            (Some(observation), 0.0, false)
        } else {
            // Take a step in the inner episode
            let (inner_observation, reward, inner_episode_done) = env.step(action, logger);
            self.inner_episode_done = inner_episode_done;
            let observation = (
                inner_observation,
                Some((*action, reward)),
                inner_episode_done,
            );

            let mut trial_done = false;
            if inner_episode_done {
                self.episode_index += 1;
                if self.episode_index >= self.episodes_per_trial {
                    // Completed the last inner episode of the trial.
                    // This is treated as an abrupt cut-off of a theorically infinite sequence of
                    // inner episodes so the meta state is not treated as terminal.
                    trial_done = true;
                    self.env = None;
                }
            }

            (Some(observation), reward, trial_done)
        }
    }

    fn reset(&mut self) -> Self::Observation {
        // Start a new trial
        let mut env = self.env_distribution.sample_environment(&mut self.rng);
        let inner_observation = env.reset();
        self.env = Some(env);
        self.episode_index = 0;
        self.inner_episode_done = false;
        (Some(inner_observation), None, false)
    }
}

// # Meta Environment Types

/// Observation type for [`MetaPomdp`] and [`MetaEnv`].
///
/// See [`MetaPomdp`] and [`MetaObservationSpace`] for details.
pub type MetaObservation<O, A> = (Option<O>, Option<(A, f64)>, bool);

/// Meta-environment observation space. See [`MetaPomdp`] for details.
pub type MetaObservationSpace<OS, AS> =
    TupleSpace3<OptionSpace<OS>, OptionSpace<TupleSpace2<AS, IntervalSpace<f64>>>, BooleanSpace>;

/// Construct the meta observation space for an inner environment structure.
fn meta_observation_space<E: EnvStructure + ?Sized>(
    env: &E,
) -> MetaObservationSpace<E::ObservationSpace, E::ActionSpace> {
    let (min_reward, max_reward) = env.reward_range();
    TupleSpace3(
        OptionSpace::new(env.observation_space()),
        OptionSpace::new(TupleSpace2(
            env.action_space(),
            IntervalSpace::new(min_reward, max_reward),
        )),
        BooleanSpace::new(),
    )
}

/// The state of a [`MetaPomdp`].
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MetaState<E: Pomdp> {
    /// An instance of the inner environment (sampled for this trial).
    inner_env: E,
    /// The current inner environment state. `None` represents a terminal state.
    inner_state: Option<E::State>,
    /// The inner episode index within the current trial.
    episode_index: usize,
    /// Details of the previous step of this inner episode. A copy of the action and the reward.
    prev_step_info: Option<(E::Action, f64)>,
    /// Whether the previous step ended the inner episode.
    inner_episode_done: bool,
}

/// Wrapper that provides the inner environment structure of a meta environment.
///
/// # Usage
/// Wraps a meta-environment structure (e.g. [`MetaPomdp`], [`MetaEnv`]),
///
///     use relearn::envs::{InnerEnvStructure, MetaPomdp, OneHotBandits, StoredEnvStructure};
///
///     let base_env = OneHotBandits::default();
///     let meta_env = MetaPomdp::new(base_env, 10);
///
///     let base_structure = StoredEnvStructure::from(&base_env);
///     let meta_inner_structure = StoredEnvStructure::from(&InnerEnvStructure::new(&meta_env));
///
///     assert_eq!(base_structure, meta_inner_structure);
///
/// implements an [`EnvStructure`] corresponding to the inner environment structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InnerEnvStructure<'a, T: ?Sized>(&'a T);

impl<'a, T: ?Sized> InnerEnvStructure<'a, T> {
    pub const fn new(inner_env: &'a T) -> Self {
        Self(inner_env)
    }
}

impl<'a, T, OS, AS> EnvStructure for InnerEnvStructure<'a, T>
where
    T: EnvStructure<ObservationSpace = MetaObservationSpace<OS, AS>, ActionSpace = AS> + ?Sized,
    OS: Space,
    AS: Space,
{
    type ObservationSpace = OS;
    type ActionSpace = AS;

    fn observation_space(&self) -> Self::ObservationSpace {
        self.0.observation_space().0.inner
    }
    fn action_space(&self) -> Self::ActionSpace {
        self.0.action_space()
    }
    fn reward_range(&self) -> (f64, f64) {
        self.0.reward_range()
    }
    fn discount_factor(&self) -> f64 {
        self.0.discount_factor()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Comparing exact reward values; 0.0 or 1.0 without error
mod meta_env_bandits {
    use super::super::testing;
    use super::*;

    #[test]
    fn build_meta_pomdp() {
        let config = MetaPomdp::new(testing::RoundRobinDeterministicBandits::new(2), 3);
        let _pomdp = config.build_pomdp().unwrap();
    }

    #[test]
    fn build_meta_env() {
        let config = MetaEnvConfig::new(testing::RoundRobinDeterministicBandits::new(2), 3);
        let _env = config.build_env(0);
    }

    #[test]
    fn run_meta_pomdp() {
        let env = MetaPomdp::new(testing::RoundRobinDeterministicBandits::new(2), 3);
        testing::run_pomdp(env, 1000, 0);
    }

    #[test]
    fn run_meta_env() {
        let mut env = MetaEnv::new(testing::RoundRobinDeterministicBandits::new(2), 3, 0);
        testing::run_env(&mut env, 1000, 0);
    }

    #[test]
    fn meta_pomdp_expected_steps() {
        let env = MetaPomdp::new(testing::RoundRobinDeterministicBandits::new(2), 3);
        let mut rng = StdRng::seed_from_u64(0);

        // Trial 0; Ep 0; Init
        let state = env.initial_state(&mut rng);
        assert_eq!(env.observe(&state, &mut rng), (Some(()), None, false));

        // Trial 0; Ep 0; Step 0
        // Take action 0 and get 1 reward
        // Inner state is terminal.
        let (maybe_state, reward, trial_done) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, 1.0);
        assert!(!trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (None, Some((0, 1.0)), true));

        // Trial 0; Ep 1; Init.
        // The action is ignored and a new inner episode is started.
        let (maybe_state, reward, trial_done) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (Some(()), None, false));

        // Trial 0; Ep 1; Step 0
        // Take action 1 and get 0 reward
        // Inner state is terminal
        let (maybe_state, reward, trial_done) = env.step(state, &1, &mut rng, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (None, Some((1, 0.0)), true));

        // Trial 0; Ep 2; Init.
        // The action is ignored and a new inner episode is started.
        let (maybe_state, reward, trial_done) = env.step(state, &1, &mut rng, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (Some(()), None, false));

        // Trial 0; Ep 2; Step 0
        // Take action 0 and get 1 reward
        // The inner state is terminal.
        // This inner episode was the last in the trial so the trial is done.
        let (maybe_state, reward, trial_done) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, 1.0);
        assert!(trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (None, Some((0, 1.0)), true));

        // Trial 1; Ep 1; Init.
        let state = env.initial_state(&mut rng);
        assert_eq!(env.observe(&state, &mut rng), (Some(()), None, false));

        // Trial 1; Ep 0; Step 0
        // Take action 0 and get 0 reward, since now 1 is the target action
        // Inner state is terminal.
        let (maybe_state, reward, trial_done) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        let state = maybe_state.unwrap();
        assert_eq!(env.observe(&state, &mut rng), (None, Some((0, 0.0)), true));
    }

    #[test]
    fn meta_env_expected_steps() {
        let mut env = MetaEnv::new(testing::RoundRobinDeterministicBandits::new(2), 3, 0);

        // Trial 0; Ep 0; Init
        let observation = env.reset();
        assert_eq!(observation, (Some(()), None, false));

        // Trial 0; Ep 0; Step 0
        // Take action 0 and get 1 reward
        // Inner state is terminal.
        let (observation, reward, trial_done) = env.step(&0, &mut ());
        assert_eq!(reward, 1.0);
        assert!(!trial_done);
        assert_eq!(observation, Some((None, Some((0, 1.0)), true)));

        // Trial 0; Ep 1; Init.
        // The action is ignored and a new inner episode is started.
        let (observation, reward, trial_done) = env.step(&0, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        assert_eq!(observation, Some((Some(()), None, false)));

        // Trial 0; Ep 1; Step 0
        // Take action 1 and get 0 reward
        // Inner state is terminal
        let (observation, reward, trial_done) = env.step(&1, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        assert_eq!(observation, Some((None, Some((1, 0.0)), true)));

        // Trial 0; Ep 2; Init.
        // The action is ignored and a new inner episode is started.
        let (observation, reward, trial_done) = env.step(&1, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        assert_eq!(observation, Some((Some(()), None, false)));

        // Trial 0; Ep 2; Step 0
        // Take action 0 and get 1 reward
        // The inner state is terminal.
        // This inner episode was the last in the trial so the trial is done.
        let (observation, reward, trial_done) = env.step(&0, &mut ());
        assert_eq!(reward, 1.0);
        assert!(trial_done);
        assert_eq!(observation, Some((None, Some((0, 1.0)), true)));

        // Trial 1; Ep 1; Init.
        let observation = env.reset();
        assert_eq!(observation, (Some(()), None, false));

        // Trial 1; Ep 0; Step 0
        // Take action 0 and get 0 reward, since now 1 is the target action
        // Inner state is terminal.
        let (observation, reward, trial_done) = env.step(&0, &mut ());
        assert_eq!(reward, 0.0);
        assert!(!trial_done);
        assert_eq!(observation, Some((None, Some((0, 0.0)), true)));
    }
}

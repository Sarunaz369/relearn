//! Meta reinforcement learning environment.
use super::{
    BuildEnv, BuildEnvDist, BuildEnvError, EnvDistribution, EnvStructure, Environment,
    StructurePreservingWrapper, Successor, Wrapped,
};
use crate::feedback::Reward;
use crate::logging::StatsLogger;
use crate::spaces::{BooleanSpace, OptionSpace, ProductSpace, Space};
use crate::Prng;
use serde::{Deserialize, Serialize};
use std::fmt;

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
/// A [`MetaObservation`]. Consists of the inner observation, the previous step action and
/// feedback, and whether the inner episode is done.
///
/// # Actions
/// The action space is the same as the action space of the inner environments.
/// Actions are forwarded to the inner environment except when the current state is the last state
/// of the inner episode (`episode_done == true`).
/// In that case, the provided action is ignored and the next state will be the start of a new
/// inner episode.
///
/// # Feedback
/// The inner environment feedback must implement [`MetaFeedback`] (and [`MetaFeedbackSpace`] for
/// the space). This feedback is decomposed into separate inner and outer feedback.
/// In the case of [`Reward`] feedback, the same value is used as both the inner and outer feedback.
///
/// # States
/// The state ([`MetaState`]) consists of an inner environment instance,
/// an inner environment state, an episode index within the trial, and details of the most recent
/// inner step within the episode.
///
/// # Reference
/// This meta environment design is roughly consistent with the structure used in the paper
/// "[RL^2: Fast Reinforcement Learning via Slow Reinforcement Learning][rl2]" by Duan et al.
///
/// [rl2]: https://arxiv.org/abs/1611.02779
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetaEnv<E> {
    /// Environment distribution from which each trial's episode is sampled.
    pub env_distribution: E,
}

impl<E> MetaEnv<E> {
    pub const fn new(env_distribution: E) -> Self {
        Self { env_distribution }
    }
}

impl<E: Default> Default for MetaEnv<E> {
    fn default() -> Self {
        Self {
            env_distribution: E::default(),
        }
    }
}

impl<EC> BuildEnv for MetaEnv<EC>
where
    EC: BuildEnvDist,
    EC::Action: Copy,
    // Bounds are a bit of a mess to convince the compiler that
    // EC::FeedbackSpace::InnerSpace::Element == EC::Feedback::Inner
    EC::FeedbackSpace: MetaFeedbackSpace<Element = EC::Feedback>,
    <EC::FeedbackSpace as Space>::Element: MetaFeedback,
    EC::Feedback: MetaFeedback,
{
    type Observation = <Self::Environment as Environment>::Observation;
    type Action = <Self::Environment as Environment>::Action;
    type Feedback = <Self::Environment as Environment>::Feedback;
    type ObservationSpace = <Self::Environment as EnvStructure>::ObservationSpace;
    type ActionSpace = <Self::Environment as EnvStructure>::ActionSpace;
    type FeedbackSpace = <Self::Environment as EnvStructure>::FeedbackSpace;
    type Environment = MetaEnv<EC::EnvDistribution>;

    fn build_env(&self, _: &mut Prng) -> Result<Self::Environment, BuildEnvError> {
        Ok(MetaEnv::new(self.env_distribution.build_env_dist()))
    }
}

impl<E> EnvStructure for MetaEnv<E>
where
    E: EnvStructure,
    E::FeedbackSpace: MetaFeedbackSpace,
{
    type ObservationSpace = MetaObservationSpace<
        E::ObservationSpace,
        E::ActionSpace,
        <E::FeedbackSpace as MetaFeedbackSpace>::InnerSpace,
    >;
    type ActionSpace = E::ActionSpace;
    type FeedbackSpace = <E::FeedbackSpace as MetaFeedbackSpace>::OuterSpace;

    fn observation_space(&self) -> Self::ObservationSpace {
        MetaObservationSpace::from_inner_env(&self.env_distribution)
    }
    fn action_space(&self) -> Self::ActionSpace {
        self.env_distribution.action_space()
    }
    fn feedback_space(&self) -> Self::FeedbackSpace {
        self.env_distribution.feedback_space().into_outer()
    }
    fn discount_factor(&self) -> f64 {
        self.env_distribution.discount_factor()
    }
}

impl<E> MetaEnv<E>
where
    E: EnvStructure,
{
    /// View the structure of the inner environment.
    pub const fn inner_structure(&self) -> InnerEnvStructure<&Self> {
        InnerEnvStructure::new(self)
    }
}

impl<E> Environment for MetaEnv<E>
where
    E: EnvDistribution,
    E::Action: Clone,
    E::Observation: Clone,
    E::Feedback: MetaFeedback,
{
    type State = MetaState<E::Environment>;
    type Observation =
        MetaObservation<E::Observation, E::Action, <E::Feedback as MetaFeedback>::Inner>;
    type Action = E::Action;
    type Feedback = <E::Feedback as MetaFeedback>::Outer;

    fn initial_state(&self, rng: &mut Prng) -> Self::State {
        // Sample a new inner environment.
        let inner_env = self.env_distribution.sample_environment(rng);
        let inner_state = inner_env.initial_state(rng);
        MetaState {
            inner_env,
            inner_successor: Successor::Continue(inner_state),
            prev_step_obs: None,
        }
    }

    fn observe(&self, state: &Self::State, rng: &mut Prng) -> Self::Observation {
        let inner_successor_obs = state
            .inner_successor
            .as_ref()
            .map(|s| state.inner_env.observe(s, rng));
        let episode_done = inner_successor_obs.episode_done();
        MetaObservation {
            inner_observation: inner_successor_obs.into_inner(),
            prev_step: state.prev_step_obs.clone(),
            episode_done,
        }
    }

    fn step(
        &self,
        state: Self::State,
        action: &Self::Action,
        rng: &mut Prng,
        logger: &mut dyn StatsLogger,
    ) -> (Successor<Self::State>, Self::Feedback) {
        match state.inner_successor {
            Successor::Continue(prev_inner_state) => {
                // Take a step in the inner episode
                let (inner_successor, feedback) =
                    state.inner_env.step(prev_inner_state, action, rng, logger);
                let (inner_feedback, outer_feedback) = feedback.into_inner_outer();

                let new_state = MetaState {
                    inner_env: state.inner_env,
                    inner_successor,
                    prev_step_obs: Some(InnerStepObs {
                        action: action.clone(),
                        feedback: inner_feedback,
                    }),
                };

                (Successor::Continue(new_state), outer_feedback)
            }
            _ => {
                // The inner state ended the episode.
                // Ignore the action and start a new inner episode.
                let inner_state = state.inner_env.initial_state(rng);
                let state = MetaState {
                    inner_env: state.inner_env,
                    inner_successor: Successor::Continue(inner_state),
                    prev_step_obs: None,
                };
                (Successor::Continue(state), E::Feedback::neutral_outer())
            }
        }
    }
}

/// A feedback space that can be decomposed into an inner and outer space for a meta environment.
pub trait MetaFeedbackSpace: Space<Element = <Self as MetaFeedbackSpace>::Element> {
    type Element: MetaFeedback;
    type InnerSpace: Space<Element = <<Self as MetaFeedbackSpace>::Element as MetaFeedback>::Inner>;
    type OuterSpace: Space<Element = <<Self as MetaFeedbackSpace>::Element as MetaFeedback>::Outer>;

    /// Convert the feedback space into the inner and outer feedback spaces.
    fn into_inner_outer(self) -> (Self::InnerSpace, Self::OuterSpace);

    /// Convert the feedback space into the inner feedback space.
    #[inline]
    fn into_inner(self) -> Self::InnerSpace
    where
        Self: Sized,
    {
        self.into_inner_outer().0
    }

    /// Convert the feedback space into the outer feedback space.
    fn into_outer(self) -> Self::OuterSpace
    where
        Self: Sized,
    {
        self.into_inner_outer().1
    }
}

/// Reward feedback is always replicated in both the inner and outer environments.
///
/// This is the structure of RL-Squared meta reinforcement learning.
impl<F> MetaFeedbackSpace for F
where
    F: Space<Element = Reward> + Clone,
{
    type Element = Reward;
    type InnerSpace = Self;
    type OuterSpace = Self;

    #[inline]
    fn into_inner_outer(self) -> (Self::InnerSpace, Self::OuterSpace) {
        (self.clone(), self)
    }
    #[inline]
    fn into_inner(self) -> Self::InnerSpace {
        self
    }
    #[inline]
    fn into_outer(self) -> Self::OuterSpace {
        self
    }
}

/// A feedback type that can be decomposed into an inner and outer space for a meta environment.
pub trait MetaFeedback {
    // Clone + Send to match bound on `Space::Element`.
    type Inner: Clone + Send;
    type Outer: Clone + Send;

    /// Neutral outer feedback that does not indicate good or bad behaviour.
    fn neutral_outer() -> Self::Outer;

    /// Split the feedback into inner and outer environment feedback
    fn into_inner_outer(self) -> (Self::Inner, Self::Outer);
}

/// Reward feedback is always replicated in both the inner and outer environments.
///
/// This is the structure of RL-Squared meta reinforcement learning.
impl MetaFeedback for Reward {
    type Inner = Self;
    type Outer = Self;

    #[inline]
    fn neutral_outer() -> Self {
        Self(0.0)
    }

    #[inline]
    fn into_inner_outer(self) -> (Self, Self) {
        (self, self)
    }
}

// # Meta Environment Types

/// Observation of a completed inner step in a meta environment.
///
/// The agent is expected to remember the inner observation if necessary, so it is not included.
/// The agent is not expected to have a mechanism to remember its own actions, since actions only
/// ought to matter to the extent that they affect the resulting state.
///
/// # Note
/// The generic feedback type `FI` is the inner feedback `E::Feedback::Inner` for `MetaEnv<E>`.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct InnerStepObs<A, FI> {
    /// Action selected by the agent on the step.
    pub action: A,
    /// Inner feedback for this step.
    pub feedback: FI,
}

/// Observation space for [`InnerStepObs`].
///
/// # Note
/// The generic feedback space type `FSI`
/// is the inner feedback space `E::FeedbackSpace::InnerSpace` for `MetaEnv<E>`.
#[derive(Debug, Copy, Clone, PartialEq, ProductSpace)]
#[element(InnerStepObs<AS::Element, FSI::Element>)]
pub struct InnerStepObsSpace<AS, FSI> {
    pub action: AS,
    pub feedback: FSI,
}

impl<AS, FSI> InnerStepObsSpace<AS, FSI> {
    /// Construct a step observation space from an inner environment structure
    fn from_inner_env<E>(env: &E) -> Self
    where
        E: EnvStructure<ActionSpace = AS> + ?Sized,
        E::FeedbackSpace: MetaFeedbackSpace<InnerSpace = FSI>,
    {
        Self {
            action: env.action_space(),
            feedback: env.feedback_space().into_inner(),
        }
    }
}

/// An observation from a meta enviornment.
///
/// # Note
/// The generic feedback type `FI` is the inner feedback `E::Feedback::Inner` for `MetaEnv<E>`.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct MetaObservation<O, A, FI> {
    /// The current inner observation on which the next step will act.
    ///
    /// Is `None` if this is a terminal state, in which case `episode_done` must be `True`.
    pub inner_observation: Option<O>,

    /// Observation of the previous inner step.
    ///
    /// Is `None` if this is the first step of an inner episode.
    pub prev_step: Option<InnerStepObs<A, FI>>,

    /// Whether the previous step ended the inner episode.
    pub episode_done: bool,
}

/// [`MetaEnv`] observation space for element [`MetaObservation`].
///
/// # Note
/// The generic feedback space type `FSI`
/// is the inner feedback space `E::FeedbackSpace::InnerSpace` for `MetaEnv<E>`.
#[derive(Debug, Copy, Clone, PartialEq, ProductSpace)]
#[element(MetaObservation<OS::Element, AS::Element, FSI::Element>)]
pub struct MetaObservationSpace<OS, AS, FSI> {
    pub inner_observation: OptionSpace<OS>,
    pub prev_step: OptionSpace<InnerStepObsSpace<AS, FSI>>,
    pub episode_done: BooleanSpace,
}

impl<OS, AS, FSI> MetaObservationSpace<OS, AS, FSI> {
    /// Construct a meta observation space from an inner environment structure
    fn from_inner_env<E>(env: &E) -> Self
    where
        E: EnvStructure<ObservationSpace = OS, ActionSpace = AS> + ?Sized,
        E::FeedbackSpace: MetaFeedbackSpace<InnerSpace = FSI>,
    {
        Self {
            inner_observation: OptionSpace::new(env.observation_space()),
            prev_step: OptionSpace::new(InnerStepObsSpace::from_inner_env(env)),
            episode_done: BooleanSpace,
        }
    }
}

/// The state of a [`MetaEnv`].
pub struct MetaState<E: Environment>
where
    E::Feedback: MetaFeedback,
{
    /// An instance of the inner environment (sampled for this trial).
    inner_env: E,
    /// The upcoming inner environment state.
    inner_successor: Successor<E::State>,
    /// Observation of the previous step of this inner episode.
    prev_step_obs: Option<InnerStepObs<E::Action, <E::Feedback as MetaFeedback>::Inner>>,
}

// Custom implementations to satisfy the non-trivial associated type bounds

#[allow(clippy::expl_impl_clone_on_copy)]
impl<E> Clone for MetaState<E>
where
    E: Environment,
    E: Clone,
    E::State: Clone,
    E::Action: Clone,
    E::Feedback: MetaFeedback,
{
    fn clone(&self) -> Self {
        Self {
            inner_env: self.inner_env.clone(),
            inner_successor: self.inner_successor.clone(),
            prev_step_obs: self.prev_step_obs.clone(),
        }
    }
}

impl<E> Copy for MetaState<E>
where
    E: Environment,
    E: Copy,
    E::State: Copy,
    E::Action: Copy,
    E::Feedback: MetaFeedback,
    <E::Feedback as MetaFeedback>::Inner: Copy,
{
}

impl<E> PartialEq for MetaState<E>
where
    E: Environment,
    E: PartialEq,
    E::State: PartialEq,
    E::Action: PartialEq,
    E::Feedback: MetaFeedback,
    <E::Feedback as MetaFeedback>::Inner: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.inner_env == other.inner_env
            && self.inner_successor == other.inner_successor
            && self.prev_step_obs == other.prev_step_obs
    }
}

impl<E> fmt::Debug for MetaState<E>
where
    E: Environment,
    E: fmt::Debug,
    E::State: fmt::Debug,
    E::Action: fmt::Debug,
    E::Feedback: MetaFeedback,
    <E::Feedback as MetaFeedback>::Inner: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MetaState")
            .field("inner_env", &self.inner_env)
            .field("inner_successor", &self.inner_successor)
            .field("prev_step_obs", &self.prev_step_obs)
            .finish()
    }
}

/// Whether the inner episode represented by this state is done.
pub trait InnerEpisodeDone {
    fn inner_episode_done(&self) -> bool;
}

impl<E> InnerEpisodeDone for MetaState<E>
where
    E: Environment,
    E::Feedback: MetaFeedback,
{
    #[inline]
    fn inner_episode_done(&self) -> bool {
        self.inner_successor.episode_done()
    }
}

impl<A, B> InnerEpisodeDone for (A, B)
where
    A: InnerEpisodeDone,
{
    #[inline]
    fn inner_episode_done(&self) -> bool {
        self.0.inner_episode_done()
    }
}

/// Wrapper that provides the inner environment structure of a meta environment ([`MetaEnv`]).
///
/// Implements [`EnvStructure`] according to the inner environment structure.
/// Can also be used via `MetaEnv::inner_structure`.
///
/// # Example
///
///     use relearn::envs::{meta::InnerEnvStructure, MetaEnv, OneHotBandits, StoredEnvStructure};
///
///     let base_env = OneHotBandits::default();
///     let meta_env = MetaEnv::new(base_env);
///
///     let base_structure = StoredEnvStructure::from(&base_env);
///     let meta_inner_structure = StoredEnvStructure::from(&InnerEnvStructure::new(&meta_env));
///
///     assert_eq!(base_structure, meta_inner_structure);
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InnerEnvStructure<T>(T);

impl<T> InnerEnvStructure<T> {
    pub const fn new(inner_env: T) -> Self {
        Self(inner_env)
    }
}

impl<T, OS, AS, FS> EnvStructure for InnerEnvStructure<T>
where
    T: EnvStructure<
        ObservationSpace = MetaObservationSpace<OS, AS, FS>,
        ActionSpace = AS,
        FeedbackSpace = FS,
    >,
    OS: Space,
    AS: Space,
    FS: Space,
{
    type ObservationSpace = OS;
    type ActionSpace = AS;
    type FeedbackSpace = FS;

    fn observation_space(&self) -> Self::ObservationSpace {
        self.0.observation_space().inner_observation.inner
    }
    fn action_space(&self) -> Self::ActionSpace {
        self.0.action_space()
    }
    fn feedback_space(&self) -> Self::FeedbackSpace {
        self.0.feedback_space()
    }
    fn discount_factor(&self) -> f64 {
        self.0.discount_factor()
    }
}

/// Wrapper that limits the number of inner episodes in a meta-env trial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrialEpisodeLimit {
    pub episodes_per_trial: u64,
}

impl TrialEpisodeLimit {
    #[must_use]
    #[inline]
    pub fn new(episodes_per_trial: u64) -> Self {
        assert!(
            episodes_per_trial > 0,
            "trials must contain at least 1 episode"
        );
        Self { episodes_per_trial }
    }
}

impl Default for TrialEpisodeLimit {
    #[inline]
    fn default() -> Self {
        Self {
            episodes_per_trial: 10,
        }
    }
}

impl StructurePreservingWrapper for TrialEpisodeLimit {}

impl<E> Environment for Wrapped<E, TrialEpisodeLimit>
where
    E: Environment,
    E::State: InnerEpisodeDone,
{
    /// (Inner state, remaining episodes)
    type State = (E::State, u64);
    type Observation = E::Observation;
    type Action = E::Action;
    type Feedback = E::Feedback;

    #[inline]
    fn initial_state(&self, rng: &mut Prng) -> Self::State {
        assert!(
            self.wrapper.episodes_per_trial > 0,
            "trials must contain at least 1 episode"
        );
        (
            self.inner.initial_state(rng),
            self.wrapper.episodes_per_trial,
        )
    }

    #[inline]
    fn observe(&self, state: &Self::State, rng: &mut Prng) -> Self::Observation {
        self.inner.observe(&state.0, rng)
    }

    fn step(
        &self,
        state: Self::State,
        action: &Self::Action,
        rng: &mut Prng,
        logger: &mut dyn StatsLogger,
    ) -> (Successor<Self::State>, Self::Feedback) {
        // Note: "inner" here refers to the wrapped environment,
        // not the inner environment of the meta structure.
        let (inner_state, mut remaining_episodes) = state;
        let (inner_successor, feedback) = self.inner.step(inner_state, action, rng, logger);
        let successor = inner_successor
            .map(|s| {
                if s.inner_episode_done() {
                    remaining_episodes -= 1;
                }
                (s, remaining_episodes)
            })
            .then_interrupt_if(|(_, remaining_episodes)| *remaining_episodes == 0);
        (successor, feedback)
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // Comparing exact reward values; 0.0 or 1.0 without error
mod meta_env_bandits {
    use super::super::{testing, Wrap};
    use super::*;
    use crate::feedback::Reward;
    use rand::SeedableRng;

    #[test]
    fn build_meta_env() {
        let config = MetaEnv::new(testing::RoundRobinDeterministicBandits::new(2))
            .wrap(TrialEpisodeLimit::new(3));
        let _env = config.build_env(&mut Prng::seed_from_u64(0)).unwrap();
    }

    #[test]
    fn run_meta_env() {
        let env = MetaEnv::new(testing::RoundRobinDeterministicBandits::new(2))
            .wrap(TrialEpisodeLimit::new(3));
        testing::check_structured_env(&env, 1000, 0);
    }

    #[test]
    fn meta_env_expected_steps() {
        let env = MetaEnv::new(testing::RoundRobinDeterministicBandits::new(2))
            .wrap(TrialEpisodeLimit::new(3));
        let mut rng = Prng::seed_from_u64(0);

        // Trial 0; Ep 0; Init
        let state = env.initial_state(&mut rng);
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: Some(()),
                prev_step: None,
                episode_done: false
            }
        );

        // Trial 0; Ep 0; Step 0
        // Take action 0 and get 1 reward
        // Inner state is terminal.
        let (successor, feedback) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(feedback, Reward(1.0));
        let state = successor.into_continue().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: None,
                prev_step: Some(InnerStepObs {
                    action: 0,
                    feedback: Reward(1.0)
                }),
                episode_done: true
            }
        );

        // Trial 0; Ep 1; Init.
        // The action is ignored and a new inner episode is started.
        let (successor, reward) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, Reward(0.0));
        let state = successor.into_continue().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: Some(()),
                prev_step: None,
                episode_done: false
            }
        );

        // Trial 0; Ep 1; Step 0
        // Take action 1 and get 0 reward
        // Inner state is terminal
        let (successor, feedback) = env.step(state, &1, &mut rng, &mut ());
        assert_eq!(feedback, Reward(0.0));
        let state = successor.into_continue().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: None,
                prev_step: Some(InnerStepObs {
                    action: 1,
                    feedback: Reward(0.0)
                }),
                episode_done: true
            }
        );

        // Trial 0; Ep 2; Init.
        // The action is ignored and a new inner episode is started.
        let (successor, feedback) = env.step(state, &1, &mut rng, &mut ());
        assert_eq!(feedback, Reward(0.0));
        let state = successor.into_continue().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: Some(()),
                prev_step: None,
                episode_done: false
            }
        );

        // Trial 0; Ep 2; Step 0
        // Take action 0 and get 1 reward
        // The inner state is terminal.
        // This inner episode was the last in the trial so the trial is done.
        let (successor, feedback) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(feedback, Reward(1.0));
        let state = successor.into_interrupt().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: None,
                prev_step: Some(InnerStepObs {
                    action: 0,
                    feedback: Reward(1.0)
                }),
                episode_done: true
            }
        );

        // Trial 1; Ep 1; Init.
        let state = env.initial_state(&mut rng);
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: Some(()),
                prev_step: None,
                episode_done: false
            }
        );

        // Trial 1; Ep 0; Step 0
        // Take action 0 and get 0 reward, since now 1 is the target action
        // Inner state is terminal.
        let (successor, reward) = env.step(state, &0, &mut rng, &mut ());
        assert_eq!(reward, Reward(0.0));
        let state = successor.into_continue().unwrap();
        assert_eq!(
            env.observe(&state, &mut rng),
            MetaObservation {
                inner_observation: None,
                prev_step: Some(InnerStepObs {
                    action: 0,
                    feedback: Reward(0.0)
                }),
                episode_done: true
            }
        );
    }
}

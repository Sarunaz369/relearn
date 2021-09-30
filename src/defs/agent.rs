use super::{CriticDef, CriticUpdaterDef, PolicyDef, PolicyUpdaterDef};
use crate::agents::{
    Agent, BetaThompsonSamplingAgentConfig, BoxingManager, BuildAgent, BuildAgentError,
    BuildManagerAgent, FullAgent, ManagerAgent, MutexAgentManager, RandomAgentConfig,
    ResettingMetaAgent, TabularQLearningAgentConfig, UCB1AgentConfig,
};
use crate::envs::{EnvStructure, InnerEnvStructure, MetaObservationSpace};
use crate::logging::Loggable;
use crate::spaces::{
    BatchFeatureSpace, ElementRefInto, FeatureSpace, FiniteSpace, ParameterizedDistributionSpace,
    SampleSpace, Space,
};
use crate::torch::agents::ActorCriticConfig;
use std::borrow::Borrow;
use std::fmt::Debug;
use tch::Tensor;

/// Agent definition
#[derive(Debug, Clone, PartialEq)]
pub enum AgentDef {
    /// An agent that selects actions randomly.
    Random,
    /// Epsilon-greedy tabular Q learning.
    TabularQLearning(TabularQLearningAgentConfig),
    /// Thompson sampling of for Bernoulli rewards using Beta priors.
    ///
    /// Assumes no relationship between states.
    BetaThompsonSampling(BetaThompsonSamplingAgentConfig),
    /// UCB1 agent from Auer 2002
    UCB1(UCB1AgentConfig),
    /// Torch actor-critic agent
    ActorCritic(Box<ActorCriticConfig<PolicyDef, PolicyUpdaterDef, CriticDef, CriticUpdaterDef>>),
    /// Applies a non-meta agent to a meta environment by resetting between trials
    ResettingMeta(Box<AgentDef>),
}

/// Multi-thread agent definition
#[derive(Debug, Clone, PartialEq)]
pub enum MultiThreadAgentDef {
    /// A mutex-based simulated multi-thread agent. Does not provide meaningful parallelism.
    Mutex(Box<AgentDef>),
}

/// A comprehensive space trait for use by RL agents.
///
/// This includes most interfaces required by any agent, environment, or simulator
/// excluding interfaces that can only apply to some spaces, like [`FiniteSpace`].
pub trait RLSpace: Space + SampleSpace + ElementRefInto<Loggable> + Debug {}
impl<T: Space + SampleSpace + ElementRefInto<Loggable> + Debug> RLSpace for T {}

/// Comprehensive observation space for use in reinforcement learning
pub trait RLObservationSpace: RLSpace + FeatureSpace<Tensor> + BatchFeatureSpace<Tensor> {}
impl<T: RLSpace + FeatureSpace<Tensor> + BatchFeatureSpace<Tensor>> RLObservationSpace for T {}

/// Comprehensive action space for use in reinforcement learning
pub trait RLActionSpace: RLSpace + ParameterizedDistributionSpace<Tensor> {}
impl<T: RLSpace + ParameterizedDistributionSpace<Tensor>> RLActionSpace for T {}

// TODO Change ForAnyAny etc into BuildAgent-like traits

/// Wrapper implementing [`BuildAgent`] for [`AgentDef`] for any observation and action space.
///
/// More specifically, any observation and action space satisfying the relatively generic
/// [`RLObservationSpace`] and [`RLActionSpace`] traits.
///
/// There is no trait specialization so this will fail for those agents that require a tighter
/// bounds on the observation and actions paces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForAnyAny<T>(T);

impl<T> ForAnyAny<T> {
    pub const fn new(agent_def: T) -> Self {
        Self(agent_def)
    }
}

impl<T, E> BuildAgent<E> for ForAnyAny<T>
where
    T: Borrow<AgentDef>,
    E: EnvStructure + ?Sized,
    <E as EnvStructure>::ObservationSpace: RLObservationSpace + Send + 'static,
    <E as EnvStructure>::ActionSpace: RLActionSpace + Send + 'static,
{
    type Agent = Box<DynFullAgent<E>>;

    fn build_agent(&self, env: &E, seed: u64) -> Result<Self::Agent, BuildAgentError> {
        use AgentDef::*;
        match self.0.borrow() {
            Random => RandomAgentConfig::new()
                .build_agent(env, seed)
                .map(|a| Box::new(a) as _),
            // TODO: Implement Send for ActorCriticAgent
            /*
            ActorCritic(config) => config
                .as_ref()
                .build_agent(env, seed)
                .map(|a| Box::new(a) as _),
            */
            _ => Err(BuildAgentError::InvalidSpaceBounds),
        }
    }
}

impl<T, E> BuildManagerAgent<E> for ForAnyAny<T>
where
    T: Borrow<MultiThreadAgentDef>,
    T: Borrow<MultiThreadAgentDef>,
    E: EnvStructure + ?Sized,
    <E as EnvStructure>::ObservationSpace: RLObservationSpace + Send + 'static,
    <E as EnvStructure>::ActionSpace: RLActionSpace + Send + 'static,
{
    type ManagerAgent = Box<DynEnvManagerAgent<E>>;

    fn build_manager_agent(
        &self,
        env: &E,
        seed: u64,
    ) -> Result<Self::ManagerAgent, BuildAgentError> {
        use MultiThreadAgentDef::*;
        match self.0.borrow() {
            Mutex(config) => ForAnyAny(config.borrow())
                .build_agent(env, seed)
                .map(|a| Box::new(BoxingManager::new(MutexAgentManager::new(a))) as _),
        }
    }
}

/// Wrapper implementing [`BuildAgent`] for [`AgentDef`] for finite observation and action spaces.
///
/// There is no trait specialization so this will fail for those agents that require a tighter
/// bounds on the observation and actions paces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForFiniteFinite<T>(T);

impl<T> ForFiniteFinite<T> {
    pub const fn new(agent_def: T) -> Self {
        Self(agent_def)
    }
}

impl<T, E> BuildAgent<E> for ForFiniteFinite<T>
where
    T: Borrow<AgentDef>,
    E: EnvStructure + ?Sized,
    <E as EnvStructure>::ObservationSpace: RLObservationSpace + FiniteSpace + Send + 'static,
    <E as EnvStructure>::ActionSpace: RLActionSpace + FiniteSpace + Send + 'static,
{
    type Agent = Box<DynFullAgent<E>>;

    fn build_agent(&self, env: &E, seed: u64) -> Result<Self::Agent, BuildAgentError> {
        use AgentDef::*;
        match self.0.borrow() {
            TabularQLearning(config) => config.build_agent(env, seed).map(|a| Box::new(a) as _),
            BetaThompsonSampling(config) => config.build_agent(env, seed).map(|a| Box::new(a) as _),
            UCB1(config) => config.build_agent(env, seed).map(|a| Box::new(a) as _),
            agent_def => ForAnyAny::new(agent_def).build_agent(env, seed),
        }
    }
}

impl<T, E> BuildManagerAgent<E> for ForFiniteFinite<T>
where
    T: Borrow<MultiThreadAgentDef>,
    E: EnvStructure + ?Sized,
    <E as EnvStructure>::ObservationSpace: RLObservationSpace + FiniteSpace + Send + 'static,
    <E as EnvStructure>::ActionSpace: RLActionSpace + FiniteSpace + Send + 'static,
{
    type ManagerAgent = Box<DynEnvManagerAgent<E>>;

    fn build_manager_agent(
        &self,
        env: &E,
        seed: u64,
    ) -> Result<Self::ManagerAgent, BuildAgentError> {
        use MultiThreadAgentDef::*;
        match self.0.borrow() {
            Mutex(config) => ForFiniteFinite(config.borrow())
                .build_agent(env, seed)
                .map(|a| Box::new(BoxingManager::new(MutexAgentManager::new(a))) as _),
        }
    }
}

/// Wrapper implementing [`BuildAgent`] for [`AgentDef`] for meta finite obs/action spaces.
///
/// Specifically, it is the inner observation space that must be finite.
/// The outer observation space is not finite.
///
/// There is no trait specialization so this will fail for those agents that require a tighter
/// bounds on the observation and actions paces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ForMetaFiniteFinite<T>(T);

impl<T> ForMetaFiniteFinite<T> {
    pub const fn new(agent_def: T) -> Self {
        Self(agent_def)
    }
}

impl<T, E, OS, AS> BuildAgent<E> for ForMetaFiniteFinite<T>
where
    T: Borrow<AgentDef>,
    E: EnvStructure<ObservationSpace = MetaObservationSpace<OS, AS>, ActionSpace = AS> + ?Sized,
    OS: RLObservationSpace + FiniteSpace + Clone + Send + 'static,
    <OS as Space>::Element: Clone + Send, // ResettingMetaAgent: Send requires OS::Element: Send
    AS: RLActionSpace + FiniteSpace + Clone + Send + 'static,
    <AS as Space>::Element: Clone,
    // I think this ought to be inferrable but for whatever reason it isn't
    <E as EnvStructure>::ObservationSpace: RLObservationSpace,
{
    type Agent = Box<DynFullAgent<E>>;

    fn build_agent(&self, env: &E, seed: u64) -> Result<Self::Agent, BuildAgentError> {
        use AgentDef::*;

        match self.0.borrow() {
            ResettingMeta(inner_agent_def) => ResettingMetaAgent::new(
                ForFiniteFinite::new(inner_agent_def.as_ref().clone()),
                (&InnerEnvStructure::<E, &E>::new(env)).into(),
                seed,
            )
            .map(|a| Box::new(a) as _),
            agent_def => ForAnyAny::new(agent_def).build_agent(env, seed),
        }
    }
}

impl<T, E, OS, AS> BuildManagerAgent<E> for ForMetaFiniteFinite<T>
where
    T: Borrow<MultiThreadAgentDef>,
    E: EnvStructure<ObservationSpace = MetaObservationSpace<OS, AS>, ActionSpace = AS> + ?Sized,
    OS: RLObservationSpace + FiniteSpace + Clone + Send + 'static,
    <OS as Space>::Element: Clone + Send, // ResettingMetaAgent: Send requires OS::Element: Send
    AS: RLActionSpace + FiniteSpace + Clone + Send + 'static,
    <AS as Space>::Element: Clone,
    // I think this ought to be inferrable but for whatever reason it isn't
    <E as EnvStructure>::ObservationSpace: RLObservationSpace,
{
    type ManagerAgent = Box<DynEnvManagerAgent<E>>;

    fn build_manager_agent(
        &self,
        env: &E,
        seed: u64,
    ) -> Result<Self::ManagerAgent, BuildAgentError> {
        use MultiThreadAgentDef::*;
        match self.0.borrow() {
            Mutex(config) => ForMetaFiniteFinite(config.borrow())
                .build_agent(env, seed)
                .map(|a| Box::new(BoxingManager::new(MutexAgentManager::new(a))) as _),
        }
    }
}

/// Send-able [`FullAgent`] trait object for an environment structure.
pub type DynFullAgent<E> = dyn FullAgent<
        <<E as EnvStructure>::ObservationSpace as Space>::Element,
        <<E as EnvStructure>::ActionSpace as Space>::Element,
    > + Send;

/// Send-able [`Agent`] trait object for an environment structure.
pub type DynEnvAgent<E> = dyn Agent<
        <<E as EnvStructure>::ObservationSpace as Space>::Element,
        <<E as EnvStructure>::ActionSpace as Space>::Element,
    > + Send;

/// The agent manager trait object for a given environment structure.
///
/// See also [`BoxingManager`](crate::agents::BoxingManager).
pub type DynEnvManagerAgent<E> = dyn ManagerAgent<Worker = Box<DynEnvAgent<E>>>;

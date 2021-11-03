//! Multi-thread agents
mod batch;
mod mutex;

pub use batch::{
    InitializeMultithreadBatchAgent, MultithreadBatchAgentConfig, MultithreadBatchManager,
    MultithreadBatchWorker,
};
pub use mutex::{MutexAgentConfig, MutexAgentInitializer, MutexAgentManager, MutexAgentWorker};

use super::{Agent, BuildAgentError};
use crate::envs::EnvStructure;
use crate::logging::TimeSeriesLogger;
use crate::spaces::Space;

/// Build a multithread agent initializer ([`InitializeMultithreadAgent`]).
pub trait BuildMultithreadAgent<OS: Space, AS: Space> {
    type MultithreadAgent: InitializeMultithreadAgent<OS::Element, AS::Element>;

    fn build_multithread_agent(
        &self,
        env: &dyn EnvStructure<ObservationSpace = OS, ActionSpace = AS>,
        seed: u64,
    ) -> Result<Self::MultithreadAgent, BuildAgentError>;
}

/// Initialize a multithread agent.
///
/// A multithread agent consists of a single manager and multiple workers.
/// The manager is run on the current thread while each worker is sent to run on its own thread.
/// The manager and workers are responsible for internaly coordinating updates and synchronization.
pub trait InitializeMultithreadAgent<O, A> {
    type Manager: MultithreadAgentManager;
    type Worker: Agent<O, A> + Send + 'static;

    /// Create a new worker instance.
    fn new_worker(&mut self) -> Self::Worker;

    /// Convert the initializer into the manager instance.
    fn into_manager(self) -> Self::Manager;
}

/// The manager part of a multithread agent.
pub trait MultithreadAgentManager {
    fn run(&mut self, logger: &mut dyn TimeSeriesLogger);
}

/// Try to convert into a stand-alone actor.
///
/// This is generally implemented for [`MultithreadAgentManager`].
pub trait TryIntoActor: Sized {
    type Actor;

    /// Try to convert the manager into a standalone actor, otherwise return self
    ///
    /// For multithread managers, this is likely to fail if any workers still exist.
    fn try_into_actor(self) -> Result<Self::Actor, Self>;
}

impl<T> MultithreadAgentManager for Box<T>
where
    T: MultithreadAgentManager + ?Sized,
{
    fn run(&mut self, logger: &mut dyn TimeSeriesLogger) {
        T::run(self, logger)
    }
}

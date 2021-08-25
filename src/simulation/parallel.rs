use super::hooks::SimulationHook;
use super::{run_agent, RunSimulation};
use crate::agents::{Agent, ManagerAgent};
use crate::envs::{EnvBuilder, StatefulEnvironment};
use crate::logging::TimeSeriesLogger;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};
use std::thread;

/// Configuration for [`MultiThreadSimulator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MultiThreadSimulatorConfig {
    pub num_workers: usize,
    // TODO: Add seed
}

impl Default for MultiThreadSimulatorConfig {
    fn default() -> Self {
        Self {
            num_workers: num_cpus::get(),
        }
    }
}

impl MultiThreadSimulatorConfig {
    pub const fn new(num_workers: usize) -> Self {
        Self { num_workers }
    }

    pub fn build_simulator<EB, E, MA, H>(
        &self,
        env_config: EB,
        manager_agent: MA,
        worker_hook: H,
    ) -> Box<dyn RunSimulation>
    where
        EB: EnvBuilder<E> + Send + Sync + 'static,
        E: StatefulEnvironment + 'static,
        MA: ManagerAgent + 'static,
        <MA as ManagerAgent>::Worker: Agent<<E as StatefulEnvironment>::Observation, <E as StatefulEnvironment>::Action>
            + 'static,
        <E as StatefulEnvironment>::Observation: Clone,
        H: SimulationHook<
                <E as StatefulEnvironment>::Observation,
                <E as StatefulEnvironment>::Action,
            > + Clone
            + Send
            + 'static,
    {
        Box::new(MultiThreadSimulator {
            env_builder: Arc::new(RwLock::new(env_config)),
            env_type: PhantomData::<*const E>,
            manager_agent,
            num_workers: self.num_workers,
            worker_hook,
        })
    }
}

/// Multi-thread simulator
pub struct MultiThreadSimulator<EB, E, MA, H> {
    env_builder: Arc<RwLock<EB>>,
    // *const E to avoid indicating ownership. See:
    // https://doc.rust-lang.org/std/marker/struct.PhantomData.html#ownership-and-the-drop-check
    env_type: PhantomData<*const E>,
    manager_agent: MA,
    num_workers: usize,
    worker_hook: H,
}

impl<EB, E, MA, H> RunSimulation for MultiThreadSimulator<EB, E, MA, H>
where
    EB: EnvBuilder<E> + Send + Sync + 'static,
    E: StatefulEnvironment,
    MA: ManagerAgent,
    <MA as ManagerAgent>::Worker: Agent<<E as StatefulEnvironment>::Observation, <E as StatefulEnvironment>::Action>
        + 'static,
    <E as StatefulEnvironment>::Observation: Clone,
    H: SimulationHook<<E as StatefulEnvironment>::Observation, <E as StatefulEnvironment>::Action>
        + Clone
        + Send
        + 'static,
{
    fn run_simulation(&mut self, logger: &mut dyn TimeSeriesLogger) {
        run_agent_multithread(
            &self.env_builder,
            &mut self.manager_agent,
            self.num_workers,
            &self.worker_hook,
            logger,
        );
    }
}

pub fn run_agent_multithread<EB, E, MA, H>(
    env_config: &Arc<RwLock<EB>>,
    agent_manager: &mut MA,
    num_workers: usize,
    worker_hook: &H,
    logger: &mut dyn TimeSeriesLogger,
) where
    EB: EnvBuilder<E> + Send + Sync + 'static,
    E: StatefulEnvironment,
    MA: ManagerAgent,
    <MA as ManagerAgent>::Worker: Agent<<E as StatefulEnvironment>::Observation, <E as StatefulEnvironment>::Action>
        + 'static,
    <E as StatefulEnvironment>::Observation: Clone,
    H: SimulationHook<<E as StatefulEnvironment>::Observation, <E as StatefulEnvironment>::Action>
        + Clone
        + Send
        + 'static,
{
    let mut worker_threads = vec![];
    for i in 0..num_workers {
        // TODO: Allow setting a seed
        let env_seed = 2 * u64::try_from(i).unwrap();
        let env_config_ = Arc::clone(env_config);
        let mut worker = agent_manager.make_worker(env_seed + 1);
        let mut hook = worker_hook.clone();
        worker_threads.push(thread::spawn(move || {
            let mut env: E = (*env_config_.read().unwrap()).build_env(env_seed).unwrap();
            drop(env_config_);
            run_agent(&mut env, &mut worker, &mut hook, &mut ());
        }));
    }

    agent_manager.run(logger);
    for thread in worker_threads.into_iter() {
        thread.join().unwrap();
    }
}
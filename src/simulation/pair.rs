use super::hooks::BuildSimulationHook;
use super::{run_agent, FullStep, Simulator, SimulatorError};
use crate::agents::{Actor, BuildAgent, SynchronousAgent};
use crate::envs::{BuildEnv, FirstPlayerView, SecondPlayerView, Successor};
use crate::logging::TimeSeriesLogger;
use crate::spaces::{Space, TupleSpace2};
use crossbeam_channel::{Receiver, Sender};
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::thread;

/// Simulate a two-agent environment.
///
/// Each agent is run on its own thread.
pub struct PairSimulator<EC, AC1, AC2, HC> {
    env_config: EC,
    first_agent_config: AC1,
    second_agent_config: AC2,
    hook_config: HC,
}

impl<EC, AC1, AC2, HC> PairSimulator<EC, AC1, AC2, HC> {
    pub const fn new(
        env_config: EC,
        first_agent_config: AC1,
        second_agent_config: AC2,
        hook_config: HC,
    ) -> Self {
        Self {
            env_config,
            first_agent_config,
            second_agent_config,
            hook_config,
        }
    }
}

impl<EC, AC1, AC2, OS1, OS2, AS1, AS2, HC> Simulator for PairSimulator<EC, AC1, AC2, HC>
where
    EC: BuildEnv<
        Observation = (OS1::Element, OS2::Element),
        Action = (AS1::Element, AS2::Element),
        ObservationSpace = TupleSpace2<OS1, OS2>,
        ActionSpace = TupleSpace2<AS1, AS2>,
    >,
    EC::Observation: Clone,
    AC1: BuildAgent<OS1, AS1>,
    AC1::Agent: Send + 'static,
    AC2: BuildAgent<OS2, AS2>,
    AC2::Agent: Send + 'static,
    OS1: Space,
    OS1::Element: 'static,
    OS2: Space,
    OS2::Element: 'static,
    AS1: Space,
    AS1::Element: 'static,
    AS2: Space,
    AS2::Element: 'static,
    HC: BuildSimulationHook<EC::ObservationSpace, EC::ActionSpace>,
{
    fn run_simulation(
        &self,
        env_seed: u64,
        agent_seed: u64,
        logger: &mut dyn TimeSeriesLogger,
    ) -> Result<(), SimulatorError> {
        let mut agent_seed_rng = StdRng::seed_from_u64(agent_seed);
        let mut env = self.env_config.build_env(env_seed)?;
        let (first_remote, first_worker) = RemoteAgent::from_agent(
            self.first_agent_config
                .build_agent(&FirstPlayerView::new(&env), agent_seed_rng.gen())?,
        );
        thread::spawn(first_worker);
        let (second_remote, second_worker) = RemoteAgent::from_agent(
            self.second_agent_config
                .build_agent(&SecondPlayerView::new(&env), agent_seed_rng.gen())?,
        );
        thread::spawn(second_worker);

        let mut agent = PairAgent(first_remote, second_remote);
        let mut hook = self.hook_config.build_hook(&env, 1, 0);

        run_agent(&mut env, &mut agent, &mut hook, logger);

        Ok(())
    }
}

/// Combine two agents into a single joint agent with pair observation and action spaces.
struct PairAgent<T1, T2>(pub T1, pub T2);
impl<T1, T2, O1, O2, A1, A2> Actor<(O1, O2), (A1, A2)> for PairAgent<T1, T2>
where
    T1: Actor<O1, A1>,
    T2: Actor<O2, A2>,
{
    fn act(&mut self, observation: &(O1, O2)) -> (A1, A2) {
        (self.0.act(&observation.0), self.1.act(&observation.1))
    }

    fn reset(&mut self) {
        self.0.reset();
        self.1.reset();
    }
}
impl<T1, T2, O1, O2, A1, A2> SynchronousAgent<(O1, O2), (A1, A2)> for PairAgent<T1, T2>
where
    T1: SynchronousAgent<O1, A1>,
    T2: SynchronousAgent<O2, A2>,
{
    fn update(&mut self, step: FullStep<(O1, O2), (A1, A2)>, logger: &mut dyn TimeSeriesLogger) {
        let (o1, o2) = step.observation;
        let (a1, a2) = step.action;
        let (n1, n2) = match step.next {
            Successor::Continue((no1, no2)) => (Successor::Continue(no1), Successor::Continue(no2)),
            Successor::Terminate => (Successor::Terminate, Successor::Terminate),
            Successor::Interrupt((no1, no2)) => {
                (Successor::Interrupt(no1), Successor::Interrupt(no2))
            }
        };
        self.0.update(
            FullStep {
                observation: o1,
                action: a1,
                reward: step.reward,
                next: n1,
            },
            logger,
        );
        self.1.update(
            FullStep {
                observation: o2,
                action: a2,
                reward: step.reward,
                next: n2,
            },
            logger,
        );
    }
}

/// Interface to a remote agent accessible via channels.
struct RemoteAgent<O, A> {
    sender: Sender<Message<O, A>>,
    receiver: Receiver<A>,
}

enum Message<O, A> {
    Act(O),
    Reset,
    Update(FullStep<O, A>),
}

impl<O, A> RemoteAgent<O, A> {
    /// Create a remote agent and a worker closure from an agent.
    fn from_agent<T>(mut agent: T) -> (Self, impl FnMut())
    where
        T: SynchronousAgent<O, A> + Send,
    {
        let (send_msg, recv_msg) = crossbeam_channel::bounded(0);
        let (send_act, recv_act) = crossbeam_channel::bounded(0);
        let remote = Self {
            sender: send_msg,
            receiver: recv_act,
        };
        let worker = move || {
            while let Ok(msg) = recv_msg.recv() {
                match msg {
                    Message::Act(obs) => {
                        let action = agent.act(&obs);
                        send_act.send(action).unwrap();
                    }
                    Message::Reset => agent.reset(),
                    Message::Update(step) => {
                        agent.update(step, &mut ());
                    }
                }
            }
        };
        (remote, worker)
    }
}

impl<O, A> Actor<O, A> for RemoteAgent<O, A>
where
    O: Clone,
{
    fn act(&mut self, observation: &O) -> A {
        self.sender.send(Message::Act(observation.clone())).unwrap();
        self.receiver.recv().unwrap()
    }

    fn reset(&mut self) {
        self.sender.send(Message::Reset).unwrap();
    }
}

impl<O, A> SynchronousAgent<O, A> for RemoteAgent<O, A>
where
    O: Clone,
{
    fn update(&mut self, step: FullStep<O, A>, _logger: &mut dyn TimeSeriesLogger) {
        self.sender.send(Message::Update(step)).unwrap();
    }
}

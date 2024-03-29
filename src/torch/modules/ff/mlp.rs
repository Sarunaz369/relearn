//! Multi-layer perceptron
use super::super::{
    BuildModule, Forward, Module, ModuleExtras, SeqIterative, SeqPacked, SeqSerial,
};
use super::{Activation, Linear, LinearConfig};
use crate::torch::packed::PackedTensor;
use serde::{Deserialize, Serialize};
use std::iter::{self, FlatMap};
use std::slice;
use tch::{Device, Tensor};

/// Configuration for the [`Mlp`] module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MlpConfig {
    /// Sizes of the hidden layers
    pub hidden_sizes: Vec<usize>,
    /// Activation function between hidden layers.
    pub activation: Activation,
    /// Activation function on the output.
    pub output_activation: Activation,
    /// Configuration for the linear layers
    pub linear_config: LinearConfig,
}

impl Default for MlpConfig {
    fn default() -> Self {
        Self {
            hidden_sizes: vec![128],
            activation: Activation::Relu,
            output_activation: Activation::Identity,
            linear_config: LinearConfig::default(),
        }
    }
}

impl BuildModule for MlpConfig {
    type Module = Mlp;

    fn build_module(&self, in_dim: usize, out_dim: usize, device: Device) -> Self::Module {
        Mlp::new(in_dim, out_dim, device, self)
    }
}

/// Multi-layer perceptron
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Mlp {
    layers: Vec<Linear>,
    activation: Activation,
    output_activation: Activation,
}

impl Mlp {
    #[must_use]
    pub fn new(in_dim: usize, out_dim: usize, device: Device, config: &MlpConfig) -> Self {
        let in_dims = iter::once(&in_dim).chain(&config.hidden_sizes);
        let out_dims = config.hidden_sizes.iter().chain(iter::once(&out_dim));

        let layers: Vec<_> = in_dims
            .zip(out_dims)
            .map(|(in_, out_)| Linear::new(*in_, *out_, device, &config.linear_config))
            .collect();

        Self {
            layers,
            activation: config.activation,
            output_activation: config.output_activation,
        }
    }
}

impl Module for Mlp {
    fn shallow_clone(&self) -> Self
    where
        Self: Sized,
    {
        Self {
            layers: self.layers.iter().map(Module::shallow_clone).collect(),
            ..*self
        }
    }

    fn clone_to_device(&self, device: Device) -> Self
    where
        Self: Sized,
    {
        Self {
            layers: self
                .layers
                .iter()
                .map(|l| l.clone_to_device(device))
                .collect(),
            ..*self
        }
    }

    #[inline]
    fn variables(&self) -> Box<dyn Iterator<Item = &Tensor> + '_> {
        Box::new(ModuleExtras::variables(self))
    }

    #[inline]
    fn trainable_variables(&self) -> Box<dyn Iterator<Item = &Tensor> + '_> {
        Box::new(ModuleExtras::trainable_variables(self))
    }

    fn has_cudnn_second_derivatives(&self) -> bool {
        self.layers.iter().all(Linear::has_cudnn_second_derivatives)
    }
}

impl<'a> ModuleExtras<'a> for Mlp {
    #[allow(clippy::type_complexity)]
    type Variables = FlatMap<
        slice::Iter<'a, Linear>,
        <Linear as ModuleExtras<'a>>::Variables,
        fn(&'a Linear) -> <Linear as ModuleExtras<'a>>::Variables,
    >;
    #[allow(clippy::type_complexity)]
    type TrainableVariables = FlatMap<
        slice::Iter<'a, Linear>,
        <Linear as ModuleExtras<'a>>::TrainableVariables,
        fn(&'a Linear) -> <Linear as ModuleExtras<'a>>::TrainableVariables,
    >;

    #[inline]
    fn variables(&'a self) -> Self::Variables {
        self.layers.iter().flat_map(ModuleExtras::variables)
    }

    #[inline]
    fn trainable_variables(&'a self) -> Self::TrainableVariables {
        self.layers
            .iter()
            .flat_map(ModuleExtras::trainable_variables)
    }
}

impl Forward for Mlp {
    fn forward(&self, input: &Tensor) -> Tensor {
        let mut iter_layers = self.layers.iter();
        let mut hidden = iter_layers
            .next()
            .expect("must have >= 1 layers by construction")
            .forward(input);
        for layer in iter_layers {
            hidden = self.activation.forward_owned(hidden);
            hidden = layer.forward(&hidden);
        }
        hidden = self.output_activation.forward_owned(hidden);
        hidden
    }
}

/// Sequence processing by batching over the sequence dimension.
impl SeqSerial for Mlp {
    fn seq_serial(&self, inputs: &Tensor, _seq_lengths: &[usize]) -> Tensor {
        self.forward(inputs)
    }
}

/// Sequence processing by batching over the sequence dimension.
impl SeqPacked for Mlp {
    fn seq_packed(&self, inputs: &PackedTensor) -> PackedTensor {
        inputs.batch_map_ref(|tensor| self.forward(tensor))
    }
}

/// Iterate over a sequence by independently and identically transforming each step.
impl SeqIterative for Mlp {
    type State = ();
    fn initial_state(&self) -> Self::State {}
    fn step(&self, _: &mut Self::State, input: &Tensor) -> Tensor {
        self.forward(input)
    }
}

#[cfg(test)]
// Confusion with rstest hack when passing the _runner arg
#[allow(
    clippy::needless_pass_by_value,
    clippy::used_underscore_binding,
    clippy::no_effect_underscore_binding
)]
mod tests {
    use super::super::super::testing::{
        self, RunForward, RunIterStep, RunModule, RunSeqPacked, RunSeqSerial,
    };
    use super::*;
    use rstest::{fixture, rstest};
    use tch::{kind::Kind, Device};

    #[fixture]
    fn default_module() -> (Mlp, usize, usize) {
        let in_dim = 3;
        let out_dim = 2;
        let config = MlpConfig::default();
        let module = config.build_module(in_dim, out_dim, Device::Cpu);
        (module, in_dim, out_dim)
    }

    #[rstest]
    fn forward_batch(default_module: (Mlp, usize, usize)) {
        let (module, in_dim, out_dim) = default_module;
        testing::check_forward(&module, in_dim, out_dim, &[4], Kind::Float);
    }

    #[rstest]
    fn seq_serial(default_module: (Mlp, usize, usize)) {
        let (module, in_dim, out_dim) = default_module;
        testing::check_seq_serial(&module, in_dim, out_dim);
    }

    #[rstest]
    fn seq_packed(default_module: (Mlp, usize, usize)) {
        let (module, in_dim, out_dim) = default_module;
        testing::check_seq_packed(&module, in_dim, out_dim);
    }

    #[rstest]
    fn seq_step(default_module: (Mlp, usize, usize)) {
        let (module, in_dim, out_dim) = default_module;
        testing::check_step(&module, in_dim, out_dim);
    }

    #[rstest]
    fn seq_consistent(default_module: (Mlp, usize, usize)) {
        let (module, in_dim, out_dim) = default_module;
        testing::check_seq_packed_matches_iter_steps(&module, in_dim, out_dim);
    }

    #[rstest]
    #[case::forward(RunForward)]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn gradient_descent<R: RunModule<Mlp>>(#[case] _runner: R) {
        testing::check_config_gradient_descent::<R, _>(&MlpConfig::default());
    }

    #[rstest]
    #[case::forward(RunForward)]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn clone_to_new_device<R: RunModule<Mlp>>(#[case] _runner: R) {
        testing::check_config_clone_to_new_device::<R, _>(&MlpConfig::default());
    }

    #[test]
    fn clone_to_same_device() {
        testing::check_config_clone_to_same_device::<RunForward, _>(&MlpConfig::default());
    }

    #[rstest]
    #[case::forward(RunForward)]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn ser_de_matches<R: RunModule<Mlp>>(#[case] _runner: R, default_module: (Mlp, usize, usize)) {
        let (module, in_dim, _) = default_module;
        testing::check_ser_de_matches::<R, _>(&module, in_dim);
    }

    #[rstest]
    fn variables_count(default_module: (Mlp, usize, usize)) {
        let (module, _, _) = default_module;
        assert_eq!(Module::variables(&module).count(), 4);
    }

    #[rstest]
    fn trainable_variables_count(default_module: (Mlp, usize, usize)) {
        let (module, _, _) = default_module;
        assert_eq!(Module::trainable_variables(&module).count(), 4);
    }
}

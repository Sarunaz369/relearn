//! Recurrent neural networks
mod gru;
mod lstm;

pub use gru::{Gru, GruConfig};
pub use lstm::{Lstm, LstmConfig};

use super::super::super::initializers::{Initializer, VarianceScale};
use super::super::{BuildModule, IterativeModule, Module};
use smallvec::SmallVec;
use std::marker::PhantomData;
use tch::{nn::Path, Cuda, Device, Tensor};

/// Basic recurrent neural network configuration
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RnnBaseConfig<T> {
    /// Number of layers; each has size equal to the output size when built.
    pub num_layers: usize,
    /// Initialization for input-to-hidden weight matrices
    pub input_weights_init: Initializer,
    /// Initialization for hidden-to-hidden weight matrices
    pub hidden_weights_init: Initializer,
    /// Initialization for bias vectors. None if there should be no bias terms.
    pub bias_init: Option<Initializer>,
    /// Phantom marker for the specific RNN implementation (`RNNImpl`, `GRUImpl`, `LSTMImpl`, etc)
    pub impl_: PhantomData<fn() -> T>,
}

impl<T> Default for RnnBaseConfig<T> {
    fn default() -> Self {
        Self {
            num_layers: 1,
            // Default initialization follows the Tensorflow RNN implementation as it seems the
            // most considered. The PyTorch RNN initializes all weights and biases in the same way.
            input_weights_init: Initializer::Uniform(VarianceScale::FanAvg),
            hidden_weights_init: Initializer::Orthogonal,
            bias_init: Some(Initializer::Zeros),
            impl_: PhantomData,
        }
    }
}

impl<T: RnnImpl> BuildModule for RnnBaseConfig<T> {
    type Module = RnnBase<T>;

    fn build_module(&self, vs: &Path, in_dim: usize, out_dim: usize) -> Self::Module {
        RnnBase::new(vs, in_dim, out_dim, self)
    }
}

pub trait RnnImpl {
    /// State for one cell (single iterative layer)
    type CellState;

    /// cuDNN RNN mode code
    ///
    /// See <https://github.com/pytorch/pytorch/blob/d6909732954ad182d13fa8ab9959502a386e9d3a/torch/csrc/api/src/nn/modules/rnn.cpp#L29>
    ///
    /// * `RnnRelu` - `0`
    /// * `RnnTanh` - `1`
    /// * `Lstm` - 2
    /// * `Gru` - 3
    const CUDNN_MODE: u32;

    /// Number of gates per hidden unit
    const GATES_MULTIPLE: u32;

    fn initial_cell_state(rnn: &RnnBase<Self>, batch_size: i64) -> Self::CellState
    where
        Self: Sized;

    fn cell_batch_step(
        rnn: &RnnBase<Self>,
        state: &mut Self::CellState,
        weights: &RnnLayerWeights,
        batch_input: &Tensor,
    ) -> Tensor
    where
        Self: Sized;
}

#[derive(Debug)]
pub struct RnnBase<T> {
    weights: RnnWeights,
    hidden_size: i64,
    dropout: f64,
    device: Device,
    type_: PhantomData<fn() -> T>,
}

impl<T: RnnImpl> RnnBase<T> {
    pub fn new(vs: &Path, in_dim: usize, out_dim: usize, config: &RnnBaseConfig<T>) -> Self {
        Self {
            weights: RnnWeights::new(vs, in_dim, out_dim, config),
            hidden_size: out_dim.try_into().unwrap(),
            dropout: 0.0,
            device: vs.device(),
            type_: PhantomData,
        }
    }
}

impl<T> Module for RnnBase<T> {
    fn has_cudnn_second_derivatives(&self) -> bool {
        false
    }
}

impl<T: RnnImpl> IterativeModule for RnnBase<T> {
    // Hold up to 4 layers without allocationg
    type State = SmallVec<[T::CellState; 4]>;

    fn initial_state(&self) -> Self::State {
        let batch_size = 1;
        (0..self.weights.num_layers())
            .map(|_| T::initial_cell_state(self, batch_size))
            .collect()
    }

    fn step(&self, state: &mut Self::State, input: &Tensor) -> Tensor {
        let mut hidden = input.unsqueeze(0);
        for (layer_weights, layer_state) in self.weights.layers().zip(state) {
            hidden = T::cell_batch_step(self, layer_state, &layer_weights, &hidden);
        }
        hidden.squeeze_dim(0)
    }
}

#[derive(Debug)]
struct RnnWeights {
    flat_weights: Vec<Tensor>,
    has_biases: bool,
}

impl RnnWeights {
    /// Initialize [`RnnWeights`].
    ///
    /// # Reference Initialization Strategies
    /// ## Pytorch
    /// Initializes all weights and biases from `U(-lim, lim)` where `lim = 1 / sqrt(hidden_dim)`.
    /// [Source](https://github.com/pytorch/pytorch/blob/5a04bd87233b5391a9fe471fadac5a3edc128e05/torch/csrc/api/src/nn/modules/rnn.cpp#L677-L683).
    ///
    /// ## Tensorflow
    /// By default, initializes as:
    /// * Input-to-hidden weights: Glorot Uniform (aka Xavier)
    /// * Hidden-to-hidden weights: Orthogonal
    /// * Biases: Zero
    /// [GRU](https://github.com/keras-team/keras/blob/d8fcb9d4d4dad45080ecfdd575483653028f8eda/keras/layers/recurrent.py#L1771-L1773).
    /// [LSTM](https://github.com/keras-team/keras/blob/d8fcb9d4d4dad45080ecfdd575483653028f8eda/keras/layers/recurrent.py#L2334-L2336).
    ///
    /// The weight matrices for the separate gates are initialized as a single large matrix with
    /// output dimension `K * hidden_dim` (`K = 3` for GRU). As far as I can tell, this is not
    /// accounted for in the initialization. Consequently, the Glorot Uniform distribution is
    /// `U(-lim, lim)` where
    /// `lim = sqrt(6 / (fan_in + fan_out)) = sqrt(6 / (in_dim + K * hidden_dim))`.
    ///
    /// ## Tch
    /// Initializes as:
    /// * Weights: `U(-lim, lim)` where `lim = 1 / sqrt(fan_in)`.
    ///     (Named Kaiming Uniform but missing factor of `sqrt(3)`).
    /// * Biases: Zero.
    /// [Source](https://docs.rs/tch/0.6.1/src/tch/nn/rnn.rs.html#210).
    ///
    pub fn new<T: RnnImpl>(
        vs: &Path,
        in_dim: usize,
        out_dim: usize,
        config: &RnnBaseConfig<T>,
    ) -> Self {
        let in_dim: i64 = in_dim.try_into().unwrap();
        let hidden_size: i64 = out_dim.try_into().unwrap();
        let gates_size = hidden_size * i64::from(T::GATES_MULTIPLE);

        let mut flat_weights = Vec::new();
        for i in 0..config.num_layers {
            let layer_input_size = if i == 0 { in_dim } else { hidden_size };
            // TODO: Use the same initialization as PyTorch
            // <https://pytorch.org/docs/stable/_modules/torch/nn/modules/rnn.html>
            flat_weights.push(config.input_weights_init.add_tensor(
                vs,
                &format!("weight_ih_l{}", i),
                &[gates_size, layer_input_size],
                1.0,
                None,
            ));
            flat_weights.push(config.hidden_weights_init.add_tensor(
                vs,
                &format!("weight_hh_l{}", i),
                &[gates_size, hidden_size],
                1.0,
                None,
            ));

            if let Some(bias_init) = config.bias_init {
                flat_weights.push(bias_init.add_tensor(
                    vs,
                    &format!("bias_ih_l{}", i),
                    &[gates_size],
                    1.0,
                    None,
                ));
                flat_weights.push(bias_init.add_tensor(
                    vs,
                    &format!("bias_hh_l{}", i),
                    &[gates_size],
                    1.0,
                    None,
                ));
            }
        }

        if vs.device().is_cuda() && Cuda::cudnn_is_available() {
            // Flatten the weights in-place
            // <https://github.com/pytorch/pytorch/blob/5a04bd87233b5391a9fe471fadac5a3edc128e05/torch/csrc/api/src/nn/modules/rnn.cpp#L159-L221>
            let _no_grad = tch::no_grad_guard();
            let _ = Tensor::internal_cudnn_rnn_flatten_weight(
                &flat_weights,
                if config.bias_init.is_some() { 4 } else { 2 },
                in_dim,
                T::CUDNN_MODE.into(),
                hidden_size,
                0,                        // No projections
                config.num_layers as i64, // Num layers
                true,                     // Batch first
                false,                    // Not bidirectional
            );
        }
        Self {
            flat_weights,
            has_biases: config.bias_init.is_some(),
        }
    }

    /// All weights an array
    pub fn flat_weights(&self) -> &[Tensor] {
        &self.flat_weights
    }

    pub fn num_layers(&self) -> usize {
        assert_eq!(self.flat_weights.len() % self.weights_per_layer(), 0);
        self.flat_weights.len() / self.weights_per_layer()
    }

    pub const fn weights_per_layer(&self) -> usize {
        if self.has_biases {
            4
        } else {
            2
        }
    }

    pub fn layers(&self) -> impl Iterator<Item = RnnLayerWeights<'_>> {
        self.flat_weights
            .chunks_exact(self.weights_per_layer())
            .map(|weights| RnnLayerWeights {
                weights,
                has_biases: self.has_biases,
            })
    }
}

pub struct RnnLayerWeights<'a> {
    weights: &'a [Tensor],
    has_biases: bool,
}

impl<'a> RnnLayerWeights<'a> {
    pub const fn w_ih(&self) -> &Tensor {
        &self.weights[0]
    }

    pub const fn w_hh(&self) -> &Tensor {
        &self.weights[1]
    }

    pub const fn b_ih(&self) -> Option<&Tensor> {
        if self.has_biases {
            Some(&self.weights[2])
        } else {
            None
        }
    }

    pub const fn b_hh(&self) -> Option<&Tensor> {
        if self.has_biases {
            Some(&self.weights[3])
        } else {
            None
        }
    }
}

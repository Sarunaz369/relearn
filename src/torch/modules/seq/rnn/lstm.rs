//! Long Short-Term Memory
use super::super::super::{SeqPacked, SeqSerial};
use super::super::seq_serial_map;
use super::{RnnBase, RnnBaseConfig, RnnImpl, RnnLayerWeights};
use crate::torch::packed::PackedTensor;
use serde::{Deserialize, Serialize};
use tch::{Kind, Tensor};

/// Configuration for [`Lstm`]
pub type LstmConfig = RnnBaseConfig<LstmImpl>;

/// Long Short-Term Memory Module
pub type Lstm = RnnBase<LstmImpl>;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LstmImpl;
impl RnnImpl for LstmImpl {
    type CellState = (Tensor, Tensor);

    const CUDNN_MODE: u32 = 2;
    const GATES_MULTIPLE: usize = 4;

    fn initial_cell_state(rnn: &RnnBase<Self>, batch_size: usize) -> Self::CellState {
        let hidden_state = Tensor::zeros(
            &[batch_size as i64, rnn.hidden_size as i64],
            (Kind::Float, rnn.device),
        );

        let cell_state = hidden_state.shallow_clone();
        (hidden_state, cell_state)
    }

    fn cell_batch_step(
        _: &RnnBase<Self>,
        state: &mut Self::CellState,
        w: &RnnLayerWeights,
        batch_input: &Tensor,
    ) -> Tensor {
        let (ref hidden_state, ref cell_state) = state;
        let (new_hidden_state, new_cell_state) = batch_input.lstm_cell(
            &[hidden_state, cell_state],
            w.w_ih(),
            w.w_hh(),
            w.b_ih(),
            w.b_hh(),
        );
        *state = (new_hidden_state, new_cell_state);
        state.0.shallow_clone()
    }
}

impl SeqSerial for Lstm {
    fn seq_serial(&self, inputs: &Tensor, seq_lengths: &[usize]) -> Tensor {
        let shape = inputs.size();
        assert_eq!(
            shape.len(),
            3,
            "Input must have 3 dimensions: [BATCH_SIZE, SEQ_LEN, NUM_FEATURES]"
        );
        let num_layers = self.weights.num_layers() as i64;
        let batch_size = shape[0] as i64;
        let zeros = Tensor::zeros(
            &[num_layers, batch_size, self.hidden_size as i64],
            (inputs.kind(), inputs.device()),
        );
        let initial_state = [zeros.shallow_clone(), zeros];

        seq_serial_map(inputs, seq_lengths, |seq_input| {
            let (seq_output, _, _) = seq_input.lstm(
                &initial_state,
                self.weights.flat_weights(),
                self.weights.has_biases,
                num_layers,
                self.dropout,
                true,  // train
                false, // bidirectional
                true,  // batch_first
            );
            seq_output
        })
    }
}

impl SeqPacked for Lstm {
    fn seq_packed(&self, inputs: &PackedTensor) -> PackedTensor {
        let initial_batch_size = match inputs.first_batch_size() {
            Some(size) => size,
            None => {
                return inputs.clone();
            }
        };
        let num_layers: i64 = self.weights.num_layers() as i64;
        let zeros = Tensor::zeros(
            &[num_layers, initial_batch_size, self.hidden_size as i64],
            (inputs.kind(), inputs.device()),
        );
        let initial_state = [zeros.shallow_clone(), zeros];
        let batch_sizes = inputs.batch_sizes_tensor();

        let lstm = |input_tensor| {
            let (outputs, _, _) = Tensor::lstm_data(
                input_tensor,
                &batch_sizes,
                &initial_state,
                self.weights.flat_weights(),
                self.weights.has_biases,
                num_layers,
                self.dropout,
                true,  // train
                false, // bidirectional
            );
            outputs
        };
        inputs.batch_map_ref(lstm)
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
    use super::super::super::super::testing::{
        self, RunIterStep, RunModule, RunSeqPacked, RunSeqSerial,
    };
    use super::super::super::super::Module;
    use super::*;
    use rstest::{fixture, rstest};
    use tch::Device;

    #[fixture]
    fn lstm() -> (Lstm, usize, usize) {
        let in_dim: usize = 3;
        let out_dim: usize = 2;
        let lstm = Lstm::new(in_dim, out_dim, Device::Cpu, &LstmConfig::default());
        (lstm, in_dim, out_dim)
    }

    #[rstest]
    fn seq_serial(lstm: (Lstm, usize, usize)) {
        let (lstm, in_dim, out_dim) = lstm;
        testing::check_seq_serial(&lstm, in_dim, out_dim);
    }

    #[rstest]
    fn seq_packed(lstm: (Lstm, usize, usize)) {
        let (lstm, in_dim, out_dim) = lstm;
        testing::check_seq_packed(&lstm, in_dim, out_dim);
    }

    #[rstest]
    fn seq_step(lstm: (Lstm, usize, usize)) {
        let (lstm, in_dim, out_dim) = lstm;
        testing::check_step(&lstm, in_dim, out_dim);
    }

    #[rstest]
    fn seq_packed_matches_iter_steps(lstm: (Lstm, usize, usize)) {
        let (lstm, in_dim, out_dim) = lstm;
        testing::check_seq_packed_matches_iter_steps(&lstm, in_dim, out_dim);
    }

    #[test]
    fn seq_packed_matches_iter_steps_2layers() {
        let in_dim: usize = 3;
        let out_dim: usize = 2;
        let config = LstmConfig {
            num_layers: 2,
            ..LstmConfig::default()
        };
        let lstm = Lstm::new(in_dim, out_dim, Device::Cpu, &config);
        testing::check_seq_packed_matches_iter_steps(&lstm, in_dim, out_dim);
    }

    #[test]
    fn seq_packed_matches_iter_steps_nobias() {
        let in_dim: usize = 3;
        let out_dim: usize = 2;
        let config = LstmConfig {
            bias_init: None,
            ..LstmConfig::default()
        };
        let lstm = Lstm::new(in_dim, out_dim, Device::Cpu, &config);
        testing::check_seq_packed_matches_iter_steps(&lstm, in_dim, out_dim);
    }

    #[rstest]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn gradient_descent<R: RunModule<Lstm>>(#[case] _runner: R) {
        testing::check_config_gradient_descent::<R, _>(&LstmConfig::default());
    }

    #[rstest]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn clone_to_new_device<R: RunModule<Lstm>>(#[case] _runner: R) {
        testing::check_config_clone_to_new_device::<R, _>(&LstmConfig::default());
    }

    #[test]
    fn clone_to_same_device() {
        testing::check_config_clone_to_same_device::<RunSeqPacked, _>(&LstmConfig::default());
    }

    #[rstest]
    #[case::seq_serial(RunSeqSerial)]
    #[case::seq_packed(RunSeqPacked)]
    #[case::iter_step(RunIterStep)]
    fn ser_de_matches<R: RunModule<Lstm>>(#[case] _runner: R, lstm: (Lstm, usize, usize)) {
        let (module, in_dim, _) = lstm;
        testing::check_ser_de_matches::<R, _>(&module, in_dim);
    }

    #[rstest]
    fn variables_count(lstm: (Lstm, usize, usize)) {
        let (lstm, _, _) = lstm;
        assert_eq!(lstm.variables().count(), 4);
    }

    #[rstest]
    fn trainable_variables_count(lstm: (Lstm, usize, usize)) {
        let (lstm, _, _) = lstm;
        assert_eq!(lstm.trainable_variables().count(), 4);
    }
}

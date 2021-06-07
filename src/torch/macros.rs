macro_rules! box_impl_sequence_module {
    ($type:ty) => {
        impl SequenceModule for Box<$type> {
            fn seq_serial(&self, inputs: &Tensor, seq_lengths: &[usize]) -> Tensor {
                self.as_ref().seq_serial(inputs, seq_lengths)
            }

            fn seq_packed(&self, inputs: &Tensor, batch_sizes: &Tensor) -> Tensor {
                self.as_ref().seq_packed(inputs, batch_sizes)
            }
        }
    };
}

macro_rules! box_impl_stateful_iterative_module {
    ($type:ty) => {
        impl StatefulIterativeModule for Box<$type> {
            fn step(&mut self, input: &Tensor) -> Tensor {
                self.as_mut().step(input)
            }

            fn reset(&mut self) {
                self.as_mut().reset()
            }
        }
    };
}

macro_rules! box_impl_cudnn_support {
    ($type:ty) => {
        impl CudnnSupport for Box<$type> {
            fn has_cudnn_second_derivatives(&self) -> bool {
                self.as_ref().has_cudnn_second_derivatives()
            }
        }
    };
}
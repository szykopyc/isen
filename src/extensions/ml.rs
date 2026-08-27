#[cfg(feature = "ml-kernels")]
use crate::{Ty, extensions::extension_array as kernels};

pub(crate) fn register(registry: &mut crate::native::NativeRegistry) {
    #[cfg(not(feature = "ml-kernels"))]
    let _ = registry;

    #[cfg(feature = "ml-kernels")]
    {
        use crate::native::{
            NativeExpected as Expected, NativeProduced as Produced,
            NativeRuntimeFunction as RuntimeFunction,
            NativeRuntimeSpace as Space, NativeSignature as Signature,
        };
        registry.add_runtime(Space {
            name: "ML",
            functions: &[
                RuntimeFunction { name: "mlp_forward", call: kernels::array_mlp_forward },
                RuntimeFunction { name: "sampled_update", call: kernels::array_sampled_update },
                RuntimeFunction { name: "sampled_softmax_update", call: kernels::array_sampled_softmax_update },
                RuntimeFunction { name: "mlp_backprop", call: kernels::array_mlp_backprop },
                RuntimeFunction { name: "softmax_sample", call: kernels::array_softmax_sample },
                RuntimeFunction { name: "gru_forward", call: kernels::array_gru_forward },
                RuntimeFunction { name: "gru_backprop", call: kernels::array_gru_backprop },
            ],
            signatures: || [
                ("mlp_forward", 12, Ty::Unit),
                ("sampled_update", 8, Ty::Float),
                ("sampled_softmax_update", 8, Ty::Float),
                ("mlp_backprop", 14, Ty::Unit),
                ("softmax_sample", 8, Ty::Int),
                ("gru_forward", 17, Ty::Unit),
                ("gru_backprop", 19, Ty::Unit),
            ].into_iter().map(|(name, arity, result)| Signature::custom(name, vec![Expected::Any; arity], Produced::Exact(result))).collect(),
        });
    }
}

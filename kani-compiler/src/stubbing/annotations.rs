use rustc_data_structures::fx::FxHashMap;
use rustc_driver::RunCompiler;
use rustc_driver::{Callbacks, Compilation};
use rustc_errors::ErrorGuaranteed;
use rustc_interface::interface::Compiler;
use rustc_interface::Queries;

use crate::attributes::{extract_path_arguments, partition_kanitool_attributes};

pub struct AnnotationCollector<'a> {
    rustc_args: &'a [String],
}

impl<'a> AnnotationCollector<'a> {
    pub fn new(rustc_args: &'a [String]) -> AnnotationCollector<'a> {
        AnnotationCollector { rustc_args }
    }

    pub fn run(&self) -> Result<FxHashMap<String, FxHashMap<String, String>>, ErrorGuaranteed> {
        let mut callbacks = AnnotationCollectorCallbacks::new();
        let compiler = RunCompiler::new(self.rustc_args, &mut callbacks);
        compiler.run().and_then(|_| Ok(callbacks.stub_mapping))
    }
}

struct AnnotationCollectorCallbacks {
    stub_mapping: FxHashMap<String, FxHashMap<String, String>>,
}

impl AnnotationCollectorCallbacks {
    pub fn new() -> AnnotationCollectorCallbacks {
        AnnotationCollectorCallbacks { stub_mapping: FxHashMap::default() }
    }
}

impl Callbacks for AnnotationCollectorCallbacks {
    fn after_expansion<'tcx>(
        &mut self,
        _compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        queries.global_ctxt().unwrap().peek_mut().enter(|tcx| {
            for item in tcx.hir_crate_items(()).items() {
                let def_id = item.def_id.to_def_id();
                let (proof, other) = partition_kanitool_attributes(tcx.get_attrs_unchecked(def_id));
                if proof.is_empty() {
                    continue;
                }
                let mut stub_pairs = FxHashMap::default();
                for (name, attr) in other {
                    if name == "stub_by" {
                        if let Some(args) = extract_path_arguments(attr) {
                            if args.len() == 2 {
                                let other = stub_pairs.insert(args[0].clone(), args[1].clone());
                                if let Some(other) = other {
                                    tcx.sess.span_err(
                                        attr.span,
                                        format!(
                                            "duplicate stub mapping: {} mapped to {} AND {}",
                                            args[0], args[1], other
                                        ),
                                    );
                                }
                            } else {
                                tcx.sess
                                    .span_err(attr.span, "kani::stub_by takes two path arguments");
                            }
                        } else {
                            tcx.sess.span_err(attr.span, "kani::stub_by takes two path arguments");
                        }
                    }
                }
                self.stub_mapping.insert(tcx.def_path_str(def_id), stub_pairs);
            }
            tcx.sess.abort_if_errors();
            Compilation::Stop
        })
    }
}

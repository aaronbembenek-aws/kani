// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use rustc_hir::def_id::{DefId, LocalDefId};
use std::collections::HashMap;
use std::sync::RwLock;

static STUB_MAPPING: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);

pub struct StubbingPass {}

impl StubbingPass {
    pub fn new() -> StubbingPass {
        Self {}
    }

    pub fn run_pass<'tcx>(
        &self,
        tcx: rustc_middle::ty::TyCtxt<'tcx>,
        def_id: DefId,
        body: &mut rustc_middle::mir::Body<'tcx>,
    ) {
        let guard = STUB_MAPPING.read().unwrap();
        let mapping = guard.as_ref().unwrap();
        let name = tcx.def_path_str(def_id);
        if let Some(replacement) = mapping.get(&name) {
            if let Some(replacement_id) = StubbingPass::get_def_id(tcx, replacement) {
                tracing::debug!("Replacing {} with {}", name, replacement);
                *body = tcx.optimized_mir(replacement_id).clone();
            } else {
                tracing::warn!("Unable to replace {} with {}", name, replacement);
            }
        }
    }

    fn get_def_id<'tcx>(tcx: rustc_middle::ty::TyCtxt<'tcx>, path: &str) -> Option<DefId> {
        tcx.iter_local_def_id().map(LocalDefId::to_def_id).find(|&id| tcx.def_path_str(id) == path)
    }

    pub fn set_stub_mapping(mapping: HashMap<String, String>) {
        *STUB_MAPPING.write().unwrap() = Some(mapping);
    }
}

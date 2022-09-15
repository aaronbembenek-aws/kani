// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::codegen_cprover_gotoc::mir_transform::identity::IdentityPass;
use rustc_hir::def_id::DefId;
use rustc_interface;
use rustc_middle::{
    mir::{Body, MirPass},
    ty::{query::Providers, TyCtxt},
};

mod identity;

fn run_transformation_passes<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) -> &Body<'tcx> {
    tracing::debug!(?def_id, "Run rustc transformation passes");
    let body = ((*rustc_interface::DEFAULT_QUERY_PROVIDERS).optimized_mir)(tcx, def_id);

    tracing::debug!(?def_id, "Run kani transformation passes");
    let mut new_body = body.clone();
    IdentityPass::new().run_pass(tcx, &mut new_body);
    return tcx.arena.alloc(new_body);
}

pub fn provide(providers: &mut Providers) {
    providers.optimized_mir = run_transformation_passes;
}

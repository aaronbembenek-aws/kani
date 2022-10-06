// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use crate::stubbing::transform::StubbingPass;
use kani_queries::UserInput;
use rustc_hir::def_id::DefId;
use rustc_interface;
use rustc_middle::{
    mir::Body,
    ty::{query::ExternProviders, query::Providers, TyCtxt},
};

fn run_transformation_passes<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) -> &Body<'tcx> {
    tracing::debug!(?def_id, "Run rustc transformation passes");
    let body = ((*rustc_interface::DEFAULT_QUERY_PROVIDERS).optimized_mir)(tcx, def_id);
    run_kani_passes(tcx, def_id, body)
}

fn run_transformation_passes_extern<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) -> &Body<'tcx> {
    tracing::debug!(?def_id, "Run rustc extern transformation passes");
    let body = ((*rustc_interface::DEFAULT_EXTERN_QUERY_PROVIDERS).optimized_mir)(tcx, def_id);
    run_kani_passes(tcx, def_id, body)
}

fn run_kani_passes<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId, body: &Body<'tcx>) -> &'tcx Body<'tcx> {
    tracing::debug!(?def_id, "Run kani transformation passes");
    let mut new_body = body.clone();
    StubbingPass::new().run_pass(tcx, def_id, &mut new_body);
    return tcx.arena.alloc(new_body);
}

pub fn provide(providers: &mut Providers, queries: &kani_queries::QueryDb) {
    StubbingPass::set_stub_mapping(queries.get_stub_mapping());
    providers.optimized_mir = run_transformation_passes;
}

pub fn provide_extern(providers: &mut ExternProviders, queries: &kani_queries::QueryDb) {
    StubbingPass::set_stub_mapping(queries.get_stub_mapping());
    providers.optimized_mir = run_transformation_passes_extern;
}

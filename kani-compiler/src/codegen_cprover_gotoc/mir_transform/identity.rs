// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! This "transformation" does not make any changes to the code; it's here as a
//! proof of concept of running a MIR transformation.

use rustc_middle::mir::MirPass;

pub struct IdentityPass {}

impl<'tcx> MirPass<'tcx> for IdentityPass {
    fn run_pass(
        &self,
        _tcx: rustc_middle::ty::TyCtxt<'tcx>,
        _body: &mut rustc_middle::mir::Body<'tcx>,
    ) {
        // do nothing
    }
}

impl IdentityPass {
    pub fn new() -> IdentityPass {
        Self {}
    }
}

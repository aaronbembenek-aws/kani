// Copyright Kani Contributors
// SPDX-License-Identifier: Apache-2.0 OR MIT

use rustc_ast::Attribute;
use rustc_data_structures::fx::FxHashMap;
use rustc_driver::RunCompiler;
use rustc_driver::{Callbacks, Compilation};
use rustc_errors::ErrorGuaranteed;
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::CRATE_DEF_INDEX;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_hir::{Item, ItemKind, Path};
use rustc_interface::interface::Compiler;
use rustc_interface::Queries;
use rustc_middle::ty::TyCtxt;
use rustc_span::Span;

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

enum TryResolveUseResult {
    NotResolved,
    Fn(String),
    Mod(DefId),
}

impl AnnotationCollectorCallbacks {
    pub fn new() -> AnnotationCollectorCallbacks {
        AnnotationCollectorCallbacks { stub_mapping: FxHashMap::default() }
    }

    /*
    see if name is relative
    if so, try to resolve it
    if this fails or it is not relative, then do absolute search
    (first resolving self, super, etc.)
    */
    fn try_resolve(
        tcx: TyCtxt,
        span: Span,
        name: &str,
        current_module: LocalDefId,
    ) -> Option<String> {
        let path: Vec<String> = name.split("::").map(|s| s.to_string()).collect();
        let maybe_resolution = if AnnotationCollectorCallbacks::is_absolute_path(&path) {
            AnnotationCollectorCallbacks::try_resolve_absolute_path(tcx, current_module, &path)
        } else {
            AnnotationCollectorCallbacks::try_resolve_relative_path(tcx, current_module, &path)
        };
        if maybe_resolution.is_some() {
            println!("RESOLVED: {} --> {}", name, maybe_resolution.as_ref().unwrap());
            return maybe_resolution;
        }
        tcx.sess.span_err(span, format!("kani::stub_by: unable to resolve {}", name));
        None
    }

    fn try_resolve_absolute_path(
        tcx: TyCtxt,
        current_module: LocalDefId,
        path: &Vec<String>,
    ) -> Option<String> {
        // FIXME(aaronbem): validate path
        AnnotationCollectorCallbacks::try_resolve_absolute_path_rec(tcx, current_module, path)
    }

    fn try_resolve_absolute_path_rec(
        tcx: TyCtxt,
        current_module: LocalDefId,
        path: &Vec<String>,
    ) -> Option<String> {
        match path[0].as_str() {
            "self" => {
                if path.len() > 1 {
                    let mut path = path.clone();
                    path.remove(0);
                    AnnotationCollectorCallbacks::try_resolve_absolute_path_rec(
                        tcx,
                        current_module,
                        &path,
                    )
                } else {
                    None
                }
            }
            "crate" => {
                let mut cur = current_module;
                // Could use hir().parent_iter() last() instead
                loop {
                    if tcx.def_path_str(cur.to_def_id()) == "" {
                        break;
                    }
                    cur = tcx.parent_module_from_def_id(cur);
                }
                if path.len() > 1 {
                    let mut path = path.clone();
                    path.remove(0);
                    AnnotationCollectorCallbacks::try_resolve_relative_path(tcx, cur, &path)
                } else {
                    None
                }
            }
            "super" => {
                if tcx.def_path_str(current_module.to_def_id()) == "" || path.len() < 2 {
                    None
                } else {
                    let parent = tcx.parent_module_from_def_id(current_module);
                    let mut path = path.clone();
                    path.remove(0);
                    AnnotationCollectorCallbacks::try_resolve_relative_path(tcx, parent, &path)
                }
            }
            "{{root}}" => {
                if path.len() < 3 {
                    None
                } else {
                    let mut path = path.clone();
                    path.remove(0);
                    let first = path.remove(0);
                    for crate_num in tcx.crates(()) {
                        println!("INSPECTING A CRATE");
                        let crate_name = tcx.crate_name(*crate_num);
                        if crate_name.as_str() == first {
                            let crate_def_id = DefId { index: CRATE_DEF_INDEX, krate: *crate_num };
                            println!("FOUND {}", tcx.def_path_str(crate_def_id));
                            return AnnotationCollectorCallbacks::try_resolve_foreign_module(
                                tcx,
                                crate_def_id,
                                &path,
                            );
                        }
                    }
                    None
                }
            }
            _ => AnnotationCollectorCallbacks::try_resolve_relative_path(tcx, current_module, path),
        }
    }

    fn try_resolve_relative_path(
        tcx: TyCtxt,
        current_module: LocalDefId,
        path: &Vec<String>,
    ) -> Option<String> {
        let name = path.join("::");
        let module_name = tcx.def_path_str(current_module.to_def_id());
        let qualified_name =
            if module_name.is_empty() { name.clone() } else { module_name + "::" + &name };

        for item_id in tcx.hir().module_items(current_module) {
            let item = tcx.hir().item(item_id);
            println!("TRYING TO RESOLVE: {} aka {}", name, qualified_name);
            let mut used_mods = Vec::new();
            match item.kind {
                ItemKind::Fn(..) => {
                    let fn_name = tcx.def_path_str(item.def_id.to_def_id());
                    if *qualified_name == fn_name {
                        return Some(fn_name);
                    }
                }
                ItemKind::Use(use_path, kind) => {
                    let maybe_resolved =
                        AnnotationCollectorCallbacks::try_resolve_use(tcx, &name, item, use_path);
                    match maybe_resolved {
                        TryResolveUseResult::Fn(func) => return Some(func),
                        TryResolveUseResult::Mod(mod_id) => match kind {
                            rustc_hir::UseKind::Single => {
                                println!("IDENT: {}", item.ident);
                                println!("ID: {}", tcx.def_path_str(mod_id));
                                if item.ident.as_str() == path[0] {
                                    let mut new_path = path.clone();
                                    new_path.remove(0);
                                    used_mods.push((mod_id, Some(new_path)));
                                }
                            }
                            rustc_hir::UseKind::Glob => used_mods.push((mod_id, None)),
                            rustc_hir::UseKind::ListStem => (),
                        },
                        _ => (),
                    }
                }
                _ => (),
            }
            for (mod_id, maybe_path) in used_mods {
                println!("USED MOD: {}", tcx.def_path_str(mod_id));
                match mod_id.as_local() {
                    Some(mod_id) => {
                        let maybe_res = AnnotationCollectorCallbacks::try_resolve_relative_path(
                            tcx,
                            mod_id,
                            maybe_path.as_ref().unwrap_or(path),
                        );
                        if maybe_res.is_some() {
                            return maybe_res;
                        }
                    }
                    None => {
                        let maybe_res = AnnotationCollectorCallbacks::try_resolve_foreign_module(
                            tcx,
                            mod_id,
                            maybe_path.as_ref().unwrap_or(path),
                        );
                        if maybe_res.is_some() {
                            return maybe_res;
                        }
                    }
                }
            }
        }
        None
    }

    fn try_resolve_foreign_module(
        tcx: TyCtxt,
        foreign_mod: DefId,
        path: &Vec<String>,
    ) -> Option<String> {
        println!("MOD_PATH: {}", tcx.def_path_str(foreign_mod));
        println!("MY_PATH: {}", path.join("::"));
        for child in tcx.module_children(foreign_mod) {
            println!("IDENT: {}", child.ident);
            println!("RES: {:#?}", child.res);
            if child.ident.as_str() == path[0] {
                match child.res {
                    Res::Def(DefKind::Fn, def_id) => {
                        if path.len() == 1 {
                            return Some(tcx.def_path_str(def_id));
                        }
                    }
                    Res::Def(DefKind::Mod, def_id) => {
                        if path.len() > 1 {
                            let mut path = path.clone();
                            path.remove(0);
                            let maybe_res =
                                AnnotationCollectorCallbacks::try_resolve_foreign_module(
                                    tcx, def_id, &path,
                                );
                            if maybe_res.is_some() {
                                return maybe_res;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }

    fn try_resolve_use(
        tcx: TyCtxt,
        name: &str,
        item: &Item,
        use_path: &Path,
    ) -> TryResolveUseResult {
        if let Res::Def(def_kind, def_id) = use_path.res {
            match def_kind {
                DefKind::Fn => {
                    let ident = item.ident.to_string();
                    //println!("IDENT {}", ident);
                    if ident == name {
                        let res_name = tcx.def_path_str(def_id);
                        println!("RESOLVED: {} --> {}", &name, res_name);
                        return TryResolveUseResult::Fn(res_name);
                    }
                }
                DefKind::Mod => return TryResolveUseResult::Mod(def_id),
                _ => {}
            }
        }
        TryResolveUseResult::NotResolved
    }

    fn is_absolute_path(path: &Vec<String>) -> bool {
        match path[0].as_str() {
            "crate" | "super" | "self" | "{{root}}" => true,
            _ => false,
        }
    }

    fn extract_stub_by(
        tcx: TyCtxt,
        attr: &Attribute,
        current_module: LocalDefId,
    ) -> Option<(String, String)> {
        if let Some(args) = extract_path_arguments(attr) {
            if args.len() == 2 {
                let original = AnnotationCollectorCallbacks::try_resolve(
                    tcx,
                    attr.span,
                    &args[0],
                    current_module,
                );
                let replacement = AnnotationCollectorCallbacks::try_resolve(
                    tcx,
                    attr.span,
                    &args[1],
                    current_module,
                );
                return original
                    .map(|original| replacement.map(|replacement| (original, replacement)))
                    .flatten();
            }
        }
        tcx.sess.span_err(attr.span, "kani::stub_by takes two path arguments");
        None
    }

    fn update_stub_mapping(
        tcx: TyCtxt,
        current_module: LocalDefId,
        attr: &Attribute,
        stub_pairs: &mut FxHashMap<String, String>,
    ) {
        if let Some((original, replacement)) =
            AnnotationCollectorCallbacks::extract_stub_by(tcx, attr, current_module)
        {
            let other = stub_pairs.insert(original.clone(), replacement.clone());
            if let Some(other) = other {
                tcx.sess.span_err(
                    attr.span,
                    format!(
                        "duplicate stub mapping: {} mapped to {} AND {}",
                        original, replacement, other
                    ),
                );
            }
        }
    }
}

impl Callbacks for AnnotationCollectorCallbacks {
    fn after_analysis<'tcx>(
        &mut self,
        _compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        queries.global_ctxt().unwrap().peek_mut().enter(|tcx| {
            for item in tcx.hir_crate_items(()).items() {
                let def_id = item.def_id.to_def_id();
                //println!("ITEM: {}", tcx.def_path_str(def_id));
                //println!("{:#?}", tcx.hir().item(item));
                let (proof, other) = partition_kanitool_attributes(tcx.get_attrs_unchecked(def_id));
                if proof.is_empty() {
                    continue;
                }
                let mut stub_pairs = FxHashMap::default();
                let current_module = tcx.parent_module_from_def_id(item.def_id);
                for (name, attr) in other {
                    if name == "stub_by" {
                        AnnotationCollectorCallbacks::update_stub_mapping(
                            tcx,
                            current_module,
                            attr,
                            &mut stub_pairs,
                        );
                    }
                }
                let harness_name = tcx.def_path_str(def_id);
                self.stub_mapping.insert(harness_name, stub_pairs);
            }
            tcx.sess.abort_if_errors();
            Compilation::Stop
        })
    }
}

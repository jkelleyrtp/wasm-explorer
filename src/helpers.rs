use std::collections::HashMap;

use walrus::{ElementItems, ElementKind, ExportId, FunctionId, FunctionKind, IdsToIndices};
use walrus::{ImportKind, Local, Module};
use wasmparser::{RelocSectionReader, RelocationEntry};

#[derive(Debug, Clone)]
pub struct SplitPoint {
    pub module_name: String,
    pub import: FunctionId,
    pub export: FunctionId,
    pub import_name: String,
    pub export_name: String,
    pub component_name: String,
}

pub struct SplitModule {
    pub name: String,
}

pub fn accumulate_split_points(module: &Module) -> Vec<SplitPoint> {
    module
        .imports
        .iter()
        .flat_map(|import| {
            if !import.name.starts_with("__wasm_split") {
                return None;
            }

            // __wasm_split_00add_body_element00_import_abef5ee3ebe66ff17677c56ee392b4c2_SomeRoute2
            // __wasm_split_00add_body_element00_export_abef5ee3ebe66ff17677c56ee392b4c2_SomeRoute2
            let remain = import.name.trim_start_matches("__wasm_split_00");
            let (mod_namename, rest) = remain.split_once("00").unwrap();
            let rest = rest.trim_start_matches("_import_");
            let (hash, fnname) = rest.split_once("_").unwrap();

            let ImportKind::Function(import_id) = &import.kind else {
                return None;
            };

            let exported_name = format!("__wasm_split_00{mod_namename}00_export_{hash}_{fnname}");
            let export_id = module
                .exports
                .get_func(&exported_name)
                .unwrap_or_else(|_err| {
                    let exports = module.exports.iter().map(|e| &e.name).collect::<Vec<_>>();
                    println!(
                        "Export not found: {}. Exports: {:?}",
                        exported_name, exports
                    );

                    for export in exports {
                        if export.contains("__wasm_split") {
                            println!("Found: {}", export);
                        }
                    }

                    panic!("Failed to find export: {}", exported_name);
                });

            Some(SplitPoint {
                module_name: mod_namename.to_string(),
                import: *import_id,
                export: export_id,
                export_name: exported_name,
                import_name: import.name.clone(),
                component_name: fnname.to_string(),
            })
        })
        .collect()
}

/// weird/not-weird
pub fn accumulate_indirect_fns(module: &mut Module) -> (Vec<FunctionId>, Vec<FunctionId>) {
    let mut weird_ids = vec![];
    let mut ids = vec![];
    for func in module.funcs.iter_mut() {
        let func_id = func.id();
        let FunctionKind::Local(local) = &mut func.kind else {
            continue;
        };

        let mut last = None;
        for (instr, id) in local.builder_mut().func_body().instrs().iter() {
            if let walrus::ir::Instr::CallIndirect { .. } = instr {
                match last {
                    Some(walrus::ir::Instr::Load(_)) => {
                        ids.push(func_id);
                    }
                    _ => {
                        weird_ids.push(func_id);
                        println!("Weird: {:?}", func.name.as_ref().unwrap());
                    }
                }
            }
            last = Some(instr.clone());
        }
    }

    (weird_ids, ids)
}

pub fn accumulate_active_segments(module: &Module) -> Vec<(FunctionId, String)> {
    // there should only be one?
    let elements = module.elements.iter().next().unwrap();
    let ElementKind::Active { table, offset } = &elements.kind else {
        panic!("Expected active element");
    };
    let ElementItems::Functions(funcs) = &elements.items else {
        panic!("Expected functions");
    };

    funcs
        .iter()
        .map(|id| {
            let func = module.funcs.get(*id);
            let name = func.name.as_ref().cloned().unwrap_or_default();
            (*id, name.clone())
        })
        .collect()
}

pub trait Demangler {
    fn demangle(&self) -> String;
}

impl<T> Demangler for T
where
    T: AsRef<str>,
{
    fn demangle(&self) -> String {
        let name = self.as_ref();
        rustc_demangle::try_demangle(name)
            .map(|f| f.to_string())
            .unwrap_or_else(|_| name.to_string())
    }
}

use std::{collections::HashMap, ops::Range};

use anyhow::Result;
use walrus::{
    ir::{Binop, Instr},
    Data, Function, FunctionId, FunctionKind, IdsToIndices, InstrLocId, Module,
};
use wasmparser::{RelocSectionReader, RelocationEntry, SymbolInfo};

#[derive(Debug)]
pub struct RelocationMap {
    pub code_relocs: HashMap<usize, RelocationEntry>,
    pub data_relocs: HashMap<usize, RelocationEntry>,
    /// A map from function ID to relocations
    pub functions: HashMap<FunctionId, FunctionRelocations>,
}

#[derive(Debug)]
pub struct FunctionRelocations {
    pub func_id: FunctionId,
    pub original_range: Range<usize>,
    pub relocations: Vec<RelocationEntry>,
    pub relocation_map: HashMap<usize, RelocationEntry>,
}

/// Aggregate the relocations per function
///
/// This way you can grab out the original relocations used to fixup any given function
pub fn accumulate_relocations(module: &Module) -> Result<RelocationMap> {
    // Get the relocation section
    let code_relocs = accumulate_relocations_from_section(module, "reloc.CODE")?;
    let data_relocs = accumulate_relocations_from_section(module, "reloc.DATA")?;

    // if data_relocs.keys().any(|key| code_relocs.contains_key(key)) {
    //     panic!("duplicate offsets in code/data relocs");
    // }

    let mut current_relocation_idx = 0;
    let mut relocations_per_function = HashMap::new();

    for func in module.funcs.iter() {
        let mut relocations = Vec::new();
        let mut relocation_map = HashMap::new();

        // Only handle locally defined functions
        let FunctionKind::Local(local_function) = &func.kind else {
            continue;
        };

        let original_range = local_function
            .original_range
            .as_ref()
            .expect("locally defined functions to have valid sourecode ranges")
            .clone();

        // Accumulate all relocations within this local function's range
        while let Some(this_reloc) = code_relocs.get(&current_relocation_idx) {
            let this_reloc_offset = this_reloc.offset as usize;
            if this_reloc_offset >= original_range.end {
                break;
            }

            // Ensure we only save relocations that are valid for this function's range
            debug_assert!(original_range.contains(&this_reloc_offset));

            relocations.push(this_reloc.clone());
            relocation_map.insert(this_reloc_offset, this_reloc.clone());
            current_relocation_idx += 1;
        }

        relocations_per_function.insert(
            func.id(),
            FunctionRelocations {
                func_id: func.id(),
                original_range,
                relocations,
                relocation_map,
            },
        );
    }

    Ok(RelocationMap {
        functions: relocations_per_function,
        code_relocs,
        data_relocs,
    })
}

pub fn accumulate_relocations_from_section(
    module: &Module,
    section_name: &str,
) -> anyhow::Result<HashMap<usize, RelocationEntry>> {
    let (_reloc_id, code_reloc) = module
        .customs
        .iter()
        .find(|(_, c)| c.name() == section_name)
        .unwrap();

    let code_reloc_data = code_reloc.data(&IdsToIndices::default());

    // Accumulate the relocations
    let mut relocations = HashMap::new();
    for entry in RelocSectionReader::new(&code_reloc_data, 0)
        .unwrap()
        .entries()
        .into_iter()
        .flatten()
    {
        relocations.insert(entry.offset as usize, entry);
    }

    Ok(relocations)
}

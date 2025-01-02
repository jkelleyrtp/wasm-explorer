#![allow(non_snake_case)]

use std::collections::HashMap;

use dioxus::prelude::*;
use itertools::Itertools;
use relocations::{accumulate_relocations, RelocationMap};
use walrus::{FunctionId, FunctionKind, Module};
mod helpers;
mod relocations;
use helpers::*;
use wasmparser::RelocationEntry;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");

fn main() {
    dioxus::launch(|| {
        rsx! {
            document::Link { rel: "icon", href: FAVICON }
            document::Link { rel: "stylesheet", href: MAIN_CSS }
            document::Link { rel: "stylesheet", href: TAILWIND_CSS }
            Router::<Route> {}
        }
    });
}

static WASM_FILE: GlobalSignal<Option<ParsedModule>> = Global::new(|| {
    if std::fs::exists("sample.wasm").unwrap_or(false) {
        let bytes = std::fs::read("sample.wasm").unwrap();
        return ParsedModule::new(bytes);
    }

    None
});

#[derive(Clone, PartialEq, Debug, Routable)]
enum Route {
    #[layout(Nav)]
    #[route("/")]
    Home,
    #[route("/exports")]
    Exports,
    #[route("/data-symbols")]
    DataSymbols,
    #[route("/split-points")]
    SplitPoints,
    #[route("/relocations")]
    Relocations,
    #[route("/data-relocations")]
    DataRelocations,
    #[route("/functions")]
    Functions,
    #[route("/functions/:name")]
    SingleFunction { name: String },
    #[route("/indirects/:weird")]
    IndirectFns { weird: bool },
}

#[component]
fn Nav() -> Element {
    let add_wasm_file = move |event: FormEvent| async move {
        async fn receive_file(event: FormEvent) -> Option<ParsedModule> {
            let files = event.files()?;
            let file_list = files.files();
            let file_name = file_list.get(0)?.clone();
            let contents = files.read_file(&file_name).await?;
            ParsedModule::new(contents)
        }

        if let Some(contents) = receive_file(event).await {
            *WASM_FILE.write() = Some(contents);
        }
    };
    rsx! {
        nav { class: "flex justify-between items-center bg-gray-800 text-white p-4 border border-red-500",
            div { class: "flex items-center justify-start space-x-4",
                h1 { "Wasm Explorer" }
                input { r#type: "file", class: "w-24", oninput: add_wasm_file }
                button { onclick: move |_| *WASM_FILE.write() = None, "Clear" }
            }
            div { class: "flex items-center space-x-4",
                Link { to: Route::Home, "Home" }
                Link { to: Route::Exports, "Exports" }
                Link { to: Route::DataSymbols, "Data Symbols" }
                Link { to: Route::SplitPoints, "Split Points" }
                Link { to: Route::Relocations, "Relocations" }
                Link { to: Route::DataRelocations, "Data Relocations" }
                Link { to: Route::Functions, "Functions" }
                Link { to: Route::IndirectFns { weird: false }, "Indirect Fns" }
                Link { to: Route::IndirectFns { weird: true }, "Weird Indirect" }
            }
        }
        Outlet::<Route> {}
    }
}

#[component]
fn Home() -> Element {
    let skeleton = use_memo(skeleton_memo);
    fn skeleton_memo() -> Option<String> {
        let file = WASM_FILE.read();
        let file = file.as_ref()?;
        let mut printer = wasmprinter::PrintFmtWrite(String::default());
        wasmprinter::Config::default()
            .print_skeleton(true)
            .print(&file.bytes, &mut printer)
            .ok()?;
        Some(printer.0)
    }

    rsx! {
        div { class: "flex flex-col",
            if let Some(skeleton) = skeleton.read().as_ref() {
                pre { "{skeleton}" }
            }
        }
    }
}

fn DataSymbols() -> Element {
    let maybe_file = WASM_FILE.read_unchecked();
    let Some(file) = maybe_file.as_ref() else {
        return rsx! { "No module loaded" };
    };

    rsx! {
        div {
            h1 { "Data Symbols" }
            div { class: "flex flex-col space-y-2",
                for data in file.module.data.iter() {
                    div { class: "flex flex-row space-x-2",
                        span { "{data.name.as_ref().unwrap()}" }
                        span { "{data.value.len()} bytes" }
                        span { "{data.kind:?}" }
                    }
                }
            }
        }
    }
}

fn Exports() -> Element {
    let maybe_file = WASM_FILE.read();
    let Some(file) = maybe_file.as_ref() else {
        return rsx! { "No module loaded" };
    };

    let mut show_wb = use_signal(|| false);

    rsx! {
        div {
            h1 { "Module" }
            div {
                input {
                    r#type: "checkbox",
                    oninput: move |event| show_wb.set(event.checked()),
                }
                label { "Show wb describe" }
            }
            div { class: "flex flex-col space-y-2",
                for export in file.module.exports.iter() {
                    if !(!show_wb() && export.name.contains("__wbindgen_describe")) {
                        div {
                            span { {export.name.as_str().demangle()} }
                            span { {export.name.as_str().demangle()} }
                        }
                    }
                }
            }
        }
    }
}

fn SplitPoints() -> Element {
    let maybe_file = WASM_FILE.read();
    let Some(file) = maybe_file.as_ref() else {
        return rsx! { "No module loaded" };
    };

    rsx! {
        div {
            h1 { "Split Points" }
            div { class: "flex flex-col space-y-2",
                for split_point in file.split_points.iter() {
                    div { class: "flex flex",
                        pre { "{split_point:#?}" }
                    }
                }
            }
        }
    }
}

fn Functions() -> Element {
    let maybe_file = WASM_FILE.read();
    let Some(file) = maybe_file.as_ref() else {
        return rsx! { "No module loaded" };
    };

    let funcs: Vec<_> = file
        .module
        .funcs
        .iter()
        .map(|func| {
            let demangled = func.name.as_ref().unwrap().demangle();
            (demangled, func)
        })
        .sorted_by(|f1, f2| f1.0.cmp(&f2.0))
        .collect();

    rsx! {
        div {
            h1 { "Functions" }
            div { class: "flex flex-col",
                for (demangled_name , func) in funcs.iter() {
                    div {
                        Link {
                            to: Route::SingleFunction {
                                name: func.name.as_ref().unwrap().clone(),
                            },
                            pre {
                                color: match &func.kind {
                                    FunctionKind::Local(_) => "white",
                                    FunctionKind::Import(_) => "green",
                                    _ => "gray",
                                },
                                "{demangled_name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug, Props)]
struct IndirectFnProps {
    weird: bool,
}

fn IndirectFns(props: IndirectFnProps) -> Element {
    let maybe_file = WASM_FILE.read();
    let Some(file) = maybe_file.as_ref() else {
        return rsx! { "No module loaded" };
    };

    let fns = if props.weird {
        &file.fns_with_weird_indirects
    } else {
        &file.fns_with_indirects
    };

    let entries = fns.iter().map(|id| {
        let func = file.module.funcs.get(*id);
        let name = func.name.as_ref().unwrap();

        rsx! {
            div {
                Link {
                    to: Route::SingleFunction {
                        name: name.clone(),
                    },
                    pre { "{name}" }
                }
            }
        }
    });

    rsx! {
        div {
            h1 { "Functions" }
            div { class: "flex flex-col", {entries} }
        }
    }
}

#[component]
fn SingleFunction(name: String) -> Element {
    let mut maybe_file = WASM_FILE.write_unchecked();
    let Some(file) = maybe_file.as_mut() else {
        return rsx! { "No module loaded" };
    };

    let func = file.module.funcs.by_name(&name).unwrap();
    let func = file.module.funcs.get_mut(func);
    let func_id = func.id();

    let FunctionKind::Local(local) = &mut func.kind else {
        return rsx! { "Not a local function" };
    };

    let relocations = &file.relocations;
    let body = local.builder_mut().func_body();
    let instrs = body.instrs();

    let mut last = None;
    let root_offset = instrs.iter().next().unwrap().1.data();

    let instrs = instrs.iter().map(|(instr, id)| {
        let last_load = match last.as_ref() {
            Some(walrus::ir::Instr::Load(load)) => {
                let (function_id, function_name) = file
                    .active_functions
                    .get(load.arg.offset as usize)
                    .expect("offset to exist");
                Some(rsx! {
                    pre { color: "green", "  {function_name}" }
                })
            }
            _ => None,
        };

        let offset = id.data() - root_offset + 6;
        let relocation = relocations
            .code_relocs
            .get(&(offset as usize))
            .map(|reloc| {
                Some(rsx! {
                    pre { color: "blue", "  {reloc:?}" }
                })
            });

        last = Some(instr.clone()).clone();

        rsx! {
            div {
                // {last_load}
                pre {
                    color: match instr {
                        walrus::ir::Instr::CallIndirect { .. } => "red",
                        walrus::ir::Instr::Load { .. } => "yellow",
                        _ => "white",
                    },
                    "{instr:?}"
                }
                {relocation}
            }
        }
    });

    rsx! {
        div {
            h1 { "Function: {func.name.as_ref().unwrap().demangle()}" }
            div { class: "flex flex-col", {instrs} }
        }
    }
}

fn Relocations() -> Element {
    let mut maybe_file = WASM_FILE.write_unchecked();
    let Some(file) = maybe_file.as_mut() else {
        return rsx! { "No module loaded" };
    };

    let relocations = &file.relocations;

    rsx! {
        div {
            h1 { "relocations" }
            pre { "{relocations:#?}" }
        }
    }
}

fn DataRelocations() -> Element {
    let mut maybe_file = WASM_FILE.write_unchecked();
    let Some(file) = maybe_file.as_mut() else {
        return rsx! { "No module loaded" };
    };

    let relocations = &file.relocations.data_relocs;

    rsx! {
        div {
            h1 { "relocations" }
            pre { "{relocations:#?}" }
        }
    }
}

struct ParsedModule {
    bytes: Vec<u8>,
    module: Module,
    split_points: Vec<SplitPoint>,
    fns_with_indirects: Vec<FunctionId>,
    fns_with_weird_indirects: Vec<FunctionId>,
    active_functions: Vec<(FunctionId, String)>,
    relocations: RelocationMap,
}

impl ParsedModule {
    fn new(bytes: Vec<u8>) -> Option<Self> {
        let mut module = Module::from_buffer(&bytes).ok()?;
        let (fns_with_weird_indirects, fns_with_indirects) = accumulate_indirect_fns(&mut module);
        let split_points = accumulate_split_points(&module);
        let active_functions = accumulate_active_segments(&module);
        let relocations = accumulate_relocations(&module).ok()?;
        Some(Self {
            bytes,
            module,
            split_points,
            fns_with_indirects,
            fns_with_weird_indirects,
            active_functions,
            relocations,
        })
    }
}

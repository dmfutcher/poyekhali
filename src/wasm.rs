use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;

use wasmer_runtime::{error, func, imports, instantiate, Array, Ctx, Func, ImportObject, WasmPtr};

type WasmData = Box<[u8]>;

pub struct WasmRuntime {
    execution_contexts: Vec<WasmExecutionContext>,
}

impl WasmRuntime {

    pub fn new() -> WasmRuntime {
        WasmRuntime{
            execution_contexts: vec!(),
        }
    }

    pub fn execute(&self, wasm: &WasmData) {
        let mut ctx = WasmExecutionContext::new(wasm).expect("Failed to init executionctx");
        ctx.run();
    }

}

struct WasmExecutionContext {
    instance: wasmer_runtime::Instance,
}

impl WasmExecutionContext {

    fn new(wasm: &WasmData) -> Result<WasmExecutionContext, String> {
        let abort = |_: i32, _: i32, _: i32, _: i32| std::process::exit(-1);
        let log = move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32| {
            let memory = ctx.memory(0);
            let string = ptr.get_utf8_string(memory, len * 2).unwrap();
            println!("log: {}", string);
        };

        let fn_table = imports! {
            "env" => {
                "abort" => func!(abort),
            },
            "host" => {
                "host.log" => func!(log),
            }
        };
        let instance = instantiate(wasm, &fn_table).map_err(|e| e.to_string())?;

        Ok(WasmExecutionContext{
            instance,
        })
    }

    fn run(&self) {
        let start_fn: Func<(), i32> = self.instance.exports.get("start").unwrap();
        start_fn.call();
    }

}

pub struct WasmLoader {}
impl WasmLoader {

    pub fn load(file: &str) -> io::Result<WasmData> {
        let mut f = File::open(file)?;
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)?;

        Ok(buffer.into_boxed_slice())
    }

}
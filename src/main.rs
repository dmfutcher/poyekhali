extern crate wasmer_runtime;

mod wasm;

use wasm::{WasmRuntime, WasmLoader};

fn main() -> Result<(), String> {
    let wasm = WasmLoader::load("./belka/build/optimized.wasm").map_err(|e| e.to_string())?;

    let mut runtime = WasmRuntime::new();
    runtime.init_module("main".to_owned(), &wasm);

    std::thread::sleep_ms(2000);

    runtime.stop();

    Ok(())
}
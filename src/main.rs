extern crate wasmer_runtime;

mod wasm;

use wasm::{WasmRuntime, WasmLoader};

fn main() -> Result<(), String> {
    let wasm = WasmLoader::load("./belka/build/optimized.wasm").map_err(|e| e.to_string())?;

    let runtime = WasmRuntime::new();
    runtime.execute(&wasm);

    std::thread::sleep_ms(1000);

    runtime.stop();

    Ok(())
}
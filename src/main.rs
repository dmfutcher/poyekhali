
#[macro_use] extern crate wasmer;

mod wasm;
mod stream;

use wasm::{WasmRuntime, WasmLoader};
use stream::StreamManager;

fn main() -> Result<(), String> {
    let wasm = WasmLoader::load("./belka/build/optimized.wasm").map_err(|e| e.to_string())?;

    let mut runtime = WasmRuntime::new();
    runtime.init_module("main".to_owned(), &wasm);

    let mut streams = StreamManager::new(&runtime);
    streams.start_streams();

    while (true) {
        // runtime.publish_stream_change("timer:seconds".to_owned());
        std::thread::sleep_ms(1000);
    }

    // runtime.stop();

    Ok(())
}
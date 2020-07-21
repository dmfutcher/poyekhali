use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;
use std::thread;
use std::sync::mpsc;

use wasmer_runtime::{error, func, imports, instantiate, Array, Ctx, Func, ImportObject, WasmPtr};

type WasmProgram = Box<[u8]>;

pub struct WasmRuntime {
    execution_contexts: Vec<WasmExecutionContext>,
}

impl WasmRuntime {

    pub fn new() -> WasmRuntime {
        WasmRuntime{
            execution_contexts: vec!(),
        }
    }

    pub fn execute(&self, wasm: &WasmProgram) {
        let ctx = WasmExecutionContext::new(wasm).expect("Failed to init executionctx");
        ctx.run();
        ctx.tst();
    }

}

struct WasmExecutionContext {
    instance: wasmer_runtime::Instance,
    exec_thread: thread::JoinHandle<()>,
    exec_tx: mpsc::Sender<i32>,  
}

impl WasmExecutionContext {

    fn new(wasm: &WasmProgram) -> Result<WasmExecutionContext, String> {
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
        let (tx, rx) = mpsc::channel::<i32>();
        let exec = ExecutorThread::new(rx);
        let thread_handle = thread::spawn(move || exec.run());

        Ok(WasmExecutionContext{
            instance,
            exec_thread: thread_handle,
            exec_tx: tx,
        })
    }

    fn run(&self) {
        let start_fn: Func<(), i32> = self.instance.exports.get("start").unwrap();
        start_fn.call();
    }

    fn tst(&self) {
        self.exec_tx.send(1);
    }

}

struct ExecutorThread {
    rx: mpsc::Receiver<i32>,
}

impl ExecutorThread {

    fn new(rx: mpsc::Receiver<i32>) -> ExecutorThread {
        ExecutorThread{
            rx
        }
    }

    fn run(&self) {
        while let Ok(msg) = self.rx.recv() {
            println!("Broadcaster got message: {}", msg);
        }
    }

}

pub struct WasmLoader {}
impl WasmLoader {

    pub fn load(file: &str) -> io::Result<WasmProgram> {
        let mut f = File::open(file)?;
        let mut buffer = Vec::new();
        f.read_to_end(&mut buffer)?;

        Ok(buffer.into_boxed_slice())
    }

}
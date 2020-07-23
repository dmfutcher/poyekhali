use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;
use std::thread;
use std::sync::mpsc;

use wasmer_runtime::{error, func, imports, instantiate, Array, Ctx, Func, ImportObject, WasmPtr};

type WasmProgram = Box<[u8]>;

pub struct WasmRuntime{
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
    }

    pub fn stop(self) {
        for c in self.execution_contexts.into_iter() {
            c.join();
        }
    }

}

struct WasmExecutionContext {
    exec_thread: thread::JoinHandle<()>,
    exec_tx: mpsc::Sender<ExecutorCommand>,
}

impl WasmExecutionContext {

    fn new(wasm: &WasmProgram) -> Result<WasmExecutionContext, String> {
        let (tx, rx) = mpsc::channel::<ExecutorCommand>();
        let exec = ExecutorThread::new(rx, wasm);
        let thread_handle = thread::spawn(move || exec.run());

        Ok(WasmExecutionContext{
            exec_thread: thread_handle,
            exec_tx: tx,
        })
    }

    fn run(self) {
        self.exec_tx.send(ExecutorCommand::Call("init".to_string()));
    }

    fn join(self) {
        self.exec_thread.join();
    }

}

#[derive(Debug)]
enum ExecutorCommand {
    Call(String),
}

struct ExecutorThread {
    rx: mpsc::Receiver<ExecutorCommand>,
    instance: wasmer_runtime::Instance,
}

impl ExecutorThread {

    fn new(rx: mpsc::Receiver<ExecutorCommand>, wasm: &WasmProgram) -> ExecutorThread {
        let abort = |_: i32, _: i32, _: i32, _: i32| std::process::exit(-1);
        let log = move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32| {
            let memory = ctx.memory(0);
            let string = ptr.get_utf8_string(memory, len * 2).unwrap();
            println!("log: {}", string);
        };
        let stream_register = |_ctx: &mut Ctx, _: i32, _:i32, fncptr:i32| {
            println!("here, {}", fncptr);
        };
        let fn_table = imports! {
            "env" => {
                "abort" => func!(abort),
            },
            "host" => {
                "host.log" => func!(log),
            },
            "stream" => {
                "stream.register" => func!(stream_register),
            }
        };
        
        let instance = instantiate(wasm, &fn_table).unwrap(); // TODO: No unwrap

        ExecutorThread{
            rx,
            instance
        }
    }

    fn run(&self) {
        while let Ok(cmd) = self.rx.recv() {
            println!("rcv: {:?}", cmd);
            match cmd {
                ExecutorCommand::Call(fn_name) => self.call_function(fn_name)
            }
        }
    }

    // TODO: How can we handle return values? Can we avoid needing to use return values?
    fn call_function(&self, fn_name: String) {
        match self.instance.exports.get::<Func<(), i32>>(fn_name.as_str()) {
            Ok(f) => { f.call(); }
            Err(e) => println!("Could not call fn {}: {}", fn_name, e)
        };
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
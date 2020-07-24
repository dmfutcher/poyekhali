use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;
use std::thread;
use std::sync::mpsc;
use std::collections::HashMap;

use wasmer_runtime::{func, imports, instantiate, Array, Ctx, Func, WasmPtr};
// use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};

type WasmProgram = Box<[u8]>;

// module name, function table index
type FunctionIndex = (String, u32);

pub struct WasmRuntime{
    execution_contexts: HashMap<String, WasmExecutionContext>,
    stream_handlers: HashMap<String, Vec<FunctionIndex>>,
    request_servicer: Option<thread::JoinHandle<()>>,
    req_tx: mpsc::Sender<ExecutorRequest>,
}

impl WasmRuntime {

    pub fn new() -> WasmRuntime {
        let (tx, rx) = mpsc::channel::<ExecutorRequest>();
        let mut runtime = WasmRuntime{
            execution_contexts: HashMap::new(),
            stream_handlers: HashMap::new(),
            request_servicer: None,
            req_tx: tx,
        };
        runtime.request_servicer = Some(thread::spawn(|| RuntimeRequestServicer::request_servicer(rx)));

        runtime
    }

    pub fn init_module(&mut self, module_name: String, wasm: &WasmProgram) {
        let ctx = WasmExecutionContext::new(wasm, self.req_tx.clone()).expect("Failed to init executionctx");
        ctx.initialise();

        self.execution_contexts.insert(module_name, ctx);
    }

    pub fn stop(self) {
        for (_, c) in self.execution_contexts.into_iter() {
            c.join();
        }
    }

}

struct RuntimeRequestServicer {

}
impl RuntimeRequestServicer {

    fn request_servicer(rx: mpsc::Receiver<ExecutorRequest>) {
        while let Ok(msg) = rx.recv() {
            println!("request_svc: recv {:?}", msg);
        }
    }

}

struct WasmExecutionContext {
    thread: thread::JoinHandle<()>,
    tx_cmd: mpsc::Sender<ExecutorCommand>,
}

impl WasmExecutionContext {

    fn new(wasm: &WasmProgram, tx_request: mpsc::Sender<ExecutorRequest>) -> Result<WasmExecutionContext, String> {
        let (tx_cmd, rx_cmd) = mpsc::channel::<ExecutorCommand>();
        let mut exec_thd = ExecutorThread::new(rx_cmd, tx_request, wasm);
        let thread_handle = thread::spawn(move || exec_thd.run());

        Ok(WasmExecutionContext{
            thread: thread_handle,
            tx_cmd,
        })
    }

    fn initialise(&self) {
        self.tx_cmd.send(ExecutorCommand::CallName("init".to_string()));
    }

    fn join(self) {
        drop(self.tx_cmd);
        self.thread.join();
    }

}

#[derive(Debug)]
enum ExecutorCommand {
    CallName(String),
    CallIndex(u32),
}

#[derive(Debug)]
enum ExecutorRequest {
    RegisterHandler{ stream: String, module: String, func_ptr: u32 }
}
unsafe impl Sync for ExecutorRequest {}


struct ExecutorThread {
    cmd_rx: mpsc::Receiver<ExecutorCommand>,
    req_tx: mpsc::Sender<ExecutorRequest>,
    instance: Option<wasmer_runtime::Instance>,
    wasm: WasmProgram,
}

impl ExecutorThread {

    fn new(cmd_rx: mpsc::Receiver<ExecutorCommand>, req_tx: mpsc::Sender<ExecutorRequest>, wasm: &WasmProgram) -> ExecutorThread {
        ExecutorThread{
            cmd_rx,
            req_tx,
            instance: None,
            wasm: wasm.clone(),
        }
    }

    fn run(&mut self) {
        let req_tx = self.req_tx.clone();

        let abort = |_: i32, _: i32, _: i32, _: i32| std::process::exit(-1);
        let log = move |ctx: &mut Ctx, ptr: WasmPtr<u8, Array>, len: u32| {
            let memory = ctx.memory(0);
            let string = ptr.get_utf8_string(memory, len * 2).unwrap();
            println!("log: {}", string);
        };
        let stream_register = move |_ctx: &mut Ctx, _: i32, _:i32, fncptr: u32| {
            println!("here, {:?}", fncptr);
            req_tx.send(ExecutorRequest::RegisterHandler{
                    stream: "stream-name".to_string(),
                    module: "my-module".to_string(), func_ptr: fncptr});
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
        
        let instance = instantiate(&self.wasm, &fn_table).unwrap(); // TODO: No unwrap
        self.instance = Some(instance);

        while let Ok(cmd) = self.cmd_rx.recv() {
            println!("rcv: {:?}", cmd);
            match cmd {
                ExecutorCommand::CallName(fn_name) => self.call_named_function(fn_name),
                ExecutorCommand::CallIndex(idx) => println!("TODO: {}", idx),
            }
        }
    }

    // TODO: How can we handle return values? Can we avoid needing to use return values?
    fn call_named_function(&self, fn_name: String) {
        match self.instance.as_ref().unwrap().exports.get::<Func<(), i32>>(fn_name.as_str()) {
            Ok(f) => { f.call(); }
            Err(e) => println!("Could not call fn {}: {}", fn_name, e)
        };
    }

    fn call_indexed_function(&self, index: u32) {
        println!("would call {}", index);
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

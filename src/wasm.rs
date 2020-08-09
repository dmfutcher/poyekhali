use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};
use std::collections::HashMap;

use wasmer_runtime::{func, imports, instantiate, Array, Ctx, Func, WasmPtr};
use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};

use crate::stream::StreamUpdate;

type WasmProgram = Box<[u8]>;

#[derive(Debug)]
struct FunctionIndex { module: String, func: u32 }

pub struct WasmRuntime{
    execution_contexts: HashMap<String, WasmExecutionContext>,
    stream_handlers: Arc<Mutex<HashMap<String, Vec<FunctionIndex>>>>,
    request_servicer: Option<thread::JoinHandle<()>>,
    req_tx: ExecutorRequestSender,
}

impl WasmRuntime {

    pub fn new() -> WasmRuntime {
        let (tx, rx) = mpsc::channel::<ExecutorRequest>();
        let mut runtime = WasmRuntime{
            execution_contexts: HashMap::new(),
            stream_handlers: Arc::new(Mutex::new(HashMap::new())),
            request_servicer: None,
            req_tx: tx,
        };

        let send_handlers = Arc::clone(&runtime.stream_handlers);
        runtime.request_servicer = Some(thread::spawn(|| RuntimeRequestServicer::request_servicer(rx, send_handlers)));

        runtime
    }

    pub fn init_module(&mut self, module_name: String, wasm: &WasmProgram) {
        let ctx = WasmExecutionContext::new(module_name.clone(), wasm, self.request_channel()).expect("Failed to init executionctx");
        ctx.initialise();

        self.execution_contexts.insert(module_name, ctx);
    }

    pub fn publish_stream_change(&self, stream: String) {
        let stream_handlers = self.stream_handlers.lock().unwrap();
        if let Some(handlers) = stream_handlers.get(&stream) {
            for func_idx in handlers {
                if let Some(exec_ctx) = self.execution_contexts.get(&func_idx.module) {
                    exec_ctx.tx_cmd.send(ExecutorCommand::CallIndex(func_idx.func));
                }
            }
        }
    }

    pub fn request_channel(&self) -> ExecutorRequestSender {
        self.req_tx.clone()
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

    fn request_servicer(rx: mpsc::Receiver<ExecutorRequest>, handlers: Arc<Mutex<HashMap<String, Vec<FunctionIndex>>>>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                ExecutorRequest::RegisterHandler{stream, module, func_ptr} => {
                    println!("Registering handler for stream {}, {}::{}", stream, module, func_ptr);
                    let func_idx = FunctionIndex{ module, func: func_ptr };

                    let mut h = handlers.lock().unwrap();
                    h.entry(stream).or_insert_with(Vec::new).push(func_idx);
                },
                ExecutorRequest::StreamUpdate{stream, update} => {
                    println!("Stream update: {} {:?}", stream, update);
                }
            }
        }
    }

}

struct WasmExecutionContext {
    thread: thread::JoinHandle<()>,
    tx_cmd: mpsc::Sender<ExecutorCommand>,
}

impl WasmExecutionContext {

    fn new(module: String, wasm: &WasmProgram, tx_request: mpsc::Sender<ExecutorRequest>) -> Result<WasmExecutionContext, String> {
        let (tx_cmd, rx_cmd) = mpsc::channel::<ExecutorCommand>();
        let mut exec_thd = ExecutorThread::new(module, rx_cmd, tx_request, wasm);
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
pub enum ExecutorRequest {
    RegisterHandler{ stream: String, module: String, func_ptr: u32 },
    StreamUpdate{ stream: String, update: StreamUpdate },
}
unsafe impl Sync for ExecutorRequest {}

pub type ExecutorRequestSender = mpsc::Sender<ExecutorRequest>;

struct ExecutorThread {
    module_name: String,
    cmd_rx: mpsc::Receiver<ExecutorCommand>,
    req_tx: mpsc::Sender<ExecutorRequest>,
    instance: Option<wasmer_runtime::Instance>,
    wasm: WasmProgram,
}

impl ExecutorThread {

    fn new(module_name: String, cmd_rx: mpsc::Receiver<ExecutorCommand>, req_tx: mpsc::Sender<ExecutorRequest>, wasm: &WasmProgram) -> ExecutorThread {
        ExecutorThread{
            module_name,
            cmd_rx,
            req_tx,
            instance: None,
            wasm: wasm.clone(),
        }
    }

    fn run(&mut self) {
        let req_tx = self.req_tx.clone();
        let module_name = self.module_name.clone();

        let abort = |_: i32, _: i32, _: i32, _: i32| std::process::exit(-1);
        let log = move |ctx: &mut Ctx, ptr: WasmPtr<u16, Array>, len: u32| {
            let memory = ctx.memory(0);
            let string = Util::utf16_string(memory, ptr, len);
            println!("log: {}", string);
        };
        let stream_register = move |ctx: &mut Ctx, stream_name_ptr: WasmPtr<u16, Array>, stream_name_len: u32, fncptr: u32| {
            let memory = ctx.memory(0);
            let stream_name = Util::utf16_string(memory, stream_name_ptr, stream_name_len);

            req_tx.send(ExecutorRequest::RegisterHandler{
                    stream: stream_name.to_string(),
                    module: module_name.clone(), func_ptr: fncptr as u32});
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
            match cmd {
                ExecutorCommand::CallName(fn_name) => self.call_named_function(fn_name),
                ExecutorCommand::CallIndex(idx) => self.call_indexed_function(idx),
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

    fn call_indexed_function(&mut self, index: u32) {
        let table_idx = TableIndex::new(index as usize);
        let instance = self.instance.as_mut().unwrap();
        let ctx = instance.context_mut();
        ctx.call_with_table_index(table_idx, &[]);
    }

}

struct Util;
impl Util {
    pub fn utf16_string(mem: &wasmer_runtime::Memory, ptr: WasmPtr<u16, Array>, len: u32) -> String {
        let str_mem = ptr.deref(mem, 0, len).unwrap();

        let bytes: Vec<u16> = str_mem.iter().map(|x| x.clone().into_inner() as u16).collect();
        std::char::decode_utf16(bytes)
                    .map(|r| r.unwrap_or(' '))
                    .collect::<String>()
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

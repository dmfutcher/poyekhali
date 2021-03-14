use std::io;
use std::io::Read;
use std::fs::File;
use std::result::Result;
use std::thread;
use std::sync::{mpsc, Arc, Mutex};
use std::collections::HashMap;

use wasmer::{imports, Array, Function, FunctionType, Instance, Module, Memory, LazyInit, Store, Type, Value, WasmPtr, WasmerEnv};
// use wasmer_runtime_core::{structures::TypedIndex, types::TableIndex};

use crate::stream::StreamUpdate;

type WasmProgram = Box<[u8]>;

#[derive(Debug)]
struct FunctionIndex { module: String, func: u32 }

pub struct WasmRuntime{
    execution_contexts: Arc<Mutex<HashMap<String, WasmExecutionContext>>>,
    stream_handlers: Arc<Mutex<HashMap<String, Vec<FunctionIndex>>>>,
    request_servicer: Option<thread::JoinHandle<()>>,
    req_tx: RuntimeRequestSender,
}

impl WasmRuntime {

    pub fn new() -> WasmRuntime {
        let (tx, rx) = mpsc::channel::<RuntimeRequest>();
        let mut runtime = WasmRuntime{
            execution_contexts: Arc::new(Mutex::new(HashMap::new())),
            stream_handlers: Arc::new(Mutex::new(HashMap::new())),
            request_servicer: None,
            req_tx: tx,
        };

        let send_handlers = Arc::clone(&runtime.stream_handlers);
        let send_contexts = Arc::clone(&runtime.execution_contexts);
        runtime.request_servicer = Some(thread::spawn(|| RuntimeRequestServicer::request_servicer(rx, send_handlers, send_contexts)));

        runtime
    }

    pub fn init_module(&mut self, module_name: String, wasm: &WasmProgram) {
        let ctx = WasmExecutionContext::new(module_name.clone(), wasm, self.request_channel()).expect("Failed to init executionctx");
        ctx.initialise();

        self.execution_contexts.lock().unwrap().insert(module_name, ctx);
    }

    // TODO: Remove once in RuntimeRequestServicer
    // pub fn publish_stream_change(&self, stream: String) {
    //     let stream_handlers = self.stream_handlers.lock().unwrap();
    //     let contexts = self.execution_contexts.lock().unwrap();
    //     if let Some(handlers) = stream_handlers.get(&stream) {
    //         for func_idx in handlers {
    //             if let Some(exec_ctx) = contexts.get(&func_idx.module) {
    //                 exec_ctx.tx_cmd.send(ExecutorCommand::CallIndex(func_idx.func));
    //             }
    //         }
    //     }
    // }

    pub fn request_channel(&self) -> RuntimeRequestSender {
        self.req_tx.clone()
    }

    // pub fn stop(self) {
    //     let contexts = self.execution_contexts.lock().unwrap();
    //     for (_, ref c) in contexts.into_iter() {
    //         c.join();
    //     }
    // }

}

struct RuntimeRequestServicer {

}
impl RuntimeRequestServicer {

    fn request_servicer(rx: mpsc::Receiver<RuntimeRequest>, handlers: Arc<Mutex<HashMap<String, Vec<FunctionIndex>>>>, contexts: Arc<Mutex<HashMap<String, WasmExecutionContext>>>) {
        while let Ok(msg) = rx.recv() {
            match msg {
                RuntimeRequest::RegisterHandler{stream, module, func_ptr} => {
                    println!("Registering handler for stream {}, {}::{}", stream, module, func_ptr);
                    let func_idx = FunctionIndex{ module, func: func_ptr };

                    let mut h = handlers.lock().unwrap();
                    h.entry(stream).or_insert_with(Vec::new).push(func_idx);
                },
                RuntimeRequest::StreamUpdate{stream, update} => {
                    let stream_handlers = handlers.lock().unwrap();
                    let exec_contexts = contexts.lock().unwrap();
                    if let Some(handlers) = stream_handlers.get(&stream) {
                        for func_idx in handlers {
                            // HERE!!!!!!!!!!!!!!!!! need arc/mutex wrapped exec_ctxs here ...
                            if let Some(exec_ctx) = exec_contexts.get(&func_idx.module) {
                                exec_ctx.tx_cmd.send(ExecutorCommand::CallIndex(func_idx.func, update.clone()));
                            }
                        }
                    }
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

    fn new(module: String, wasm: &WasmProgram, tx_request: mpsc::Sender<RuntimeRequest>) -> Result<WasmExecutionContext, String> {
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
    CallIndex(u32, StreamUpdate),
}

#[derive(Debug)]
pub enum RuntimeRequest {
    RegisterHandler{ stream: String, module: String, func_ptr: u32 },
    StreamUpdate{ stream: String, update: StreamUpdate },
}
unsafe impl Sync for RuntimeRequest {}

pub type RuntimeRequestSender = mpsc::Sender<RuntimeRequest>;

struct ExecutorThread {
    module_name: String,
    cmd_rx: mpsc::Receiver<ExecutorCommand>,
    req_tx: mpsc::Sender<RuntimeRequest>,
    instance: Option<wasmer::Instance>,
    wasm: WasmProgram,
}

#[derive(WasmerEnv, Clone)]
struct FnEnv {
    #[wasmer(export)]
    memory: LazyInit<Memory>,
    module_name: String,
    req_tx: Arc<Mutex<mpsc::Sender<RuntimeRequest>>>,
}

impl ExecutorThread {

    fn new(module_name: String, cmd_rx: mpsc::Receiver<ExecutorCommand>, req_tx: mpsc::Sender<RuntimeRequest>, wasm: &WasmProgram) -> ExecutorThread {
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

        let store = Store::default();
        let module = Module::new(&store, self.wasm.clone()).unwrap();

        let abort_signature = FunctionType::new(vec![Type::I32, Type::I32, Type::I32, Type::I32], vec![]);
        let abort = Function::new(&store, &abort_signature, |_args| {
            std::process::exit(-1);
        });

        let env_req_tx = Arc::new(Mutex::new(req_tx));
        let env = FnEnv { memory: Default::default(), module_name: module_name.clone(), req_tx: env_req_tx  };
        let log = Function::new_native_with_env(&store, env.clone(), move |env: &FnEnv, base: WasmPtr<u16, Array>, len: u32| -> () {
            let memory = env.memory_ref().expect("memory not set up");
            let val = Util::utf16_string(memory, base, len);
            println!("{}", val);
        });
        
        let stream_register = Function::new_native_with_env(&store, env, move |env: &FnEnv, stream_name_ptr: WasmPtr<u16, Array>, stream_name_len: u32, fn_ptr: u32| {
            let stream_name = Util::utf16_string(env.memory_ref().expect("memory not set up"), stream_name_ptr, stream_name_len);
            let req_tx = env.req_tx.lock().unwrap();

            req_tx.send(RuntimeRequest::RegisterHandler{
                    stream: stream_name.to_string(),
                    module: env.module_name.clone(), func_ptr: fn_ptr as u32});
        });

        let import_object = imports! {
            "env" => {
                "abort" => abort,
            },
            "host" => {
                "log" => log,
            },
            "stream" => {
                "register" => stream_register,
            }
        };

        match Instance::new(&module, &import_object) {
            Ok(instance) => self.instance = Some(instance),
            Err(e) => println!("error: {}", e),
        };

        while let Ok(cmd) = self.cmd_rx.recv() {
            match cmd {
                ExecutorCommand::CallName(fn_name) => self.call_named_function(fn_name),
                ExecutorCommand::CallIndex(idx, update) => self.call_indexed_function(idx, update),
            }
        }
    }

    // TODO: How can we handle return values? Can we avoid needing to use return values?
    fn call_named_function(&self, fn_name: String) {
        if let Some(ref instance) = self.instance {
            let function = instance.exports.get_function(fn_name.as_str()).unwrap();
            function.call(&[]);
        }
    }

    // TODO: How to we generically pass in different stream updates? Object of some kind?
    //          Obect stored in a Global and pass the pointer to that into guest fns?
    fn call_indexed_function(&mut self, index: u32, update: StreamUpdate) {
        let arg = match update {
            StreamUpdate::TimerUpdate{ seconds } => seconds,
        };

        if let Some(ref instance) = self.instance {
            let t = instance.exports.get_table("table").unwrap();
            if let Some(Value::FuncRef(ref func)) = t.get(index) {
                let fn_native = func.native::<(i64), ()>();
                fn_native.unwrap().call(arg);
            }
        }
    }

}

struct Util;
impl Util {
    pub fn utf16_string(mem: &wasmer::Memory, ptr: WasmPtr<u16, Array>, len: u32) -> String {
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

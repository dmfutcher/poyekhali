use std::thread;

use crate::wasm::{WasmRuntime, ExecutorRequest, ExecutorRequestSender};

pub struct StreamManager {
    exec_request_chan: ExecutorRequestSender,
}

impl StreamManager {

    pub fn new(runtime: &WasmRuntime) -> StreamManager {
        StreamManager{ exec_request_chan: runtime.request_channel() }
    }

    pub fn start_streams(&mut self) {
        let timer_stream = Box::new(Timer::new());
        self.start_stream(timer_stream);
    }

    fn start_stream(&mut self, stream: Box<dyn StreamSource>) {
        let channel = self.exec_request_chan.clone();
        let static_ref: &'static mut dyn StreamSource = Box::leak(stream);
        thread::spawn(move || {
            static_ref.run(channel);
        });
    }
}

#[derive(Debug)]
pub enum StreamUpdate {
    TimerUpdate{ seconds: u64 },
}

trait StreamSource: Send + Sync + 'static {
    fn frequency(&self) -> u32;
    fn tick(&mut self, chan: &ExecutorRequestSender);
    
    fn run(&mut self, chan: ExecutorRequestSender) {
        let hertz = self.frequency();

        loop {
            self.tick(&chan);
            thread::sleep_ms(1000 / hertz);
        }
    }
}


struct Timer {
    seconds: u64,
}

impl Timer {}

impl Timer {
    fn new() -> Timer {
        Timer{ seconds: 0 }
    }
}

impl StreamSource for Timer {

    fn frequency(&self) -> u32 {
        1
    }

    fn tick(&mut self, chan: &ExecutorRequestSender) {
        let update = StreamUpdate::TimerUpdate{ seconds: self.seconds };
        self.seconds += 1;
        chan.send(ExecutorRequest::StreamUpdate{ stream: "timer:seconds".to_owned(), update });
    }

}
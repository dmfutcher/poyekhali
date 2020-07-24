
struct StreamController {
    streams: Vec<Stream>,
}

impl StreamController {

    pub fn new() -> StreamController {
        StreamController{}
    }
    
}

struct Stream {
    name: String,
}

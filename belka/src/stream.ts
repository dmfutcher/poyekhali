// We need to declare types of the `host` module exposed by our host
export declare namespace stream {
    @external("register")
    export function register(stream_name: String, stream_name_len: usize, fn: (i: i64) => void): void;
}

// Due to Assemblyscript data-interop limitations, it can be helpful to write an Assemblyscript shim in front of your
// host-exposed functions to abstract away any interop details.
export function register(stream_name: String, fn: (i: i64) => void): void {
    stream.register(stream_name, stream_name.length, fn);
}

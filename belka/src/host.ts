export declare namespace host {

    @external("log")
    export function log(s: String, len: usize): void;

}

// Due to Assemblyscript data-interop limitations, it can be helpful to write an Assemblyscript shim in front of your
// host-exposed functions to abstract away any interop details.
export function log(s: String): void {
    host.log(s, s.length)
}

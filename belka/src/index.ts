// The entry file of your WebAssembly module.

// Import our host-exposed `log` function
import { log } from './host';
import { register } from './stream';

function on_timer_tick(): void {
  log('in on_timer_tick');
}

export function init(): u8 {
  log("Hello, World!");
  register('timer:seconds', on_timer_tick);
  return 0;
}

import { log } from './host';
import { register } from './stream';

var countdownVal = 10;

export function test(i: i64): void {
  log('hello, world');
}

export function on_timer_tick(_: i64): void {
  if (countdownVal == 10) {
    log('Countdown initiated');
  } else if (countdownVal == 0) {
    log('Ignition start');
    abort();
  }
  
  log(countdownVal.toString());
  countdownVal -= 1;
}

export function init(): u8 {
  register('timer:seconds', on_timer_tick);
  log('initialised');
  return 0;
}

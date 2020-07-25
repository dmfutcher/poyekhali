import { log } from './host';
import { register } from './stream';

var countdownVal = 10;

function on_timer_tick(): void {
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
  return 0;
}

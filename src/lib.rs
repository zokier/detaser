#![feature(unboxed_closures,core,negate_unsigned)]
pub use self::taserconsumer::*;
pub use self::util::*;
pub use self::rowconsumer::*;
pub use self::types::*;

mod taserconsumer;
mod util;
mod rowconsumer;
mod types;

#[macro_use]
extern crate nom;

use nom::Consumer;

pub fn detaser<CallBackType: Fn(TaserRow) -> ()>(filename: &str, cb: CallBackType) {
    let mut prod = nom::FileProducer::new(filename, 4).unwrap();
    let mut cons = TaserConsumer::new(cb);
    cons.run(&mut prod);
    //TODO make proper destructor
    std::mem::forget(std::mem::replace(&mut cons.current_row, TaserRow { fields: Vec::new() }));
}

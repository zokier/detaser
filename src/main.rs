extern crate detaser;
extern crate comm;
extern crate nom;

use nom::Consumer;

fn main() {
    let (row_send, row_recv) = comm::spsc::bounded::new(10);
    let consumer_thread = std::thread::spawn(move || {
        let mut prod = nom::FileProducer::new("sample.tsr", 4).unwrap();
        let mut cons = detaser::TaserConsumer::new(move |row: detaser::TaserRow| { row_send.send_sync(row); });
        cons.run(&mut prod);
        //TODO make proper destructor
        std::mem::forget(std::mem::replace(&mut cons.current_row, detaser::TaserRow { fields: Vec::new() }));
    });
    let printer_thread = std::thread::spawn(move || {
        let mut done = false;
        while !done {
            match row_recv.recv_sync() {
                Ok(row) => println!("{:?}", row),
                Err(e) => done = true
            }
        }
    });
    consumer_thread.join();
    printer_thread.join();
}

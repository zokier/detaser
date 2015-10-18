extern crate detaser;
extern crate comm;

fn main() {
    let (row_send, row_recv) = comm::spsc::bounded::new(10);
    let consumer_thread = std::thread::spawn(move || {
        detaser::detaser("sample.tsr", move |row: detaser::TaserRow| { row_send.send_sync(row); });
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

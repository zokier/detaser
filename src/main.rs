#![feature(negate_unsigned,unboxed_closures,core)]

extern crate comm;

#[macro_use]
extern crate nom;
use std::str::from_utf8;
use nom::Consumer;

#[derive(Debug)]
enum TaserType { 
    FixedStr(u32),
    UInt(u32),
}

#[derive(Debug)]
enum TaserValue { 
    FixedStr(String),
    UInt(u32),
}

#[derive(Debug)]
struct TaserVersion {
    major: u32,
    minor: u32
}

#[derive(Debug)]
struct TaserHeader {
    name: String,
    ttype: TaserType,
}

#[derive(Debug)]
struct TaserRow {
    fields: Vec<TaserValue>
}

named!(magic_parser, tag!("TASR"));
named!(two_char_u32_parser<u32>, map_res!(take_str!(2), <u32 as std::str::FromStr>::from_str));
named!(version_parser<TaserVersion>, chain!(major: two_char_u32_parser ~ minor: two_char_u32_parser, || {TaserVersion{major: major, minor: minor}}));
named!(preamble_parser<(TaserVersion,u32)>, chain!(
        magic_parser
        ~ version: version_parser
        ~ header_count: call!(nom::le_u32),
        || { (version, header_count) } ));
named!(single_header_parser<TaserHeader>,
   chain!(
       name: take_str!(16) 
       ~ take!(4) 
       ~ ttype: alt!(tag!("STRN") | tag!("UINT"))
       ~ typeparam: call!(nom::le_u32),
       || { 
           TaserHeader { 
               name: name.trim_matches('\0').to_string(), 
               ttype: match ttype {
                   b"STRN" => TaserType::FixedStr(typeparam),
                   b"UINT" => TaserType::UInt(typeparam),
                   _ => panic!("Unmatched ttype"),
               }
           }
       }
   )
);

#[derive(Debug)]
struct RowConsumer<'a> {
    headers: &'a [TaserHeader],
    current_header_idx: usize,
    fields: Vec<TaserValue>
}

impl<'a> nom::Consumer for RowConsumer<'a> {
    fn consume(&mut self, input: &[u8]) -> nom::ConsumerState {
        if self.current_header_idx < self.headers.len() {
            match self.headers[self.current_header_idx].ttype {
                TaserType::FixedStr(len) => {
                    match take_str!(input, len) {
                        nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                        nom::IResult::Incomplete(n) => nom::ConsumerState::Await(0,len as usize),
                        nom::IResult::Done(_,s) => {
                            self.fields.push(TaserValue::FixedStr(s.trim_matches('\0').to_string()));
                            self.current_header_idx += 1;
                            nom::ConsumerState::Await(len as usize,0)
                        }
                    }
                },
                TaserType::UInt(len) => {
                    match nom::le_u32(input) {
                        nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                        nom::IResult::Incomplete(n) => nom::ConsumerState::Await(0,4),
                        nom::IResult::Done(_,i) => {
                            self.fields.push(TaserValue::UInt(i));
                            self.current_header_idx += 1;
                            nom::ConsumerState::Await(4,0)
                        }
                    }
                },
            }
        } else {
            nom::ConsumerState::ConsumerDone
        }
    }

    fn failed(&mut self, error_code: u32) {
        println!("row consumer failed with error code {}", error_code);
    }

    fn end(&mut self) {
    }
}

fn single_row_parser<'a>(input: &'a [u8], headers: &[TaserHeader], row_length: usize) -> nom::IResult<'a, &'a [u8], TaserRow> {
    match take!(input, row_length) {
        nom::IResult::Error(e) => nom::IResult::Error(e),
        nom::IResult::Incomplete(n) => nom::IResult::Incomplete(n),
        nom::IResult::Done(i,o) => {
            let mut row_consumer = RowConsumer { headers: headers, current_header_idx: 0, fields: Vec::new() };
            let mut row_producer = nom::MemProducer::new(o, 4);
            row_consumer.run(&mut row_producer);
            if row_consumer.fields.len() == headers.len() {
                nom::IResult::Done(i, TaserRow { fields: row_consumer.fields })
            } else {
                nom::IResult::Error(nom::Err::Code(1))
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Beginning,
    Headers,
    Rows,
    End,
}

struct TaserConsumer<CallBackType: Fn(TaserRow) -> ()> {
    state: State,
    version: TaserVersion,
    header_count: u32,
    headers: Vec<TaserHeader>,
    row_length: usize,
    row_callback: CallBackType,
}

impl<CallBackType: Fn(TaserRow) -> ()> TaserConsumer<CallBackType> {
    fn new(cb: CallBackType) -> TaserConsumer<CallBackType> {
        TaserConsumer {
            state: State::Beginning,
            version: TaserVersion{major:-1, minor:-1},
            header_count: 0,
            headers: Vec::new(),
            row_length: 0,
            row_callback: cb,
        }
    }
}

fn get_error_code(e: nom::Err) -> u32 {
    println!("get_error_code: {:?}", e);
    match e {
        nom::Err::Code(c) => c,
        nom::Err::Node(c, _) => c,
        nom::Err::Position(c, _) => c,
        nom::Err::NodePosition(c, _, _) => c
    }
}

impl<CallBackType: Fn(TaserRow) -> ()> nom::Consumer for TaserConsumer<CallBackType> {
    fn consume(&mut self, input: &[u8]) -> nom::ConsumerState {
        match self.state {
            State::Beginning => {
                match preamble_parser(input) {
                    nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                    nom::IResult::Incomplete(n) => nom::ConsumerState::Await(0,12),
                    nom::IResult::Done(_,(version,header_count)) => {
                        self.version = version;
                        self.header_count = header_count;
                        self.state = State::Headers;
                        nom::ConsumerState::Await(12, 0x1c)
                    }
                }
            },
            State::Headers => { 
                match single_header_parser(input) {
                    nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                    nom::IResult::Incomplete(n) => match n {
                        nom::Needed::Unknown => nom::ConsumerState::Await(0, 0x1c),
                        nom::Needed::Size(s) => nom::ConsumerState::Await(0, s)
                    },
                    nom::IResult::Done(_,header) => {
                        self.headers.push(header);
                        match (self.header_count as usize).cmp(&self.headers.len()) {
                            std::cmp::Ordering::Less => {
                                panic!("Header count mismatch! Expected {}, found {}", self.header_count, self.headers.len())
                            },
                            std::cmp::Ordering::Equal => {
                                self.row_length = self.headers.iter().fold(0, |acc, ref hdr| {
                                    match hdr.ttype {
                                        TaserType::FixedStr(len) => acc+len as usize,
                                        TaserType::UInt(len) => acc+len as usize,
                                    }
                                });
                                self.state = State::Rows;
                                nom::ConsumerState::Await(0x1c, self.row_length)
                            },
                            std::cmp::Ordering::Greater => {
                                nom::ConsumerState::Await(0x1c, 0x1c)
                            },
                        }
                    }
                }
            },
            State::Rows => { 
                match single_row_parser(input, &self.headers, self.row_length) {
                    nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                    nom::IResult::Incomplete(n) => nom::ConsumerState::Await(0,self.row_length),
                    nom::IResult::Done(_,row) => {
                        self.row_callback.call((row,));
                        nom::ConsumerState::Await(self.row_length, self.row_length)
                    }
                }
            },
            State::End => { 
                nom::ConsumerState::ConsumerDone
            }
        }
    }

    fn failed(&mut self, error_code: u32) {
        println!("failed with error code {}, state: {:?}, headers: {:?}", error_code, self.state, self.headers);
    }

    fn end(&mut self) {
        println!("EOF");
    }
}

fn main() {
    let (row_send, row_recv) = comm::spsc::bounded::new(10);
    let consumer_thread = std::thread::spawn(move || {
        let mut prod = nom::FileProducer::new("sample.tsr", 4).unwrap();
        let mut cons = TaserConsumer::new(move |row: TaserRow| { row_send.send_sync(row); });
        cons.run(&mut prod);
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

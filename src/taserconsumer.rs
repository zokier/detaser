extern crate nom;
extern crate std;
use std::str::from_utf8;
use super::*;

named!(magic_parser, tag!("TASR"));
named!(two_char_u32_parser<u32>, map_res!(
        take_str!(2), <u32 as std::str::FromStr>::from_str));
named!(version_parser<TaserVersion>, chain!(
        major: two_char_u32_parser
        ~ minor: two_char_u32_parser,
        || { TaserVersion { major: major, minor: minor } }));
named!(preamble_parser<(TaserVersion,u32)>, chain!(
        magic_parser
        ~ version: version_parser
        ~ header_count: call!(nom::le_u32),
        || { (version, header_count) } ));
named!(single_header_parser<TaserHeader>,
   chain!(
       name: take_str!(16) 
       ~ take!(4) 
       ~ ttype: alt!(tag!("STRN") | tag!("UINT") | tag!("IINT"))
       ~ typeparam: call!(nom::le_u32),
       || { 
           TaserHeader { 
               name: name.trim_matches('\0').to_string(), 
               ttype: match ttype {
                   b"STRN" => {
                       if typeparam == 0 {
                           TaserType::VarStr
                       } else {
                           TaserType::FixedStr(typeparam)
                       }
                   },
                   b"UINT" => TaserType::UInt(typeparam),
                   b"IINT" => TaserType::Int(typeparam),
                   _ => panic!("Unmatched ttype"),
               }
           }
       }
   )
);

#[derive(Debug)]
enum State {
    Beginning,
    Headers,
    Rows,
    RowBlobs,
    End,
}

pub struct TaserConsumer<CallBackType: Fn(TaserRow) -> ()> {
    state: State,
    version: TaserVersion,
    header_count: u32,
    headers: Vec<TaserHeader>,
    row_length: usize,
    row_callback: CallBackType,
    pub current_row: TaserRow, //TODO remove pub
}

impl<CallBackType: Fn(TaserRow) -> ()> TaserConsumer<CallBackType> {
    pub fn new(cb: CallBackType) -> TaserConsumer<CallBackType> {
        TaserConsumer {
            state: State::Beginning,
            version: TaserVersion{major:-1, minor:-1},
            header_count: 0,
            headers: Vec::new(),
            row_length: 0,
            row_callback: cb,
            current_row: unsafe { std::mem::uninitialized() },
        }
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
                                        TaserType::VarStr => acc+16,
                                        TaserType::UInt(len) => acc+len as usize,
                                        TaserType::Int(len) => acc+len as usize,
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
                        // self.current_row is std::mem::uninitialized() at this stage
                        std::mem::forget(std::mem::replace(&mut self.current_row, row));
                        let mut blobs_total_len: usize = 0;
                        for field in &self.current_row.fields {
                            if let &TaserValue::VarStr(VarStr::Position((pos,len))) = field {
                                blobs_total_len = (pos+len) as usize;
                            }
                        }
                        self.state = State::RowBlobs;
                        nom::ConsumerState::Await(self.row_length, blobs_total_len)
                    }
                }
            },
            State::RowBlobs => {
                let mut blobs_total_len: usize = 0;
                for field in &mut self.current_row.fields {
                    if let &mut TaserValue::VarStr(VarStr::Position((pos,len))) = field {
                        match std::str::from_utf8(&input[pos as usize..(pos+len) as usize]) {
                            Ok(s) => std::mem::replace(field, TaserValue::VarStr(VarStr::Collected(s.to_string()))),
                            Err(e) => return nom::ConsumerState::ConsumerError(6)
                        };
                        blobs_total_len = (pos+len) as usize;
                    }
                }
                // self.current_row will be initialized when self.state == State::Rows
                let row = std::mem::replace(&mut self.current_row, unsafe { std::mem::uninitialized() });
                self.row_callback.call((row,));
                self.state = State::Rows;
                nom::ConsumerState::Await(blobs_total_len, self.row_length)
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


extern crate nom;
extern crate std;
use nom::Consumer;
use std::str::from_utf8;
use super::*;

pub fn single_row_parser<'a>(input: &'a [u8], headers: &[TaserHeader], row_length: usize) -> nom::IResult<'a, &'a [u8], TaserRow> {
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
                println!("Wrong field count, row: {:?}", row_consumer.fields);
                nom::IResult::Error(nom::Err::Code(3))
            }
        }
    }
}

fn inline_varstr_parser(input: &[u8]) -> nom::IResult<&[u8], VarStr> {
    if input.len() < 16 {
        nom::IResult::Incomplete(nom::Needed::Size(16))
    } else {
        if (input[0] & 0b1000_0000) == 0 {
            chain!(input,
                   pos: call!(nom::le_u64)
                   ~ len: call!(nom::le_u64),
                   || VarStr::Position((pos,len))
            )
        } else {
            let len = input[0] & !0b1000_0000;
            if len < 16 {
                match std::str::from_utf8(&input[1..(len+1) as usize]) {
                    Ok(s) => nom::IResult::Done(&input[16..], VarStr::Collected(s.to_string())),
                    Err(e) => nom::IResult::Error(nom::Err::Code(3))
                }
            } else {
                nom::IResult::Error(nom::Err::Code(2))
            }
        }
    }
}


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
                TaserType::VarStr => {
                    match inline_varstr_parser(input) {
                        nom::IResult::Error(a) => nom::ConsumerState::ConsumerError(get_error_code(a)),
                        nom::IResult::Incomplete(n) => nom::ConsumerState::Await(0, 16),
                        nom::IResult::Done(_,s) => {
                            self.fields.push(TaserValue::VarStr(s));
                            self.current_header_idx += 1;
                            nom::ConsumerState::Await(16, 0)
                        }
                    }
                },
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



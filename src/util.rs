extern crate nom;

pub fn get_error_code(e: nom::Err) -> u32 {
    println!("get_error_code: {:?}", e);
    match e {
        nom::Err::Code(c) => c,
        nom::Err::Node(c, _) => c,
        nom::Err::Position(c, _) => c,
        nom::Err::NodePosition(c, _, _) => c
    }
}


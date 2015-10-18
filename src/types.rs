#[derive(Debug)]
pub enum TaserType { 
    FixedStr(u32),
    VarStr,
    UInt(u32),
}

#[derive(Debug)]
pub enum TaserValue { 
    FixedStr(String),
    VarStr(VarStr),
    UInt(u32),
}

#[derive(Debug)]
pub struct TaserVersion {
    pub major: u32,
    pub minor: u32
}

#[derive(Debug)]
pub struct TaserHeader {
    pub name: String,
    pub ttype: TaserType,
}

#[derive(Debug)]
pub struct TaserRow {
    pub fields: Vec<TaserValue>
}

#[derive(Debug)]
pub enum VarStr {
    Position((u64,u64)),
    Collected(String),
}



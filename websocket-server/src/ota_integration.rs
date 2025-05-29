#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Instruction {
    #[prost(uint64, tag="1")]
    pub property_id: u64,
    #[prost(uint32, tag="2")]
    pub message_id: u32,
    #[prost(uint64, tag="3")]
    pub somecnt1: u64,
    #[prost(uint64, tag="4")]
    pub somecnt2: u64,
    #[prost(uint64, tag="5")]
    pub somecnt3: u64,
    #[prost(uint64, tag="6")]
    pub somecnt4: u64,
}
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InstructionBatch {
    #[prost(message, repeated, tag="1")]
    pub instructions: ::prost::alloc::vec::Vec<Instruction>,
    #[prost(bytes="vec", tag="2")]
    pub signature: ::prost::alloc::vec::Vec<u8>,
}

pub mod usage_example;

use num_enum::TryFromPrimitive;
///In this example we will create structure type for our other examples
use std::any::{Any, TypeId};
use std::hash::{DefaultHasher, Hash, Hasher};
use tfserver::{impl_strong_type, impl_structure_type};
use tfserver::rkyv::{Archive, Deserialize, Serialize};
use tfserver::structures::s_type::{StrongType, StructureType};

///the repr statement, must be used, can be any unsigned integer including up to u64
#[repr(u8)]
///Keep this derives as default, they will help us in implementing StructureType trait for this enum, the debug is optional for sure
#[derive(Serialize, Deserialize, PartialEq, Clone, Hash, Eq, TryFromPrimitive, Copy, Debug, Archive)]
pub enum ExampleSType {
    TestMessage,
    TestResponse,
    ExpensiveMessage,
    ExpensiveResponse,
    ManualHandlerRequest,
}





impl_structure_type!(
    ExampleSType, ArchivedExampleSType,
    TestMessage => (TestMsg, ArchivedTestMsg),
    TestResponse => (TestResponse, ArchivedTestResponse),
    ExpensiveMessage => (ExpensiveMsg, ArchivedExpensiveMsg),
    ExpensiveResponse => (ExpensiveResponse, ArchivedExpensiveResponse),
    ManualHandlerRequest => (ManualHandlerRequest, ArchivedManualHandlerRequest)
);

impl_strong_type!(
    TestMsg => ArchivedTestMsg,
    TestResponse => ArchivedTestResponse,
    ExpensiveMsg => ArchivedExpensiveMsg,
    ExpensiveResponse => ArchivedExpensiveResponse,
    ManualHandlerRequest => ArchivedManualHandlerRequest
);

///In this part we define our structures, each structure must have it's own s_type field
#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct TestMsg {
    pub s_type: ExampleSType,
    pub id: u64,
    pub data: Vec<u8>,
    pub message: String,
}
#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct TestResponse {
    pub s_type: ExampleSType,
    pub id: u64,
    pub data: Vec<u8>,
    pub another_message: String,
}
#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct ExpensiveMsg {
    pub s_type: ExampleSType,
    pub id: u64,
    pub data: Vec<u8>,
}
#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct ExpensiveResponse {
    pub s_type: ExampleSType,
    pub id: u64,
    pub data: Vec<u8>,
}
#[derive(Serialize, Deserialize, Debug, Archive)]
pub struct ManualHandlerRequest {
    s_type: ExampleSType,
}


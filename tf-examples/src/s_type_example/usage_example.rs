use crate::s_type_example::{ArchivedExpensiveMsg, ArchivedExpensiveResponse, ArchivedTestMsg, ArchivedTestResponse, ExampleSType, ExpensiveMsg, ExpensiveResponse, TestMsg, TestResponse};
use tfserver::structures::s_type;

pub fn try_serialize_structures() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>){
    let test1 = TestMsg {
        s_type: ExampleSType::TestMessage,
        id: 532,
        data: vec![],
        message: "hello message".to_string(),
    };
    let test1 = s_type::to_vec(&test1).expect("Should serialize");
    let test2 = TestResponse {
        s_type: ExampleSType::TestResponse,
        id: 34,
        data: vec![32, 32, 12, 32],
        another_message: "test response".to_string(),
    };
    let test2 = s_type::to_vec(&test2).expect("Should serialize");
    let test3 = ExpensiveMsg {
        s_type: ExampleSType::ExpensiveMessage,
        id: 23,
        data: vec![18u8; 2048],
    };
    let test3 = s_type::to_vec(&test3).expect("Should serialize");
    let test4 = ExpensiveResponse {
        s_type: ExampleSType::ExpensiveResponse,
        id: 23,
        data: vec![18u8; 4096],
    };
    let test4 = s_type::to_vec(&test4).expect("Should serialize");

    println!(
        "{}, {}, {}, {}",
        test1.len(),
        test2.len(),
        test3.len(),
        test4.len()
    );
    (test1, test2, test3, test4)
}

pub fn try_deserialize_structures(structures: (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)){
    let test1 = s_type::access::<TestMsg>(&structures.0).expect("Should be able to deserialize");
    let test2= s_type::access::<TestResponse>(&structures.1).expect("Should be able to deserialize");
    let test3 = s_type::access::<ExpensiveMsg>(&structures.2).expect("Should be able to deserialize");
    let test4 = s_type::access::<ExpensiveResponse>(&structures.3).expect("Should be able to deserialize");
  //  println!("{:?}, {:?}, {:?}, {:?}", test1, test2, test3, test4);
}
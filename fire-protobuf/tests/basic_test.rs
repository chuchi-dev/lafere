use std::fmt::Debug;
use std::collections::HashMap;

use fire_protobuf::{EncodeMessage, DecodeMessage};
use fire_protobuf::encode::EncodeMessage;
use fire_protobuf::decode::DecodeMessage;


#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
struct Test1 {
	#[field(1)]
	s: String,
	#[field(5)]
	some_struct: Test2
}

#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
struct Test2 {
	#[field(1)]
	nums: Vec<u32>,
	#[field(2)]
	compl_enum: Test3,
	#[field(4)]
	tupls: (u32, u64),
	#[field(6)]
	map: HashMap<String, Test4>,
	#[field(200)]
	test4: Test4
}

/// is internaly represented as
/// message {
/// 	oneof inner {
///			String one = 1;
/// 	}
/// }
#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
enum Test3 {
	#[field(1)]
	One(String),
	#[field(2, default)]
	Two
}

#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
#[repr(i32)]
enum Test4 {
	Unknown = 0,
	One = 1,
	Two = 2
}

#[derive(Debug, PartialEq, Eq, EncodeMessage, DecodeMessage)]
struct Test5 {}


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test5() {
		let mut test5 = Test5 {};
		let bytes = test5.write_to_bytes().unwrap();
		assert!(bytes.is_empty());
		let n_test5 = Test5::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test5, n_test5);
	}

	#[test]
	fn test3() {
		let mut test3 = Test3::Two;
		let bytes = test3.write_to_bytes().unwrap();
		let n_test3 = Test3::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test3, n_test3);

		let mut test3 = Test3::One("hello World".into());
		let bytes = test3.write_to_bytes().unwrap();
		let n_test3 = Test3::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test3, n_test3);

		let mut test3 = Test3::Two;
		let bytes = test3.write_to_bytes().unwrap();
		let n_test3 = Test3::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test3, n_test3);
	}

	#[test]
	fn test2() {
		let mut test2 = Test2 {
			nums: (0..10).collect(),
			compl_enum: Test3::One("hello World".into()),
			tupls: (10, 20),
			map: [
				("abc".to_string(), Test4::One),
				("bcde".to_string(), Test4::Two)
			].into_iter().collect(),
			test4: Test4::One
		};
		let bytes = test2.write_to_bytes().unwrap();
		let n_test2 = Test2::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test2, n_test2);
	}

	#[test]
	fn test1() {
		let mut test1 = Test1 {
			s: "Hello World".into(),
			some_struct: Test2 {
				nums: (0..10).collect(),
				compl_enum: Test3::Two,
				tupls: (10, 20),
				map: HashMap::new(),
				test4: Test4::Two
			}
		};
		let bytes = test1.write_to_bytes().unwrap();
		let n_test1 = Test1::parse_from_bytes(&bytes).unwrap();
		assert_eq!(test1, n_test1);
	}
}